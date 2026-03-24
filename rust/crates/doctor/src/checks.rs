use crate::report::CheckResult;
use crate::DoctorConfig;
use anyhow::Context;
use claude_adapter::{claude_adapter, ADAPTER_NAME};
use core::{CapabilitySet, PlatformDoctorCheck, PrivacyPolicy, Profile, TargetAdapter};
use serde_json::from_reader;
use std::{
    fs,
    fs::File,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use x509_parser::{parse_x509_certificate, pem::parse_x509_pem};

const STATE_DIRS: &[&str] = &[
    "profiles",
    "identities",
    "certs",
    "hooks",
    "sessions",
    "sidecar",
    "audit",
    "config",
];

const SECRET_DIRS: &[&str] = &["profiles", "identities", "certs", "hooks"];

const SUPPORTED_ADAPTERS: &[&str] = &["claude"];

pub fn profile_existence(config: &DoctorConfig) -> CheckResult {
    match canonical_profile_name(config.profile()) {
        Ok(name) => {
            let path = profile_file_path(config.state_root(), &name);
            if path.is_file() {
                CheckResult::ok(
                    "profile existence",
                    Some(format!("profile '{}' exists at {}", name, path.display())),
                )
            } else {
                CheckResult::error(
                    "profile existence",
                    Some(format!(
                        "profile '{}' not found (expected {})",
                        name,
                        path.display()
                    )),
                )
            }
        }
        Err(err) => CheckResult::error("profile existence", Some(err)),
    }
}

pub fn state_root_layout(config: &DoctorConfig) -> CheckResult {
    let missing: Vec<String> = STATE_DIRS
        .iter()
        .filter(|dir| !config.state_root().join(dir).is_dir())
        .map(|dir| dir.to_string())
        .collect();

    if missing.is_empty() {
        CheckResult::ok(
            "State root layout",
            Some("all expected directories are present".to_string()),
        )
    } else {
        CheckResult::error(
            "State root layout",
            Some(format!("missing directories: {}", missing.join(", "))),
        )
    }
}

pub fn adapter_resolution(config: &DoctorConfig) -> CheckResult {
    let name = match canonical_profile_name(config.profile()) {
        Ok(name) => name,
        Err(err) => return CheckResult::error("adapter resolution", Some(err)),
    };

    let profile = match load_profile(config.state_root(), &name) {
        Ok(profile) => profile,
        Err(err) => {
            return CheckResult::error(
                "adapter resolution",
                Some(format!("failed to load profile: {}", err)),
            )
        }
    };

    let adapter_name = profile.adapter.clone();
    if !SUPPORTED_ADAPTERS.contains(&adapter_name.as_str()) {
        return CheckResult::error(
            "adapter resolution",
            Some(format!("unsupported adapter '{}'", adapter_name)),
        );
    }

    let required_capabilities = required_capabilities_for_adapter(adapter_name.as_str());
    let platform_identity = current_platform_identity();
    let provided_capabilities = current_platform_capabilities();
    let missing_capabilities = required_capabilities.difference(&provided_capabilities);
    if !missing_capabilities.is_empty() {
        return CheckResult::error(
            "adapter resolution",
            Some(format!(
                "adapter '{}' is unsupported on '{}' due to missing required capabilities: {}",
                adapter_name,
                platform_identity,
                render_capability_set(&missing_capabilities)
            )),
        );
    }

    let adapter = TargetAdapter::new(
        adapter_name.clone(),
        required_capabilities,
        CapabilitySet::new(),
        PrivacyPolicy::default(),
    );

    CheckResult::ok(
        "adapter resolution",
        Some(format!(
            "adapter '{}' is supported on '{}' with capabilities: {}",
            adapter.name,
            platform_identity,
            render_capability_set(&provided_capabilities)
        )),
    )
}

pub fn secret_permission_sanity(config: &DoctorConfig) -> CheckResult {
    let mut violations = Vec::new();
    let mut missing = Vec::new();

    for dir in SECRET_DIRS {
        let path = config.state_root().join(dir);
        if !path.exists() {
            missing.push(dir.to_string());
            continue;
        }

        match has_owner_only_permissions(&path) {
            Ok(true) => {}
            Ok(false) => violations.push(dir.to_string()),
            Err(err) => {
                return CheckResult::error(
                    "secret permission sanity",
                    Some(format!("cannot stat {}: {}", path.display(), err)),
                );
            }
        }
    }

    if !violations.is_empty() {
        return CheckResult::error(
            "secret permission sanity",
            Some(format!(
                "directories with weak permissions: {}",
                violations.join(", ")
            )),
        );
    }

    if !missing.is_empty() {
        return CheckResult::warning(
            "secret permission sanity",
            Some(format!(
                "missing secret directories: {}",
                missing.join(", ")
            )),
        );
    }

    CheckResult::ok(
        "secret permission sanity",
        Some("directory permissions look appropriate".to_string()),
    )
}

pub fn platform_capability_support(config: &DoctorConfig) -> CheckResult {
    let name = match canonical_profile_name(config.profile()) {
        Ok(name) => name,
        Err(err) => return CheckResult::error("platform capability support", Some(err)),
    };

    let profile = match load_profile(config.state_root(), &name) {
        Ok(profile) => profile,
        Err(err) => {
            return CheckResult::error(
                "platform capability support",
                Some(format!("failed to load profile: {}", err)),
            )
        }
    };

    let adapter = match resolve_adapter(&profile) {
        Ok(adapter) => adapter,
        Err(err) => return CheckResult::error("platform capability support", Some(err)),
    };

    let platform_checks = current_platform_doctor_checks(&adapter.required_capabilities);
    summarize_platform_doctor_checks(platform_checks)
}

pub fn identity_materials(config: &DoctorConfig) -> CheckResult {
    let name = match canonical_profile_name(config.profile()) {
        Ok(name) => name,
        Err(err) => return CheckResult::error("identity materials", Some(err)),
    };
    let required = [
        (
            "uuid",
            config
                .state_root()
                .join("identities")
                .join(&name)
                .join("uuid"),
        ),
        (
            "stable_id",
            config
                .state_root()
                .join("identities")
                .join(&name)
                .join("stable_id"),
        ),
        (
            "user_id",
            config
                .state_root()
                .join("identities")
                .join(&name)
                .join("user_id"),
        ),
        (
            "machine_id",
            config
                .state_root()
                .join("identities")
                .join(&name)
                .join("machine_id"),
        ),
        (
            "hostname",
            config
                .state_root()
                .join("identities")
                .join(&name)
                .join("hostname"),
        ),
        (
            "mac_address",
            config
                .state_root()
                .join("identities")
                .join(&name)
                .join("mac_address"),
        ),
        (
            "tz",
            config
                .state_root()
                .join("identities")
                .join(&name)
                .join("tz"),
        ),
        (
            "lang",
            config
                .state_root()
                .join("identities")
                .join(&name)
                .join("lang"),
        ),
    ];
    let missing = required
        .iter()
        .filter_map(|(label, path)| is_missing_or_blank(path).then_some(*label))
        .collect::<Vec<_>>();

    if missing.is_empty() {
        CheckResult::ok(
            "identity materials",
            Some("identity files are present".to_string()),
        )
    } else {
        CheckResult::error(
            "identity materials",
            Some(format!(
                "missing or empty identity files: {}",
                missing.join(", ")
            )),
        )
    }
}

pub fn mtls_materials(config: &DoctorConfig) -> CheckResult {
    let name = match canonical_profile_name(config.profile()) {
        Ok(name) => name,
        Err(err) => return CheckResult::error("mTLS materials", Some(err)),
    };
    let required = [
        ("ca_cert", config.state_root().join("certs/ca/ca_cert.pem")),
        ("ca_key", config.state_root().join("certs/ca/ca_key.pem")),
        (
            "client_cert",
            config
                .state_root()
                .join("certs")
                .join(&name)
                .join("client_cert.pem"),
        ),
        (
            "client_key",
            config
                .state_root()
                .join("certs")
                .join(&name)
                .join("client_key.pem"),
        ),
    ];
    let missing = required
        .iter()
        .filter_map(|(label, path)| (!path.is_file()).then_some(*label))
        .collect::<Vec<_>>();

    if missing.is_empty() {
        match validate_mtls_certificate_chain(&required[0].1, &required[2].1) {
            Ok(details) => CheckResult::ok("mTLS materials", Some(details)),
            Err(err) => CheckResult::error(
                "mTLS materials",
                Some(format!("certificate verification failed: {err}")),
            ),
        }
    } else {
        CheckResult::error(
            "mTLS materials",
            Some(format!("missing certificate files: {}", missing.join(", "))),
        )
    }
}

pub fn dns_blocking(config: &DoctorConfig) -> CheckResult {
    let name = match canonical_profile_name(config.profile()) {
        Ok(name) => name,
        Err(err) => return CheckResult::error("dns blocking", Some(err)),
    };
    let profile = match load_profile(config.state_root(), &name) {
        Ok(profile) => profile,
        Err(err) => {
            return CheckResult::error(
                "dns blocking",
                Some(format!("failed to load profile: {}", err)),
            )
        }
    };

    if !profile.adapter.eq_ignore_ascii_case(ADAPTER_NAME) {
        return CheckResult::warning(
            "dns blocking",
            Some(format!(
                "no DNS blocking check implemented for adapter '{}'",
                profile.adapter
            )),
        );
    }

    let runtime_hook = config.state_root().join("hooks/claude-preload.js");
    if !runtime_hook.is_file() {
        return CheckResult::warning(
            "dns blocking",
            Some(format!("missing runtime hook {}", runtime_hook.display())),
        );
    }

    let blocked_hosts = config.state_root().join("config/blocked_hosts");
    if !blocked_hosts.is_file() {
        return CheckResult::warning(
            "dns blocking",
            Some(format!(
                "missing blocked hosts file {}",
                blocked_hosts.display()
            )),
        );
    }

    match execute_dns_block_check(&runtime_hook, &blocked_hosts, "statsig.anthropic.com") {
        Ok(result) if result == "BLOCKED" => CheckResult::ok(
            "dns blocking",
            Some("wrapped node DNS lookup is blocked for statsig.anthropic.com".to_string()),
        ),
        Ok(result) => CheckResult::error(
            "dns blocking",
            Some(format!("statsig.anthropic.com was not blocked ({result})")),
        ),
        Err(err) => CheckResult::warning(
            "dns blocking",
            Some(format!("unable to run node DNS blocking check: {err}")),
        ),
    }
}

pub fn proxy_reachability(config: &DoctorConfig) -> CheckResult {
    let name = match canonical_profile_name(config.profile()) {
        Ok(name) => name,
        Err(err) => return CheckResult::error("proxy reachability", Some(err)),
    };
    let profile = match load_profile(config.state_root(), &name) {
        Ok(profile) => profile,
        Err(err) => {
            return CheckResult::error(
                "proxy reachability",
                Some(format!("failed to load profile: {}", err)),
            )
        }
    };

    let Some(proxy_url) = profile.policy.proxy_url() else {
        return CheckResult::warning(
            "proxy reachability",
            Some("profile has no proxy configured".to_string()),
        );
    };
    let Some(proxy_host) = core::proxy_host_port(proxy_url) else {
        return CheckResult::error(
            "proxy reachability",
            Some(format!("invalid proxy URL '{}'", proxy_url)),
        );
    };

    match check_proxy_reachability(proxy_host.as_str()) {
        Ok(()) => CheckResult::ok(
            "proxy reachability",
            Some(format!("proxy {} accepted a TCP connection", proxy_host)),
        ),
        Err(err) => CheckResult::error(
            "proxy reachability",
            Some(format!("proxy {} is unreachable: {}", proxy_host, err)),
        ),
    }
}

pub fn local_proxy_conflicts(config: &DoctorConfig) -> CheckResult {
    let name = match canonical_profile_name(config.profile()) {
        Ok(name) => name,
        Err(err) => return CheckResult::error("local proxy conflicts", Some(err)),
    };
    let profile = match load_profile(config.state_root(), &name) {
        Ok(profile) => profile,
        Err(err) => {
            return CheckResult::error(
                "local proxy conflicts",
                Some(format!("failed to load profile: {}", err)),
            )
        }
    };

    let Some(proxy_url) = profile.policy.proxy_url() else {
        return CheckResult::warning(
            "local proxy conflicts",
            Some("profile has no proxy configured".to_string()),
        );
    };
    let Some(proxy_host) = core::proxy_host_port(proxy_url) else {
        return CheckResult::error(
            "local proxy conflicts",
            Some(format!("invalid proxy URL '{}'", proxy_url)),
        );
    };

    let process_snapshot = command_stdout(proxy_process_command());
    let interface_snapshot = command_stdout(tun_interface_command());
    let system_proxy_snapshot = system_http_proxy_snapshot();
    let direct_exit_ip = detect_direct_exit_ip().ok();
    let proxy_exit_ip = detect_exit_ip(proxy_url).ok();
    let proxy_host_name = proxy_host
        .split(':')
        .next()
        .unwrap_or(proxy_host.as_str())
        .trim();
    let findings = detect_local_proxy_conflicts(
        current_platform_identity(),
        process_snapshot.as_deref().unwrap_or_default(),
        interface_snapshot.as_deref().unwrap_or_default(),
        system_proxy_snapshot.as_deref().unwrap_or_default(),
        direct_exit_ip.as_deref(),
        proxy_exit_ip.as_deref(),
        proxy_host_name,
    );

    if findings.is_empty() {
        CheckResult::ok(
            "local proxy conflicts",
            Some("no obvious local proxy or TUN conflicts detected".to_string()),
        )
    } else {
        CheckResult::warning(
            "local proxy conflicts",
            Some(format!(
                "{}; add a DIRECT rule for {} if needed",
                findings.join("; "),
                proxy_host
            )),
        )
    }
}

pub fn proxy_exit_ip(config: &DoctorConfig) -> CheckResult {
    let name = match canonical_profile_name(config.profile()) {
        Ok(name) => name,
        Err(err) => return CheckResult::error("proxy exit IP", Some(err)),
    };
    let profile = match load_profile(config.state_root(), &name) {
        Ok(profile) => profile,
        Err(err) => {
            return CheckResult::error(
                "proxy exit IP",
                Some(format!("failed to load profile: {}", err)),
            );
        }
    };

    let Some(proxy_url) = profile.policy.proxy_url() else {
        return CheckResult::warning(
            "proxy exit IP",
            Some("profile has no proxy configured".to_string()),
        );
    };

    match detect_exit_ip(proxy_url) {
        Ok(exit_ip) if !exit_ip.trim().is_empty() => CheckResult::ok(
            "proxy exit IP",
            Some(format!("proxy exit IP is {}", exit_ip.trim())),
        ),
        Ok(_) => CheckResult::warning(
            "proxy exit IP",
            Some("exit IP service returned an empty response".to_string()),
        ),
        Err(err) => CheckResult::warning(
            "proxy exit IP",
            Some(format!("failed to determine proxy exit IP: {}", err)),
        ),
    }
}

pub fn runtime_self_audit(config: &DoctorConfig) -> CheckResult {
    let name = match canonical_profile_name(config.profile()) {
        Ok(name) => name,
        Err(err) => return CheckResult::error("runtime self-audit", Some(err)),
    };
    let profile = match load_profile(config.state_root(), &name) {
        Ok(profile) => profile,
        Err(err) => {
            return CheckResult::error(
                "runtime self-audit",
                Some(format!("failed to load profile: {}", err)),
            )
        }
    };

    if !profile.adapter.eq_ignore_ascii_case(ADAPTER_NAME) {
        return CheckResult::warning(
            "runtime self-audit",
            Some(format!(
                "no runtime self-audit implemented for adapter '{}'",
                profile.adapter
            )),
        );
    }

    let mut missing = Vec::new();
    let runtime_hook = config.state_root().join("hooks/claude-preload.js");
    let blocked_hosts = config.state_root().join("config/blocked_hosts");
    if !runtime_hook.is_file() {
        missing.push(format!("missing {}", runtime_hook.display()));
    }
    if !blocked_hosts.is_file() {
        missing.push(format!("missing {}", blocked_hosts.display()));
    }

    if missing.is_empty() {
        CheckResult::ok(
            "runtime self-audit",
            Some("runtime hook and blocked-host assets are present".to_string()),
        )
    } else {
        CheckResult::warning("runtime self-audit", Some(missing.join("; ")))
    }
}

fn summarize_platform_doctor_checks(checks: Vec<PlatformDoctorCheck>) -> CheckResult {
    if checks.is_empty() {
        return CheckResult::error(
            "platform capability support",
            Some(format!(
                "platform '{}' did not return any doctor checks",
                current_platform_identity()
            )),
        );
    }

    let has_error = checks.iter().any(|check| !check.ok);
    let message = checks
        .iter()
        .map(|check| check.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");

    if has_error {
        CheckResult::error("platform capability support", Some(message))
    } else {
        CheckResult::ok("platform capability support", Some(message))
    }
}

fn canonical_profile_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("profile name is empty".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.as_bytes().contains(&0) {
        return Err("profile name contains invalid characters".to_string());
    }
    Ok(trimmed.to_string())
}

fn profile_file_path(root: &Path, name: &str) -> PathBuf {
    root.join("profiles").join(format!("{}.json", name))
}

fn load_profile(root: &Path, name: &str) -> anyhow::Result<Profile> {
    let path = profile_file_path(root, name);
    let file = File::open(&path).context("opening profile metadata")?;
    let profile = from_reader(file).context("parsing profile metadata")?;
    Ok(profile)
}

fn is_missing_or_blank(path: &Path) -> bool {
    match fs::read_to_string(path) {
        Ok(contents) => contents.trim().is_empty(),
        Err(_) => true,
    }
}

fn check_proxy_reachability(proxy_host: &str) -> std::io::Result<()> {
    let addresses = proxy_host.to_socket_addrs()?;
    let timeout = Duration::from_secs(2);
    let mut last_error = None;

    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(_) => return Ok(()),
            Err(err) => last_error = Some(err),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no proxy addresses resolved")
    }))
}

fn execute_dns_block_check(
    runtime_hook: &Path,
    blocked_hosts: &Path,
    domain: &str,
) -> anyhow::Result<String> {
    let script = r#"
require(process.argv[1]);
const dns = require('dns');
dns.lookup(process.argv[2], (err) => {
  if (err && (err.code === 'ECONNREFUSED' || err.code === 'ENOTFOUND')) {
    process.stdout.write('BLOCKED');
    return;
  }
  if (err && err.code) {
    process.stdout.write(`OPEN:${err.code}`);
    return;
  }
  process.stdout.write('OPEN');
});
"#;
    let output = Command::new("node")
        .arg("-e")
        .arg(script)
        .arg(runtime_hook)
        .arg(domain)
        .env("HOSTALIASES", blocked_hosts)
        .output()
        .context("executing node")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("node exited with status {}", output.status)
        };
        anyhow::bail!("{detail}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn validate_mtls_certificate_chain(
    ca_cert_path: &Path,
    client_cert_path: &Path,
) -> anyhow::Result<String> {
    let ca_pem =
        fs::read(ca_cert_path).with_context(|| format!("reading {}", ca_cert_path.display()))?;
    let client_pem = fs::read(client_cert_path)
        .with_context(|| format!("reading {}", client_cert_path.display()))?;

    let (_, ca_pem_block) = parse_x509_pem(&ca_pem)
        .map_err(|err| anyhow::anyhow!("parsing {} as PEM: {:?}", ca_cert_path.display(), err))?;
    let (_, client_pem_block) = parse_x509_pem(&client_pem).map_err(|err| {
        anyhow::anyhow!("parsing {} as PEM: {:?}", client_cert_path.display(), err)
    })?;

    let (_, ca_cert) = parse_x509_certificate(&ca_pem_block.contents)
        .map_err(|err| anyhow::anyhow!("parsing {} as X.509: {:?}", ca_cert_path.display(), err))?;
    let (_, client_cert) = parse_x509_certificate(&client_pem_block.contents).map_err(|err| {
        anyhow::anyhow!("parsing {} as X.509: {:?}", client_cert_path.display(), err)
    })?;

    ca_cert
        .verify_signature(None)
        .map_err(|err| anyhow::anyhow!("CA certificate signature check failed: {err}"))?;
    client_cert
        .verify_signature(Some(ca_cert.public_key()))
        .map_err(|err| anyhow::anyhow!("client certificate was not signed by the CA: {err}"))?;

    if !ca_cert.validity().is_valid() {
        anyhow::bail!(
            "CA certificate is outside its validity window (expires {})",
            ca_cert.validity().not_after
        );
    }
    if !client_cert.validity().is_valid() {
        anyhow::bail!(
            "client certificate is outside its validity window (expires {})",
            client_cert.validity().not_after
        );
    }

    let common_name = client_cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .unwrap_or("<unknown>");

    Ok(format!(
        "client certificate verified (CN={common_name}, expires: {})",
        client_cert.validity().not_after
    ))
}

fn detect_exit_ip(proxy_url: &str) -> anyhow::Result<String> {
    let ipify_url =
        std::env::var("CCP_IPIFY_URL").unwrap_or_else(|_| "https://api.ipify.org".to_string());
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .proxy(reqwest::Proxy::all(proxy_url)?)
        .build()
        .context("building proxy-aware HTTP client")?;
    let response = client
        .get(ipify_url)
        .send()
        .context("fetching exit IP")?
        .error_for_status()
        .context("exit IP service returned non-success status")?;
    response.text().context("reading exit IP response")
}

fn detect_direct_exit_ip() -> anyhow::Result<String> {
    let ipify_url =
        std::env::var("CCP_IPIFY_URL").unwrap_or_else(|_| "https://api.ipify.org".to_string());
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .no_proxy()
        .build()
        .context("building direct HTTP client")?;
    let response = client
        .get(ipify_url)
        .send()
        .context("fetching direct exit IP")?
        .error_for_status()
        .context("direct exit IP service returned non-success status")?;
    response.text().context("reading direct exit IP response")
}

fn detect_local_proxy_conflicts(
    platform: &str,
    process_snapshot: &str,
    interface_snapshot: &str,
    system_proxy_snapshot: &str,
    direct_exit_ip: Option<&str>,
    proxy_exit_ip: Option<&str>,
    proxy_host: &str,
) -> Vec<String> {
    let mut findings = Vec::new();
    let normalized_processes = process_snapshot.to_lowercase();
    let proxy_processes = [
        "clash",
        "mihomo",
        "sing-box",
        "surge",
        "shadowrocket",
        "v2ray",
        "xray",
        "hysteria",
        "tuic",
        "nekoray",
    ];
    let running = proxy_processes
        .iter()
        .filter(|name| normalized_processes.contains(**name))
        .copied()
        .collect::<Vec<_>>();
    if !running.is_empty() {
        findings.push(format!(
            "detected local proxy processes: {}",
            running.join(", ")
        ));
    }

    let normalized_interfaces = interface_snapshot.to_lowercase();
    let tun_present = match platform {
        "macos" => normalized_interfaces.matches("utun").count() > 3,
        "linux" => {
            normalized_interfaces.contains("tun0") || normalized_interfaces.contains("\ntun")
        }
        "windows" => {
            normalized_interfaces.contains("wintun") || normalized_interfaces.contains("tun")
        }
        _ => false,
    };
    if tun_present {
        findings.push("detected TUN-style network interfaces".to_string());
    }

    if platform == "macos" {
        let normalized_system_proxy = system_proxy_snapshot.to_lowercase();
        if normalized_system_proxy.contains("enabled: yes") {
            let server = parse_networksetup_value(system_proxy_snapshot, "Server")
                .unwrap_or_else(|| "<unknown>".to_string());
            let port = parse_networksetup_value(system_proxy_snapshot, "Port")
                .unwrap_or_else(|| "<unknown>".to_string());
            findings.push(format!("detected system HTTP proxy: {server}:{port}"));
        }
    }

    let normalized_direct_ip = direct_exit_ip
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let normalized_proxy_ip = proxy_exit_ip
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let (Some(direct_ip), Some(proxy_ip)) = (normalized_direct_ip, normalized_proxy_ip) {
        if direct_ip == proxy_ip {
            findings.push(format!(
                "proxy exit IP {proxy_ip} is the same as direct exit IP; local software may be intercepting traffic to {proxy_host}"
            ));
        }
    }

    findings
}

fn parse_networksetup_value(snapshot: &str, key: &str) -> Option<String> {
    snapshot.lines().find_map(|line| {
        let (label, value) = line.split_once(':')?;
        (label.trim() == key).then(|| value.trim().to_string())
    })
}

fn command_stdout(command: (&'static str, &'static [&'static str])) -> Option<String> {
    let (program, args) = command;
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn system_http_proxy_snapshot() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let services_output = Command::new("networksetup")
            .args(["-listallnetworkservices"])
            .output()
            .ok()?;
        if !services_output.status.success() {
            return None;
        }

        let services = String::from_utf8_lossy(&services_output.stdout);
        let service = services.lines().map(str::trim).find(|line| {
            !line.is_empty()
                && !line.starts_with('*')
                && matches!(*line, "Wi-Fi" | "Ethernet" | "以太网")
        })?;

        let output = Command::new("networksetup")
            .args(["-getwebproxy", service])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn proxy_process_command() -> (&'static str, &'static [&'static str]) {
    ("ps", &["-A", "-o", "comm="])
}

#[cfg(target_os = "linux")]
fn proxy_process_command() -> (&'static str, &'static [&'static str]) {
    ("ps", &["-A", "-o", "comm="])
}

#[cfg(target_os = "windows")]
fn proxy_process_command() -> (&'static str, &'static [&'static str]) {
    ("tasklist", &[])
}

#[cfg(target_os = "macos")]
fn tun_interface_command() -> (&'static str, &'static [&'static str]) {
    ("ifconfig", &[])
}

#[cfg(target_os = "linux")]
fn tun_interface_command() -> (&'static str, &'static [&'static str]) {
    ("ip", &["link", "show"])
}

#[cfg(target_os = "windows")]
fn tun_interface_command() -> (&'static str, &'static [&'static str]) {
    ("ipconfig", &["/all"])
}

fn resolve_adapter(profile: &Profile) -> Result<TargetAdapter, String> {
    if profile.adapter.eq_ignore_ascii_case(ADAPTER_NAME) {
        Ok(claude_adapter().target_adapter().clone())
    } else {
        Err(format!("unsupported adapter '{}'", profile.adapter))
    }
}

#[cfg(target_os = "macos")]
fn current_platform_identity() -> &'static str {
    platform_macos::platform_identity()
}

#[cfg(target_os = "linux")]
fn current_platform_identity() -> &'static str {
    platform_linux::platform_identity()
}

#[cfg(target_os = "windows")]
fn current_platform_identity() -> &'static str {
    platform_windows::platform_identity()
}

#[cfg(target_os = "macos")]
fn current_platform_doctor_checks(required: &CapabilitySet) -> Vec<PlatformDoctorCheck> {
    platform_macos::doctor_checks(required)
}

#[cfg(target_os = "macos")]
fn current_platform_capabilities() -> CapabilitySet {
    platform_macos::provided_capabilities()
}

#[cfg(target_os = "linux")]
fn current_platform_doctor_checks(required: &CapabilitySet) -> Vec<PlatformDoctorCheck> {
    platform_linux::doctor_checks(required)
}

#[cfg(target_os = "linux")]
fn current_platform_capabilities() -> CapabilitySet {
    platform_linux::provided_capabilities()
}

#[cfg(target_os = "windows")]
fn current_platform_doctor_checks(required: &CapabilitySet) -> Vec<PlatformDoctorCheck> {
    platform_windows::doctor_checks(required)
}

#[cfg(target_os = "windows")]
fn current_platform_capabilities() -> CapabilitySet {
    platform_windows::provided_capabilities()
}

fn has_owner_only_permissions(path: &Path) -> std::io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let metadata = path.metadata()?;
        let mode = metadata.permissions().mode();
        Ok(mode & 0o077 == 0)
    }

    #[cfg(not(unix))]
    {
        let _ = path.metadata()?;
        Ok(true)
    }
}

fn required_capabilities_for_adapter(adapter_name: &str) -> CapabilitySet {
    if adapter_name.eq_ignore_ascii_case("claude") {
        CapabilitySet::from(["node_preload", "sidecar"])
    } else {
        CapabilitySet::new()
    }
}

fn render_capability_set(capabilities: &CapabilitySet) -> String {
    if capabilities.is_empty() {
        return "<none>".to_string();
    }

    capabilities
        .iter()
        .map(|value| value.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_platform_doctor_checks_keeps_all_messages_and_error_status() {
        let result = summarize_platform_doctor_checks(vec![
            PlatformDoctorCheck::ok(
                "platform capability support",
                "node preload support is available",
            ),
            PlatformDoctorCheck::error("platform capability support", "sidecar bridge is missing"),
        ]);

        assert_eq!(result.name, "platform capability support");
        assert_eq!(result.status, crate::report::CheckStatus::Error);
        let message = result.message.expect("expected aggregated message");
        assert!(message.contains("node preload support is available"));
        assert!(message.contains("sidecar bridge is missing"));
    }

    #[test]
    fn detect_local_proxy_conflicts_warns_on_proxy_processes_and_tun_interfaces() {
        let findings = detect_local_proxy_conflicts(
            "macos",
            "/Applications/Clash Verge\n/usr/bin/zsh\n",
            "utun0\nutun1\nutun2\nutun3\n",
            "",
            None,
            None,
            "1.2.3.4",
        );

        assert!(findings.iter().any(|item| item.contains("proxy processes")));
        assert!(findings.iter().any(|item| item.contains("TUN-style")));
    }

    #[test]
    fn detect_local_proxy_conflicts_is_empty_when_no_signals_are_present() {
        let findings = detect_local_proxy_conflicts(
            "linux",
            "sshd\nzsh\n",
            "lo\neth0\n",
            "",
            None,
            None,
            "1.2.3.4",
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn detect_local_proxy_conflicts_warns_when_macos_system_proxy_is_enabled() {
        let findings = detect_local_proxy_conflicts(
            "macos",
            "",
            "",
            "Enabled: Yes\nServer: 127.0.0.1\nPort: 6152\n",
            None,
            None,
            "1.2.3.4",
        );

        assert!(findings
            .iter()
            .any(|item| item.contains("system HTTP proxy")));
    }

    #[test]
    fn detect_local_proxy_conflicts_warns_when_direct_exit_ip_matches_proxy_exit_ip() {
        let findings = detect_local_proxy_conflicts(
            "linux",
            "",
            "",
            "",
            Some("198.51.100.7"),
            Some("198.51.100.7"),
            "1.2.3.4",
        );

        assert!(findings
            .iter()
            .any(|item| item.contains("same as direct exit IP")));
    }
}
