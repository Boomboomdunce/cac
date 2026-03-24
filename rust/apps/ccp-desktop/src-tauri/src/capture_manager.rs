//! Manages the lifecycle of the local capture proxy and pushes events to the frontend.

use ccp_sidecar::{CaptureBuffer, CaptureProxy, CaptureProxyConfig};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

/// Shared state for the capture proxy.
pub struct CaptureState {
    proxy: Mutex<Option<CaptureProxy>>,
    buffer: Arc<CaptureBuffer>,
}

impl CaptureState {
    pub fn new() -> Self {
        Self {
            proxy: Mutex::new(None),
            buffer: CaptureBuffer::new(50_000),
        }
    }

    pub fn buffer(&self) -> &Arc<CaptureBuffer> {
        &self.buffer
    }

    /// Start the capture proxy. Returns the local port.
    pub async fn start_proxy(
        &self,
        upstream_addr: String,
        tool_name: String,
    ) -> Result<u16, String> {
        let mut guard = self.proxy.lock().await;
        if guard.is_some() {
            return Err("Capture proxy already running".to_string());
        }

        let config = CaptureProxyConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            upstream_addr,
            tool_name,
        };

        let proxy = CaptureProxy::start(config, self.buffer.clone())
            .await
            .map_err(|e| format!("Failed to start proxy: {e}"))?;

        let port = proxy.port();

        // Write port file so CLI can discover the sidecar
        let port_file = super::commands::state_root().join("config").join("sidecar_port");
        let _ = std::fs::write(&port_file, port.to_string());

        *guard = Some(proxy);
        Ok(port)
    }

    /// Stop the capture proxy.
    pub async fn stop_proxy(&self) {
        let mut guard = self.proxy.lock().await;
        if let Some(proxy) = guard.take() {
            proxy.shutdown().await;
        }
        // Remove port file
        let port_file = super::commands::state_root().join("config").join("sidecar_port");
        let _ = std::fs::remove_file(&port_file);
    }

    /// Whether the proxy is currently running.
    pub async fn is_running(&self) -> bool {
        self.proxy.lock().await.is_some()
    }

    /// Get the local proxy port, if running.
    pub async fn proxy_port(&self) -> Option<u16> {
        self.proxy.lock().await.as_ref().map(|p| p.port())
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
