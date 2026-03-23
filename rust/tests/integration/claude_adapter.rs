use claude_adapter::claude_adapter;
use serde_json::Value;
use std::{fs, process::Command};

fn preload_path() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../hooks/node/claude-preload.js")
        .display()
        .to_string()
}

#[test]
fn policy_includes_blocked_telemetry_hosts() {
    let adapter = claude_adapter();
    let blocked_hosts = adapter.blocked_hosts();

    assert!(blocked_hosts.contains("statsig.anthropic.com"));
    assert!(blocked_hosts.contains("sentry.io"));
    assert!(blocked_hosts.contains("o1137031.ingest.sentry.io"));
}

#[test]
fn policy_requires_node_preload_runtime_hook() {
    let adapter = claude_adapter();

    assert!(adapter.required_capabilities().contains("node_preload"));
    let hook = adapter.runtime_hook_bundle();
    assert_eq!(hook.relative_path(), "hooks/node/claude-preload.js");
    assert!(hook.contents().contains("dns.lookup"));
    assert!(hook.contents().contains("fetch"));
}

#[test]
fn policy_marks_sidecar_as_required() {
    let adapter = claude_adapter();

    assert!(adapter.sidecar_required());
    assert!(adapter.required_capabilities().contains("sidecar"));
}

#[test]
fn policy_includes_claude_specific_telemetry_toggles() {
    let adapter = claude_adapter();
    let env = adapter.environment_overrides();

    assert_eq!(env.get("DO_NOT_TRACK").map(String::as_str), Some("1"));
    assert_eq!(
        env.get("OTEL_SDK_DISABLED").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        env.get("OTEL_TRACES_EXPORTER").map(String::as_str),
        Some("none")
    );
    assert_eq!(
        env.get("OTEL_METRICS_EXPORTER").map(String::as_str),
        Some("none")
    );
    assert_eq!(
        env.get("OTEL_LOGS_EXPORTER").map(String::as_str),
        Some("none")
    );
    assert_eq!(env.get("SENTRY_DSN").map(String::as_str), Some(""));
    assert_eq!(
        env.get("DISABLE_ERROR_REPORTING").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        env.get("DISABLE_BUG_COMMAND").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        env.get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")
            .map(String::as_str),
        Some("1")
    );
    assert_eq!(env.get("TELEMETRY_DISABLED").map(String::as_str), Some("1"));
    assert_eq!(env.get("DISABLE_TELEMETRY").map(String::as_str), Some("1"));
    assert_eq!(
        env.get("CLAUDE_CODE_ENABLE_TELEMETRY").map(String::as_str),
        Some("")
    );
}

#[test]
fn preload_preserves_tls_connect_port_and_options_shape() {
    let temp = tempfile::tempdir().unwrap();
    let cert_path = temp.path().join("client-cert.pem");
    let key_path = temp.path().join("client-key.pem");
    fs::write(&cert_path, "test-cert").unwrap();
    fs::write(&key_path, "test-key").unwrap();

    let script = format!(
        r#"
const tls = require('tls');
let captured = null;
tls.connect = function stubTlsConnect(...args) {{
  captured = args;
  return {{ on() {{}}, once() {{}}, destroy() {{}} }};
}};
process.env.CCP_PROXY_HOST = 'proxy.test:443';
process.env.CCP_MTLS_CERT = '{}';
process.env.CCP_MTLS_KEY = '{}';
require('{}');
tls.connect(443, {{
  host: 'proxy.test',
  servername: 'proxy.test',
  ALPNProtocols: ['h2']
}});
console.log(JSON.stringify({{
  argCount: captured.length,
  host: captured[0].host,
  servername: captured[0].servername,
  alpn: captured[0].ALPNProtocols,
  hasCert: Boolean(captured[0].cert),
  hasKey: Boolean(captured[0].key)
}}));
"#,
        cert_path.display(),
        key_path.display(),
        preload_path()
    );

    let output = Command::new("node").arg("-e").arg(script).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload.get("argCount").and_then(Value::as_u64), Some(1));
    assert_eq!(
        payload.get("host").and_then(Value::as_str),
        Some("proxy.test")
    );
    assert_eq!(
        payload.get("servername").and_then(Value::as_str),
        Some("proxy.test")
    );
    assert_eq!(
        payload.get("alpn").and_then(Value::as_array).map(Vec::len),
        Some(1)
    );
    assert_eq!(payload.get("hasCert").and_then(Value::as_bool), Some(true));
    assert_eq!(payload.get("hasKey").and_then(Value::as_bool), Some(true));
}

#[test]
fn preload_blocks_tunneled_tls_connections_by_servername() {
    let temp = tempfile::tempdir().unwrap();
    let cert_path = temp.path().join("client-cert.pem");
    let key_path = temp.path().join("client-key.pem");
    fs::write(&cert_path, "test-cert").unwrap();
    fs::write(&key_path, "test-key").unwrap();

    let script = format!(
        r#"
const tls = require('tls');
let calledOriginal = false;
tls.connect = function stubTlsConnect() {{
  calledOriginal = true;
  return {{ on() {{}}, once() {{}}, destroy() {{}} }};
}};
process.env.CCP_PROXY_HOST = 'proxy.test:443';
process.env.CCP_MTLS_CERT = '{}';
process.env.CCP_MTLS_KEY = '{}';
require('{}');
const socket = tls.connect({{
  socket: {{}},
  servername: 'statsig.anthropic.com'
}});
let emittedCode = null;
socket.on('error', (err) => {{
  emittedCode = err && err.code ? err.code : null;
}});
setTimeout(() => {{
  console.log(JSON.stringify({{
    calledOriginal,
    emittedCode
  }}));
}}, 20);
"#,
        cert_path.display(),
        key_path.display(),
        preload_path()
    );

    let output = Command::new("node").arg("-e").arg(script).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        payload.get("calledOriginal").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        payload.get("emittedCode").and_then(Value::as_str),
        Some("ECONNREFUSED")
    );
}

#[test]
fn preload_blocks_tunneled_tls_connections_without_mtls_configuration() {
    let script = format!(
        r#"
const tls = require('tls');
let calledOriginal = false;
tls.connect = function stubTlsConnect() {{
  calledOriginal = true;
  return {{ on() {{}}, once() {{}}, destroy() {{}} }};
}};
require('{}');
const socket = tls.connect({{
  socket: {{}},
  servername: 'statsig.anthropic.com'
}});
let emittedCode = null;
socket.on('error', (err) => {{
  emittedCode = err && err.code ? err.code : null;
}});
setTimeout(() => {{
  console.log(JSON.stringify({{
    calledOriginal,
    emittedCode
  }}));
}}, 20);
"#,
        preload_path()
    );

    let output = Command::new("node").arg("-e").arg(script).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        payload.get("calledOriginal").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        payload.get("emittedCode").and_then(Value::as_str),
        Some("ECONNREFUSED")
    );
}

#[test]
fn preload_blocks_when_host_is_blocked_even_if_servername_is_safe() {
    let temp = tempfile::tempdir().unwrap();
    let cert_path = temp.path().join("client-cert.pem");
    let key_path = temp.path().join("client-key.pem");
    fs::write(&cert_path, "test-cert").unwrap();
    fs::write(&key_path, "test-key").unwrap();

    let script = format!(
        r#"
const tls = require('tls');
let calledOriginal = false;
tls.connect = function stubTlsConnect() {{
  calledOriginal = true;
  return {{ on() {{}}, once() {{}}, destroy() {{}} }};
}};
process.env.CCP_PROXY_HOST = 'proxy.test:443';
process.env.CCP_MTLS_CERT = '{}';
process.env.CCP_MTLS_KEY = '{}';
require('{}');
const socket = tls.connect({{
  host: 'statsig.anthropic.com',
  servername: 'proxy.test'
}});
let emittedCode = null;
socket.on('error', (err) => {{
  emittedCode = err && err.code ? err.code : null;
}});
setTimeout(() => {{
  console.log(JSON.stringify({{
    calledOriginal,
    emittedCode
  }}));
}}, 20);
"#,
        cert_path.display(),
        key_path.display(),
        preload_path()
    );

    let output = Command::new("node").arg("-e").arg(script).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        payload.get("calledOriginal").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        payload.get("emittedCode").and_then(Value::as_str),
        Some("ECONNREFUSED")
    );
}
