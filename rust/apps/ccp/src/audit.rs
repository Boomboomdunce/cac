use anyhow::{Context, Result};
use claude_adapter::{claude_adapter, ADAPTER_NAME};
use core::{proxy_host_port, Profile, TargetAdapter};
use doctor::{CheckResult, DoctorReport};
use launcher::builder::{AdapterLaunchPolicy, LaunchPlanBuilder};
use serde::Deserialize;
use std::{env, path::Path, process};
use store::{
    certificate_material, ensure_runtime_shims, load_profile_identity, mitm_certificate_material,
    ProfileStore, StateLayout,
};

const CHECK_NAME: &str = "runtime live self-audit";

pub fn augment_doctor_report_with_live_runtime_audit(
    report: &mut DoctorReport,
    root: &Path,
    profile_name: &str,
) {
    let check = match live_runtime_audit_check(root, profile_name) {
        Ok(check) => check,
        Err(err) => CheckResult::warning(
            CHECK_NAME,
            Some(format!("failed to prepare live audit: {err}")),
        ),
    };
    report.add_check(check);
}

pub fn live_runtime_audit_check(root: &Path, profile_name: &str) -> Result<CheckResult> {
    let layout = StateLayout::new(root.to_path_buf()).context("initializing state layout")?;
    let store = ProfileStore::new(layout.clone());
    let profile = store
        .load_profile(profile_name)
        .context("loading profile for live audit")?;

    if !profile.adapter.eq_ignore_ascii_case(ADAPTER_NAME) {
        return Ok(CheckResult::warning(
            CHECK_NAME,
            Some(format!(
                "no live runtime audit implemented for adapter '{}'",
                profile.adapter
            )),
        ));
    }

    let (adapter, adapter_policy) = match resolve_live_audit_adapter_policy(&profile, &layout) {
        Ok(parts) => parts,
        Err(err) => {
            return Ok(CheckResult::warning(
                CHECK_NAME,
                Some(format!("unable to assemble runtime environment: {err}")),
            ));
        }
    };

    let execution = match LaunchPlanBuilder::new()
        .profile(profile)
        .adapter(adapter)
        .adapter_policy(adapter_policy)
        .command(vec![
            "node".to_string(),
            "-e".to_string(),
            live_audit_node_script().to_string(),
        ])
        .build()
    {
        Ok(execution) => execution,
        Err(err) => {
            return Ok(CheckResult::warning(
                CHECK_NAME,
                Some(format!("unable to build wrapped node launch plan: {err}")),
            ));
        }
    };

    let output = match execute_with_output(&execution) {
        Ok(output) => output,
        Err(err) => {
            return Ok(CheckResult::warning(
                CHECK_NAME,
                Some(format!("unable to execute wrapped node audit: {err}")),
            ));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("wrapped node exited with status {}", output.status)
        };
        return Ok(CheckResult::error(CHECK_NAME, Some(detail)));
    }

    let payload: LiveAuditPayload =
        serde_json::from_slice(&output.stdout).context("parsing live audit payload")?;
    let problems = validate_live_audit_payload(&payload);
    if problems.is_empty() {
        Ok(CheckResult::ok(
            CHECK_NAME,
            Some("wrapped node launch confirmed env hardening, Node proxying, and DNS blocking".to_string()),
        ))
    } else {
        Ok(CheckResult::error(CHECK_NAME, Some(problems.join("; "))))
    }
}

fn resolve_live_audit_adapter_policy(
    profile: &Profile,
    layout: &StateLayout,
) -> Result<(TargetAdapter, AdapterLaunchPolicy)> {
    let identity = load_profile_identity(layout, &profile.name).context("loading identity")?;
    let cert_material = certificate_material(layout, &profile.name);
    let mitm_material = mitm_certificate_material(layout);
    let missing_certificates = [
        ("CA cert", cert_material.ca_cert.as_path()),
        ("CA key", cert_material.ca_key.as_path()),
        ("client cert", cert_material.client_cert.as_path()),
        ("client key", cert_material.client_key.as_path()),
        ("MITM CA cert", mitm_material.ca_cert.as_path()),
        ("MITM CA key", mitm_material.ca_key.as_path()),
        ("Node CA bundle", mitm_material.node_ca_bundle.as_path()),
    ]
    .into_iter()
    .filter_map(|(label, path)| (!path.is_file()).then_some(label))
    .collect::<Vec<_>>();
    if !missing_certificates.is_empty() {
        anyhow::bail!(
            "missing certificate files: {}",
            missing_certificates.join(", ")
        );
    }

    let shims = ensure_runtime_shims(layout).context("materializing runtime shims")?;
    let runtime_hook = layout.hooks_dir().join("claude-preload.js");
    let blocked_hosts_path = layout.config_dir().join("blocked_hosts");
    let missing_runtime_assets = [runtime_hook.as_path(), blocked_hosts_path.as_path()]
        .into_iter()
        .filter(|path| !path.is_file())
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if !missing_runtime_assets.is_empty() {
        anyhow::bail!(
            "missing runtime assets: {}",
            missing_runtime_assets.join(", ")
        );
    }

    let adapter = claude_adapter();
    let mut policy = AdapterLaunchPolicy::new().with_runtime_hook_path(runtime_hook);
    for (key, value) in adapter.environment_overrides() {
        policy = policy.with_env_override(key.clone(), value.clone());
    }
    for key in adapter.environment_unsets() {
        policy = policy.with_env_unset(key.clone());
    }
    policy = with_identity_environment(policy, &identity, &shims.dir)
        .context("building identity environment")?;
    policy = policy.with_env_override("HOSTALIASES", blocked_hosts_path.display().to_string());
    policy = with_mtls_environment(policy, profile, &cert_material, &mitm_material);

    Ok((adapter.target_adapter().clone(), policy))
}

fn with_mtls_environment(
    mut policy: AdapterLaunchPolicy,
    profile: &Profile,
    cert_material: &store::CertificateMaterial,
    mitm_material: &store::MitmCertificateMaterial,
) -> AdapterLaunchPolicy {
    policy = policy
        .with_env_override(
            "CCP_MTLS_CERT",
            cert_material.client_cert.display().to_string(),
        )
        .with_env_override(
            "CCP_MTLS_KEY",
            cert_material.client_key.display().to_string(),
        )
        .with_env_override("CCP_MTLS_CA", cert_material.ca_cert.display().to_string())
        .with_env_override(
            "CAC_MTLS_CERT",
            cert_material.client_cert.display().to_string(),
        )
        .with_env_override(
            "CAC_MTLS_KEY",
            cert_material.client_key.display().to_string(),
        )
        .with_env_override("CAC_MTLS_CA", cert_material.ca_cert.display().to_string())
        .with_env_override(
            "NODE_EXTRA_CA_CERTS",
            mitm_material.node_ca_bundle.display().to_string(),
        );

    if let Some(proxy_url) = profile.policy.proxy_url() {
        if let Some(proxy_host_port) = proxy_host_port(proxy_url) {
            policy = policy
                .with_env_override("CCP_PROXY_HOST", proxy_host_port.clone())
                .with_env_override("CAC_PROXY_HOST", proxy_host_port);
        }
    }

    policy
}

fn with_identity_environment(
    mut policy: AdapterLaunchPolicy,
    identity: &store::ProfileIdentity,
    shim_dir: &std::path::Path,
) -> Result<AdapterLaunchPolicy> {
    let path_value = prepend_to_path(shim_dir)?;
    policy = policy
        .with_env_override("HOSTNAME", identity.hostname.clone())
        .with_env_override("COMPUTERNAME", identity.hostname.clone())
        .with_env_override("TZ", identity.tz.clone())
        .with_env_override("LANG", identity.lang.clone())
        .with_env_override("CCP_FAKE_HOSTNAME", identity.hostname.clone())
        .with_env_override("CCP_FAKE_MACHINE_ID", identity.machine_id.clone())
        .with_env_override("CCP_FAKE_PLATFORM_UUID", identity.uuid.clone())
        .with_env_override("CCP_FAKE_MAC_ADDRESS", identity.mac_address.clone())
        .with_env_override("PATH", path_value);

    Ok(policy)
}

fn prepend_to_path(path: &std::path::Path) -> Result<String> {
    let mut paths = vec![path.to_path_buf()];
    if let Some(existing) = env::var_os("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    let joined = env::join_paths(paths).context("joining runtime shim path")?;
    Ok(joined.to_string_lossy().into_owned())
}

fn execute_with_output(
    execution: &launcher::builder::LaunchPlanExecution,
) -> Result<process::Output, launcher::builder::LaunchError> {
    let mut command_iter = execution.command.iter();
    let program = match command_iter.next() {
        Some(cmd) => cmd,
        None => return Err(launcher::builder::LaunchError::MissingCommand),
    };

    let mut cmd = process::Command::new(program);
    cmd.args(command_iter);
    for key in execution.env_plan.removals() {
        cmd.env_remove(key);
    }
    for (key, value) in execution.env_plan.iter() {
        cmd.env(key, value);
    }

    cmd.output()
        .map_err(launcher::builder::LaunchError::Execution)
}

fn live_audit_node_script() -> &'static str {
    r#"
const dns = require('dns');
const has = (key) => Object.prototype.hasOwnProperty.call(process.env, key);
const payload = {
  CLAUDE_CODE_ENABLE_TELEMETRY: has('CLAUDE_CODE_ENABLE_TELEMETRY') ? process.env.CLAUDE_CODE_ENABLE_TELEMETRY : null,
  NODE_USE_ENV_PROXY: has('NODE_USE_ENV_PROXY') ? process.env.NODE_USE_ENV_PROXY : null,
  DO_NOT_TRACK: has('DO_NOT_TRACK') ? process.env.DO_NOT_TRACK : null,
  OTEL_SDK_DISABLED: has('OTEL_SDK_DISABLED') ? process.env.OTEL_SDK_DISABLED : null,
  OTEL_TRACES_EXPORTER: has('OTEL_TRACES_EXPORTER') ? process.env.OTEL_TRACES_EXPORTER : null,
  OTEL_METRICS_EXPORTER: has('OTEL_METRICS_EXPORTER') ? process.env.OTEL_METRICS_EXPORTER : null,
  OTEL_LOGS_EXPORTER: has('OTEL_LOGS_EXPORTER') ? process.env.OTEL_LOGS_EXPORTER : null,
  SENTRY_DSN: has('SENTRY_DSN') ? process.env.SENTRY_DSN : null,
  DISABLE_ERROR_REPORTING: has('DISABLE_ERROR_REPORTING') ? process.env.DISABLE_ERROR_REPORTING : null,
  DISABLE_BUG_COMMAND: has('DISABLE_BUG_COMMAND') ? process.env.DISABLE_BUG_COMMAND : null,
  CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: has('CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC') ? process.env.CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC : null,
  TELEMETRY_DISABLED: has('TELEMETRY_DISABLED') ? process.env.TELEMETRY_DISABLED : null,
  DISABLE_TELEMETRY: has('DISABLE_TELEMETRY') ? process.env.DISABLE_TELEMETRY : null,
  CCP_RUNTIME_HOOK: has('CCP_RUNTIME_HOOK') ? process.env.CCP_RUNTIME_HOOK : null,
  HOSTALIASES: has('HOSTALIASES') ? process.env.HOSTALIASES : null,
  ANTHROPIC_BASE_URL: has('ANTHROPIC_BASE_URL') ? process.env.ANTHROPIC_BASE_URL : null,
  ANTHROPIC_AUTH_TOKEN: has('ANTHROPIC_AUTH_TOKEN') ? process.env.ANTHROPIC_AUTH_TOKEN : null,
  ANTHROPIC_API_KEY: has('ANTHROPIC_API_KEY') ? process.env.ANTHROPIC_API_KEY : null,
};

dns.lookup('statsig.anthropic.com', (err) => {
  payload.dnsErrorCode = err && err.code ? err.code : null;
  payload.dnsBlocked = payload.dnsErrorCode === 'ECONNREFUSED';
  console.log(JSON.stringify(payload));
});
"#
}

fn validate_live_audit_payload(payload: &LiveAuditPayload) -> Vec<String> {
    let mut problems = Vec::new();

    if payload.claude_code_enable_telemetry.as_deref() != Some("") {
        problems.push("CLAUDE_CODE_ENABLE_TELEMETRY is not cleared".to_string());
    }
    if payload.node_use_env_proxy.as_deref() != Some("1") {
        problems.push("NODE_USE_ENV_PROXY is not 1".to_string());
    }
    if payload.do_not_track.as_deref() != Some("1") {
        problems.push("DO_NOT_TRACK is not 1".to_string());
    }
    if payload.otel_sdk_disabled.as_deref() != Some("true") {
        problems.push("OTEL_SDK_DISABLED is not true".to_string());
    }
    if payload.otel_traces_exporter.as_deref() != Some("none") {
        problems.push("OTEL_TRACES_EXPORTER is not none".to_string());
    }
    if payload.otel_metrics_exporter.as_deref() != Some("none") {
        problems.push("OTEL_METRICS_EXPORTER is not none".to_string());
    }
    if payload.otel_logs_exporter.as_deref() != Some("none") {
        problems.push("OTEL_LOGS_EXPORTER is not none".to_string());
    }
    if payload.sentry_dsn.as_deref() != Some("") {
        problems.push("SENTRY_DSN is not cleared".to_string());
    }
    if payload.disable_error_reporting.as_deref() != Some("1") {
        problems.push("DISABLE_ERROR_REPORTING is not 1".to_string());
    }
    if payload.disable_bug_command.as_deref() != Some("1") {
        problems.push("DISABLE_BUG_COMMAND is not 1".to_string());
    }
    if payload.claude_code_disable_nonessential_traffic.as_deref() != Some("1") {
        problems.push("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC is not 1".to_string());
    }
    if payload.telemetry_disabled.as_deref() != Some("1") {
        problems.push("TELEMETRY_DISABLED is not 1".to_string());
    }
    if payload.disable_telemetry.as_deref() != Some("1") {
        problems.push("DISABLE_TELEMETRY is not 1".to_string());
    }
    if payload
        .ccp_runtime_hook
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        problems.push("CCP_RUNTIME_HOOK is missing".to_string());
    }
    if payload
        .hostaliases
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        problems.push("HOSTALIASES is missing".to_string());
    }
    if payload.anthropic_base_url.is_some() {
        problems.push("ANTHROPIC_BASE_URL should be unset".to_string());
    }
    if payload.anthropic_auth_token.is_some() {
        problems.push("ANTHROPIC_AUTH_TOKEN should be unset".to_string());
    }
    if payload.anthropic_api_key.is_some() {
        problems.push("ANTHROPIC_API_KEY should be unset".to_string());
    }
    if !payload.dns_blocked {
        let code = payload.dns_error_code.as_deref().unwrap_or("none");
        problems.push(format!(
            "dns.lookup(statsig.anthropic.com) was not blocked with ECONNREFUSED (got {code})"
        ));
    }

    problems
}

#[derive(Deserialize)]
struct LiveAuditPayload {
    #[serde(rename = "CLAUDE_CODE_ENABLE_TELEMETRY")]
    claude_code_enable_telemetry: Option<String>,
    #[serde(rename = "NODE_USE_ENV_PROXY")]
    node_use_env_proxy: Option<String>,
    #[serde(rename = "DO_NOT_TRACK")]
    do_not_track: Option<String>,
    #[serde(rename = "OTEL_SDK_DISABLED")]
    otel_sdk_disabled: Option<String>,
    #[serde(rename = "OTEL_TRACES_EXPORTER")]
    otel_traces_exporter: Option<String>,
    #[serde(rename = "OTEL_METRICS_EXPORTER")]
    otel_metrics_exporter: Option<String>,
    #[serde(rename = "OTEL_LOGS_EXPORTER")]
    otel_logs_exporter: Option<String>,
    #[serde(rename = "SENTRY_DSN")]
    sentry_dsn: Option<String>,
    #[serde(rename = "DISABLE_ERROR_REPORTING")]
    disable_error_reporting: Option<String>,
    #[serde(rename = "DISABLE_BUG_COMMAND")]
    disable_bug_command: Option<String>,
    #[serde(rename = "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")]
    claude_code_disable_nonessential_traffic: Option<String>,
    #[serde(rename = "TELEMETRY_DISABLED")]
    telemetry_disabled: Option<String>,
    #[serde(rename = "DISABLE_TELEMETRY")]
    disable_telemetry: Option<String>,
    #[serde(rename = "CCP_RUNTIME_HOOK")]
    ccp_runtime_hook: Option<String>,
    #[serde(rename = "HOSTALIASES")]
    hostaliases: Option<String>,
    #[serde(rename = "ANTHROPIC_BASE_URL")]
    anthropic_base_url: Option<String>,
    #[serde(rename = "ANTHROPIC_AUTH_TOKEN")]
    anthropic_auth_token: Option<String>,
    #[serde(rename = "ANTHROPIC_API_KEY")]
    anthropic_api_key: Option<String>,
    #[serde(rename = "dnsErrorCode")]
    dns_error_code: Option<String>,
    #[serde(rename = "dnsBlocked", default)]
    dns_blocked: bool,
}

#[cfg(test)]
mod tests {
    use super::{validate_live_audit_payload, LiveAuditPayload};

    #[test]
    fn validate_live_audit_payload_flags_missing_node_env_proxy() {
        let payload = LiveAuditPayload {
            claude_code_enable_telemetry: Some(String::new()),
            node_use_env_proxy: None,
            do_not_track: Some("1".to_string()),
            otel_sdk_disabled: Some("true".to_string()),
            otel_traces_exporter: Some("none".to_string()),
            otel_metrics_exporter: Some("none".to_string()),
            otel_logs_exporter: Some("none".to_string()),
            sentry_dsn: Some(String::new()),
            disable_error_reporting: Some("1".to_string()),
            disable_bug_command: Some("1".to_string()),
            claude_code_disable_nonessential_traffic: Some("1".to_string()),
            telemetry_disabled: Some("1".to_string()),
            disable_telemetry: Some("1".to_string()),
            ccp_runtime_hook: Some("/tmp/hook.js".to_string()),
            hostaliases: Some("/tmp/hosts".to_string()),
            anthropic_base_url: None,
            anthropic_auth_token: None,
            anthropic_api_key: None,
            dns_error_code: Some("ECONNREFUSED".to_string()),
            dns_blocked: true,
        };

        let problems = validate_live_audit_payload(&payload);

        assert!(problems.iter().any(|item| item.contains("NODE_USE_ENV_PROXY")));
    }
}
