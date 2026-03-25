//! Manages the lifecycle of capture backends and pushes events to the frontend.

use crate::mitmproxy_backend::MitmproxyProcess;
use ccp_sidecar::{CaptureBuffer, CaptureProxy, CaptureProxyConfig, MitmProxyConfig};
use ccp_store::StateLayout;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

pub struct CaptureRuntimeStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub backend: String,
    pub target: Option<String>,
    pub warning: Option<String>,
}

enum ActiveCapture {
    Explicit(CaptureProxy),
    Transparent(MitmproxyProcess),
}

/// Shared state for capture backends.
pub struct CaptureState {
    backend: Mutex<Option<ActiveCapture>>,
    buffer: Arc<CaptureBuffer>,
}

impl CaptureState {
    pub fn new() -> Self {
        Self {
            backend: Mutex::new(None),
            buffer: CaptureBuffer::new(50_000),
        }
    }

    pub fn buffer(&self) -> &Arc<CaptureBuffer> {
        &self.buffer
    }

    pub async fn start_explicit_proxy(
        &self,
        upstream_addr: String,
        tool_name: String,
        mitm: Option<MitmProxyConfig>,
        state_root: std::path::PathBuf,
    ) -> Result<CaptureRuntimeStatus, String> {
        let mut guard = self.backend.lock().await;
        if guard.is_some() {
            return Err("Capture backend already running".to_string());
        }

        let config = CaptureProxyConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            upstream_addr,
            tool_name,
            mitm,
        };

        let proxy = CaptureProxy::start(config, self.buffer.clone())
            .await
            .map_err(|e| format!("Failed to start proxy: {e}"))?;
        let port = proxy.port();
        let port_file = state_root.join("config").join("sidecar_port");
        let _ = std::fs::write(&port_file, port.to_string());

        *guard = Some(ActiveCapture::Explicit(proxy));
        Ok(CaptureRuntimeStatus {
            running: true,
            port: Some(port),
            backend: "explicit".to_string(),
            target: None,
            warning: None,
        })
    }

    pub async fn start_transparent_capture(
        &self,
        layout: &StateLayout,
        selector: &str,
        tool_name: &str,
    ) -> Result<CaptureRuntimeStatus, String> {
        let mut guard = self.backend.lock().await;
        if guard.is_some() {
            return Err("Capture backend already running".to_string());
        }

        let process = crate::mitmproxy_backend::start_mitmdump_process(
            layout,
            selector,
            tool_name,
            self.buffer.clone(),
        )
        .await?;
        let port_file = layout.config_dir().join("sidecar_port");
        let _ = std::fs::remove_file(&port_file);

        *guard = Some(ActiveCapture::Transparent(process));
        Ok(CaptureRuntimeStatus {
            running: true,
            port: None,
            backend: "transparent".to_string(),
            target: Some(selector.to_string()),
            warning: None,
        })
    }

    pub async fn stop_capture(&self, state_root: std::path::PathBuf) {
        let mut guard = self.backend.lock().await;
        let active = guard.take();
        drop(guard);

        match active {
            Some(ActiveCapture::Explicit(proxy)) => {
                proxy.shutdown().await;
            }
            Some(ActiveCapture::Transparent(process)) => {
                process.shutdown().await;
            }
            None => {}
        }

        let port_file = state_root.join("config").join("sidecar_port");
        let _ = std::fs::remove_file(&port_file);
    }

    pub async fn status(&self) -> CaptureRuntimeStatus {
        let mut guard = self.backend.lock().await;
        match guard.as_mut() {
            Some(ActiveCapture::Explicit(proxy)) => CaptureRuntimeStatus {
                running: true,
                port: Some(proxy.port()),
                backend: "explicit".to_string(),
                target: None,
                warning: None,
            },
            Some(ActiveCapture::Transparent(process)) => match process.try_wait() {
                Ok(None) => CaptureRuntimeStatus {
                    running: true,
                    port: None,
                    backend: "transparent".to_string(),
                    target: Some(process.selector.clone()),
                    warning: None,
                },
                Ok(Some(status)) => {
                    let selector = process.selector.clone();
                    *guard = None;
                    CaptureRuntimeStatus {
                        running: false,
                        port: None,
                        backend: "transparent".to_string(),
                        target: Some(selector),
                        warning: Some(format!("mitmdump exited: {status}")),
                    }
                }
                Err(err) => CaptureRuntimeStatus {
                    running: false,
                    port: None,
                    backend: "transparent".to_string(),
                    target: Some(process.selector.clone()),
                    warning: Some(err),
                },
            },
            None => CaptureRuntimeStatus {
                running: false,
                port: None,
                backend: "none".to_string(),
                target: None,
                warning: None,
            },
        }
    }
}

/// Spawn a background task that listens for captured requests and emits Tauri events.
/// Uses `tauri::async_runtime` so it works inside Tauri's setup callback.
pub fn spawn_event_forwarder(app: AppHandle, buffer: Arc<CaptureBuffer>) {
    let mut rx = buffer.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(req) => {
                    let _ = app.emit("capture-request", &req);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("capture event forwarder lagged by {n} messages");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });
}
