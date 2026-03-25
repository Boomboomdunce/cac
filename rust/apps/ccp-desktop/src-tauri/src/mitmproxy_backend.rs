use ccp_sidecar::{CaptureBuffer, CapturedRequest};
use ccp_store::{ensure_mitm_certificates, MitmCertificateMaterial, StateLayout};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MitmproxyBinaryStatus {
    pub available: bool,
    pub path: Option<PathBuf>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransparentCaptureSupport {
    pub available: bool,
    pub path: Option<PathBuf>,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct ManagedMitmproxyConfdir {
    pub confdir: PathBuf,
    pub bridge_script_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct MitmproxyLaunchConfig {
    pub binary_path: PathBuf,
    pub confdir: PathBuf,
    pub bridge_script_path: PathBuf,
    pub selector: String,
    pub tool_name: String,
    pub extra_args: Vec<String>,
}

pub struct MitmproxyProcess {
    child: Child,
    stdout_task: tokio::task::JoinHandle<()>,
    stderr_task: tokio::task::JoinHandle<()>,
    pub selector: String,
}

#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, Debug, Deserialize)]
struct BridgeEnvelope {
    flow_id: String,
    record: BridgeRecord,
}

#[derive(Clone, Debug, Deserialize)]
struct BridgeRecord {
    timestamp: String,
    tool: String,
    protocol: String,
    connection_id: Option<u64>,
    stream_id: Option<u64>,
    method: String,
    url: String,
    status: Option<u16>,
    size: u64,
    duration: Option<u64>,
    complete: bool,
    request_body_truncated: bool,
    response_body_truncated: bool,
    category: String,
    blocked_reason: Option<String>,
    request_headers: Vec<(String, String)>,
    request_body: Option<String>,
    response_headers: Vec<(String, String)>,
    response_body: Option<String>,
}

const BRIDGE_SCRIPT: &str = r#"import json
import os
import sys
from mitmproxy import ctx, http

TOOL_NAME = os.environ.get("CCP_CAPTURE_TOOL_NAME", "claude")


def emit(payload):
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def body_preview(message, limit=65536):
    if message is None:
        return None, False
    raw = message.raw_content or b""
    truncated = len(raw) > limit
    raw = raw[:limit]
    try:
        return raw.decode("utf-8", errors="replace"), truncated
    except Exception:
        return None, truncated


def header_pairs(headers):
    return [[k, v] for k, v in headers.items(multi=True)]


def snapshot(flow):
    request_body, request_truncated = body_preview(flow.request)
    response = flow.response
    response_body, response_truncated = body_preview(response)
    status = response.status_code if response is not None else None
    response_headers = header_pairs(response.headers) if response is not None else []
    size = len(response.raw_content or b"") if response is not None and response.raw_content else 0
    duration = None
    if flow.request.timestamp_start is not None and flow.response is not None and flow.response.timestamp_end is not None:
        duration = int((flow.response.timestamp_end - flow.request.timestamp_start) * 1000)
    return {
        "flow_id": flow.id,
        "record": {
            "timestamp": "",
            "tool": TOOL_NAME,
            "protocol": "http/1.1",
            "connection_id": None,
            "stream_id": None,
            "method": flow.request.method,
            "url": flow.request.pretty_url,
            "status": status,
            "size": size,
            "duration": duration,
            "complete": response is not None,
            "request_body_truncated": request_truncated,
            "response_body_truncated": response_truncated,
            "category": "normal",
            "blocked_reason": None,
            "request_headers": header_pairs(flow.request.headers),
            "request_body": request_body,
            "response_headers": response_headers,
            "response_body": response_body,
        },
    }


class CCPBridge:
    def request(self, flow: http.HTTPFlow):
        emit(snapshot(flow))

    def responseheaders(self, flow: http.HTTPFlow):
        emit(snapshot(flow))

    def response(self, flow: http.HTTPFlow):
        emit(snapshot(flow))

    def error(self, flow: http.HTTPFlow):
        data = snapshot(flow)
        data["record"]["complete"] = True
        if flow.error is not None:
            data["record"]["blocked_reason"] = str(flow.error)
        emit(data)


addons = [CCPBridge()]
"#;

pub fn discover_mitmdump_binary() -> Option<PathBuf> {
    if let Some(path) = env::var_os("CCP_MITMDUMP_PATH").map(PathBuf::from) {
        if is_executable_file(&path) {
            return Some(path);
        }
    }

    if let Some(path) = find_in_path("mitmdump") {
        return Some(path);
    }

    #[cfg(windows)]
    if let Some(path) = find_in_path("mitmdump.exe") {
        return Some(path);
    }

    None
}

pub fn inspect_mitmdump_binary() -> MitmproxyBinaryStatus {
    match discover_mitmdump_binary() {
        Some(path) => MitmproxyBinaryStatus {
            available: true,
            message: format!("mitmdump available at {}", path.display()),
            path: Some(path),
        },
        None => MitmproxyBinaryStatus {
            available: false,
            message: "mitmdump is not installed or not discoverable from PATH".to_string(),
            path: None,
        },
    }
}

pub fn inspect_transparent_capture_support() -> TransparentCaptureSupport {
    let binary = inspect_mitmdump_binary();
    if !binary.available {
        return TransparentCaptureSupport {
            available: false,
            path: None,
            message: binary.message,
        };
    }

    #[cfg(target_os = "macos")]
    {
        let path = binary.path.clone();
        match inspect_macos_redirector_support() {
            Ok(MacosRedirectorSupport::Ready) => TransparentCaptureSupport {
                available: true,
                path,
                message: format!(
                    "mitmdump is installed at {} and the Mitmproxy Redirector extension is enabled",
                    binary
                        .path
                        .as_ref()
                        .map(|value| value.display().to_string())
                        .unwrap_or_else(|| "<unknown>".to_string())
                ),
            },
            Ok(MacosRedirectorSupport::WaitingForUser) => TransparentCaptureSupport {
                available: false,
                path,
                message: format!(
                    "mitmdump is installed at {}, but the Mitmproxy Redirector network extension is waiting for approval. Open macOS System Settings > General > Login Items & Extensions > Network Extensions and allow Mitmproxy Redirector.",
                    binary
                        .path
                        .as_ref()
                        .map(|value| value.display().to_string())
                        .unwrap_or_else(|| "<unknown>".to_string())
                ),
            },
            Ok(MacosRedirectorSupport::Unavailable(state)) => TransparentCaptureSupport {
                available: false,
                path,
                message: format!(
                    "mitmdump is installed at {}, but the Mitmproxy Redirector network extension is not ready ({state}). Transparent local capture cannot start until that extension is enabled.",
                    binary
                        .path
                        .as_ref()
                        .map(|value| value.display().to_string())
                        .unwrap_or_else(|| "<unknown>".to_string())
                ),
            },
            Ok(MacosRedirectorSupport::Missing) => TransparentCaptureSupport {
                available: false,
                path,
                message: format!(
                    "mitmdump is installed at {}, but the Mitmproxy Redirector network extension is not installed or not visible yet. Start mitmdump local capture once and approve the extension before using transparent capture.",
                    binary
                        .path
                        .as_ref()
                        .map(|value| value.display().to_string())
                        .unwrap_or_else(|| "<unknown>".to_string())
                ),
            },
            Err(err) => TransparentCaptureSupport {
                available: false,
                path,
                message: format!(
                    "mitmdump is installed, but CCP could not verify the macOS redirector extension state: {err}"
                ),
            },
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        TransparentCaptureSupport {
            available: true,
            path: binary.path,
            message: binary.message,
        }
    }
}

pub fn export_managed_confdir(
    layout: &StateLayout,
    material: &MitmCertificateMaterial,
) -> Result<ManagedMitmproxyConfdir, String> {
    let root = layout.config_dir().join("mitmproxy");
    let confdir = root.join("confdir");
    fs::create_dir_all(&confdir).map_err(|err| err.to_string())?;

    let cert = fs::read_to_string(&material.ca_cert).map_err(|err| err.to_string())?;
    let key = fs::read_to_string(&material.ca_key).map_err(|err| err.to_string())?;
    let combined_path = confdir.join("mitmproxy-ca.pem");
    fs::write(&combined_path, format!("{}\n{}", key.trim(), cert.trim()))
        .map_err(|err| err.to_string())?;
    fs::write(confdir.join("mitmproxy-ca-cert.pem"), cert).map_err(|err| err.to_string())?;

    let bridge_script_path = root.join("ccp_bridge.py");
    fs::write(&bridge_script_path, BRIDGE_SCRIPT).map_err(|err| err.to_string())?;

    Ok(ManagedMitmproxyConfdir {
        confdir,
        bridge_script_path,
    })
}

pub fn build_mitmdump_command(config: &MitmproxyLaunchConfig) -> Command {
    let mut command = Command::new(&config.binary_path);
    let mode = if matches!(config.selector.as_str(), "" | "all" | "*") {
        "local".to_string()
    } else {
        format!("local:{}", config.selector)
    };
    command.arg("--mode").arg(mode);
    command.arg("--quiet");
    command.arg("-s").arg(&config.bridge_script_path);
    command.arg("--set").arg(format!("confdir={}", config.confdir.display()));
    command.arg("--set").arg("block_global=false");
    command.arg("--set").arg("connection_strategy=lazy");
    for arg in &config.extra_args {
        command.arg(arg);
    }
    command.env("PYTHONUNBUFFERED", "1");
    command.env("CCP_CAPTURE_TOOL_NAME", &config.tool_name);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    command
}

pub async fn start_mitmdump_process(
    layout: &StateLayout,
    selector: &str,
    tool_name: &str,
    buffer: Arc<CaptureBuffer>,
) -> Result<MitmproxyProcess, String> {
    let binary = discover_mitmdump_binary()
        .ok_or_else(|| "mitmdump is not installed or not discoverable from PATH".to_string())?;
    let material = ensure_mitm_certificates(layout).map_err(|err| err.to_string())?;
    let exported = export_managed_confdir(layout, &material)?;
    let config = MitmproxyLaunchConfig {
        binary_path: binary.clone(),
        confdir: exported.confdir.clone(),
        bridge_script_path: exported.bridge_script_path.clone(),
        selector: selector.to_string(),
        tool_name: tool_name.to_string(),
        extra_args: vec![
            "--set".to_string(),
            "store_streamed_bodies=true".to_string(),
        ],
    };
    let mut command = build_mitmdump_command(&config);
    let mut child = command.spawn().map_err(|err| err.to_string())?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "mitmdump stdout pipe was not available".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "mitmdump stderr pipe was not available".to_string())?;

    let stdout_task = tokio::spawn(forward_stdout(stdout, buffer.clone(), tool_name.to_string()));
    let stderr_task = tokio::spawn(forward_stderr(stderr));

    Ok(MitmproxyProcess {
        child,
        stdout_task,
        stderr_task,
        selector: selector.to_string(),
    })
}

impl MitmproxyProcess {
    pub async fn shutdown(mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        let _ = self.stdout_task.await;
        let _ = self.stderr_task.await;
    }

    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>, String> {
        self.child.try_wait().map_err(|err| err.to_string())
    }
}

async fn forward_stdout(
    stdout: tokio::process::ChildStdout,
    buffer: Arc<CaptureBuffer>,
    default_tool_name: String,
) {
    use std::collections::HashMap;

    let mut ids_by_flow = HashMap::<String, u64>::new();
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        if !line.trim_start().starts_with('{') {
            continue;
        }
        let Ok(envelope) = serde_json::from_str::<BridgeEnvelope>(&line) else {
            eprintln!("mitmproxy bridge emitted invalid JSON line: {line}");
            continue;
        };
        let record = captured_request_from_bridge(envelope.record, &default_tool_name);
        if let Some(id) = ids_by_flow.get(&envelope.flow_id).copied() {
            let _ = buffer.update(id, |current| {
                current.timestamp = record.timestamp.clone();
                current.tool = record.tool.clone();
                current.protocol = record.protocol.clone();
                current.connection_id = record.connection_id;
                current.stream_id = record.stream_id;
                current.method = record.method.clone();
                current.url = record.url.clone();
                current.status = record.status;
                current.size = record.size;
                current.duration = record.duration;
                current.complete = record.complete;
                current.request_body_truncated = record.request_body_truncated;
                current.response_body_truncated = record.response_body_truncated;
                current.category = record.category.clone();
                current.blocked_reason = record.blocked_reason.clone();
                current.request_headers = record.request_headers.clone();
                current.request_body = record.request_body.clone();
                current.response_headers = record.response_headers.clone();
                current.response_body = record.response_body.clone();
            });
        } else {
            let id = buffer.create(record);
            ids_by_flow.insert(envelope.flow_id, id);
        }
    }
}

async fn forward_stderr(stderr: tokio::process::ChildStderr) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if !line.trim().is_empty() {
            eprintln!("mitmdump: {line}");
        }
    }
}

fn captured_request_from_bridge(record: BridgeRecord, default_tool_name: &str) -> CapturedRequest {
    CapturedRequest {
        id: 0,
        timestamp: if record.timestamp.is_empty() {
            timestamp_now()
        } else {
            record.timestamp
        },
        tool: if record.tool.is_empty() {
            default_tool_name.to_string()
        } else {
            record.tool
        },
        protocol: record.protocol,
        connection_id: record.connection_id,
        stream_id: record.stream_id,
        method: record.method,
        url: record.url,
        status: record.status,
        size: record.size,
        duration: record.duration,
        complete: record.complete,
        request_body_truncated: record.request_body_truncated,
        response_body_truncated: record.response_body_truncated,
        category: record.category,
        blocked_reason: record.blocked_reason,
        request_headers: record.request_headers,
        request_body: record.request_body,
        response_headers: record.response_headers,
        response_body: record.response_body,
    }
}

fn timestamp_now() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    let millis = now.subsec_millis();
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq)]
enum MacosRedirectorSupport {
    Ready,
    WaitingForUser,
    Unavailable(String),
    Missing,
}

#[cfg(target_os = "macos")]
fn inspect_macos_redirector_support() -> Result<MacosRedirectorSupport, String> {
    let output = read_systemextensionsctl_list_output()?;
    if let Some(line) = find_macos_redirector_line(&output) {
        if parse_macos_redirector_ready(&output) == Some(true) {
            return Ok(MacosRedirectorSupport::Ready);
        }
        if line.contains("waiting for user") {
            return Ok(MacosRedirectorSupport::WaitingForUser);
        }
        let state = parse_bracket_state(line).unwrap_or_else(|| "unknown state".to_string());
        return Ok(MacosRedirectorSupport::Unavailable(state));
    }

    Ok(MacosRedirectorSupport::Missing)
}

#[cfg(target_os = "macos")]
fn read_systemextensionsctl_list_output() -> Result<String, String> {
    if let Some(mock) = env::var_os("CCP_TEST_SYSTEMEXTENSIONSCTL_LIST") {
        return Ok(mock.to_string_lossy().into_owned());
    }

    let output = std::process::Command::new("systemextensionsctl")
        .arg("list")
        .output()
        .map_err(|err| format!("failed to run systemextensionsctl list: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stderr.trim().is_empty() {
        stdout.into_owned()
    } else if stdout.trim().is_empty() {
        stderr.into_owned()
    } else {
        format!("{stdout}\n{stderr}")
    };

    if !output.status.success() && combined.trim().is_empty() {
        return Err(format!(
            "systemextensionsctl list exited with status {}",
            output.status
        ));
    }

    Ok(combined)
}

fn find_macos_redirector_line(output: &str) -> Option<&str> {
    output
        .lines()
        .find(|line| line.contains("org.mitmproxy.macos-redirector.network-extension"))
}

fn parse_bracket_state(line: &str) -> Option<String> {
    let start = line.rfind('[')?;
    let end = line.rfind(']')?;
    if end <= start + 1 {
        return None;
    }
    Some(line[start + 1..end].trim().to_string())
}

pub fn parse_macos_redirector_ready(output: &str) -> Option<bool> {
    let line = find_macos_redirector_line(output)?;
    let state = parse_bracket_state(line)?;
    if state.contains("enabled") {
        return Some(true);
    }
    if !state.is_empty() {
        return Some(false);
    }
    None
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|entry| entry.join(program))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return std::fs::metadata(path)
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_mitmdump_command, discover_mitmdump_binary, export_managed_confdir,
        inspect_mitmdump_binary, inspect_transparent_capture_support, parse_macos_redirector_ready,
        start_mitmdump_process, MitmproxyLaunchConfig, TEST_ENV_LOCK,
    };
    use ccp_sidecar::CaptureBuffer;
    use ccp_store::{ensure_mitm_certificates, StateLayout};
    use tempfile::tempdir;

    #[test]
    fn env_override_wins_for_mitmdump_discovery() {
        let temp = tempdir().unwrap();
        let fake = temp.path().join("mitmdump");
        std::fs::write(&fake, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&fake).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&fake, perms).unwrap();
        }

        let previous = std::env::var_os("CCP_MITMDUMP_PATH");
        std::env::set_var("CCP_MITMDUMP_PATH", &fake);
        let found = discover_mitmdump_binary();
        assert_eq!(found.as_deref(), Some(fake.as_path()));
        let status = inspect_mitmdump_binary();
        assert!(status.available);

        if let Some(value) = previous {
            std::env::set_var("CCP_MITMDUMP_PATH", value);
        } else {
            std::env::remove_var("CCP_MITMDUMP_PATH");
        }
    }

    #[test]
    fn export_managed_confdir_writes_combined_ca_and_bridge_script() {
        let temp = tempdir().unwrap();
        let layout = StateLayout::new(temp.path()).unwrap();
        let material = ensure_mitm_certificates(&layout).unwrap();

        let exported = export_managed_confdir(&layout, &material).unwrap();
        let combined_ca = std::fs::read_to_string(exported.confdir.join("mitmproxy-ca.pem")).unwrap();
        let bridge = std::fs::read_to_string(exported.bridge_script_path).unwrap();

        assert!(combined_ca.contains("BEGIN CERTIFICATE"));
        assert!(combined_ca.contains("BEGIN PRIVATE KEY") || combined_ca.contains("BEGIN RSA PRIVATE KEY"));
        assert!(bridge.contains("from mitmproxy import"));
        assert!(exported.confdir.is_dir());
    }

    #[test]
    fn build_mitmdump_command_uses_local_selector_and_confdir() {
        let temp = tempdir().unwrap();
        let binary = temp.path().join("mitmdump");
        let confdir = temp.path().join("confdir");
        let bridge = temp.path().join("bridge.py");
        let config = MitmproxyLaunchConfig {
            binary_path: binary.clone(),
            confdir: confdir.clone(),
            bridge_script_path: bridge.clone(),
            selector: "all".to_string(),
            tool_name: "claude".to_string(),
            extra_args: vec!["--set".to_string(), "store_streamed_bodies=true".to_string()],
        };

        let command = build_mitmdump_command(&config);
        let rendered = format!("{command:?}");

        assert!(rendered.contains(&binary.display().to_string()));
        assert!(rendered.contains("local"));
        assert!(rendered.contains("--quiet"));
        assert!(rendered.contains(&confdir.display().to_string()));
        assert!(rendered.contains(&bridge.display().to_string()));
        assert!(rendered.contains("store_streamed_bodies=true"));
    }

    #[test]
    fn macos_redirector_ready_parser_detects_waiting_state() {
        let sample = r#"
--- com.apple.system_extension.network_extension
enabled	active	teamID	bundleID (version)	name	[state]
	*	S8XHQB96PW	org.mitmproxy.macos-redirector.network-extension (2.0/1)	network-extension	[activated waiting for user]
"#;

        assert_eq!(parse_macos_redirector_ready(sample), Some(false));
    }

    #[test]
    fn macos_redirector_ready_parser_detects_enabled_state() {
        let sample = r#"
--- com.apple.system_extension.network_extension
enabled	active	teamID	bundleID (version)	name	[state]
*	*	S8XHQB96PW	org.mitmproxy.macos-redirector.network-extension (2.0/1)	network-extension	[activated enabled]
"#;

        assert_eq!(parse_macos_redirector_ready(sample), Some(true));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn transparent_capture_support_requires_enabled_redirector_on_macos() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let temp = tempdir().unwrap();
        let fake = temp.path().join("mitmdump");
        std::fs::write(&fake, "#!/bin/sh\nexit 0\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake, perms).unwrap();

        let previous_bin = std::env::var_os("CCP_MITMDUMP_PATH");
        let previous_list = std::env::var_os("CCP_TEST_SYSTEMEXTENSIONSCTL_LIST");
        std::env::set_var("CCP_MITMDUMP_PATH", &fake);
        std::env::set_var(
            "CCP_TEST_SYSTEMEXTENSIONSCTL_LIST",
            r#"
--- com.apple.system_extension.network_extension
enabled	active	teamID	bundleID (version)	name	[state]
	*	S8XHQB96PW	org.mitmproxy.macos-redirector.network-extension (2.0/1)	network-extension	[activated waiting for user]
"#,
        );

        let waiting = inspect_transparent_capture_support();
        assert!(!waiting.available);
        assert!(waiting.message.contains("waiting for approval"));

        std::env::set_var(
            "CCP_TEST_SYSTEMEXTENSIONSCTL_LIST",
            r#"
--- com.apple.system_extension.network_extension
enabled	active	teamID	bundleID (version)	name	[state]
*	*	S8XHQB96PW	org.mitmproxy.macos-redirector.network-extension (2.0/1)	network-extension	[activated enabled]
"#,
        );

        let enabled = inspect_transparent_capture_support();
        assert!(enabled.available);

        if let Some(value) = previous_bin {
            std::env::set_var("CCP_MITMDUMP_PATH", value);
        } else {
            std::env::remove_var("CCP_MITMDUMP_PATH");
        }

        if let Some(value) = previous_list {
            std::env::set_var("CCP_TEST_SYSTEMEXTENSIONSCTL_LIST", value);
        } else {
            std::env::remove_var("CCP_TEST_SYSTEMEXTENSIONSCTL_LIST");
        }
    }

    #[tokio::test]
    #[ignore = "runs a real HTTPS fetch through mitmproxy local capture"]
    async fn transparent_capture_records_curl_https_request() {
        if std::env::var_os("CCP_RUN_TRANSPARENT_E2E").is_none() {
            return;
        }

        let Some(binary) = discover_mitmdump_binary() else {
            panic!("mitmdump is required for this test");
        };

        let temp = tempdir().unwrap();
        let layout = StateLayout::new(temp.path()).unwrap();
        let previous = std::env::var_os("CCP_MITMDUMP_PATH");
        std::env::set_var("CCP_MITMDUMP_PATH", &binary);

        let buffer = CaptureBuffer::new(1024);
        let process = start_mitmdump_process(&layout, "all", "curl", buffer.clone())
            .await
            .unwrap();

        let output = tokio::process::Command::new("curl")
            .arg("-ksS")
            .arg("https://ifconfig.me/ip")
            .output()
            .await
            .unwrap();

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let records = buffer.snapshot();
        process.shutdown().await;

        if let Some(value) = previous {
            std::env::set_var("CCP_MITMDUMP_PATH", value);
        } else {
            std::env::remove_var("CCP_MITMDUMP_PATH");
        }

        assert!(
            output.status.success(),
            "node fetch failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(
            records.iter().any(|record| record.url.contains("ifconfig.me")),
            "expected captured request for ifconfig.me, got {records:#?}"
        );
    }
}
