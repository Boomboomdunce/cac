//! Local capture proxy — sits between tool and upstream proxy.
//!
//! Accepts HTTP/1.1 proxy requests (CONNECT for HTTPS, absolute-URI for HTTP).
//! Forwards them to the configured upstream proxy. Records metadata in a
//! [`CaptureBuffer`] for real-time display in the GUI.

use crate::capture::{build_connect_record, CaptureBuffer};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// Configuration for the local capture proxy.
#[derive(Clone, Debug)]
pub struct CaptureProxyConfig {
    /// Bind address, e.g. "127.0.0.1:0" for random port.
    pub bind_addr: String,
    /// Upstream proxy address (host:port extracted from proxy URL).
    pub upstream_addr: String,
    /// Tool name to tag captured records with.
    pub tool_name: String,
}

/// Running capture proxy handle.
pub struct CaptureProxy {
    local_addr: SocketAddr,
    shutdown_tx: watch::Sender<bool>,
    join: JoinHandle<()>,
    buffer: Arc<CaptureBuffer>,
}

impl CaptureProxy {
    /// Start the proxy. Returns immediately; proxy runs in background tasks.
    pub async fn start(
        config: CaptureProxyConfig,
        buffer: Arc<CaptureBuffer>,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(&config.bind_addr).await?;
        let local_addr = listener.local_addr()?;
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let buf = buffer.clone();
        let join = tokio::spawn(accept_loop(listener, config, buf, shutdown_rx));

        Ok(CaptureProxy {
            local_addr,
            shutdown_tx,
            join,
            buffer,
        })
    }

    /// The local address the proxy is listening on.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// The local port.
    pub fn port(&self) -> u16 {
        self.local_addr.port()
    }

    /// Get a reference to the capture buffer.
    pub fn buffer(&self) -> &Arc<CaptureBuffer> {
        &self.buffer
    }

    /// Gracefully shut down the proxy.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.join.await;
    }
}

async fn accept_loop(
    listener: TcpListener,
    config: CaptureProxyConfig,
    buffer: Arc<CaptureBuffer>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _peer)) => {
                        let cfg = config.clone();
                        let buf = buffer.clone();
                        tokio::spawn(handle_client(stream, cfg, buf));
                    }
                    Err(_) => break,
                }
            }
            _ = shutdown.changed() => {
                break;
            }
        }
    }
}

async fn handle_client(client: TcpStream, config: CaptureProxyConfig, buffer: Arc<CaptureBuffer>) {
    let _ = handle_client_inner(client, config, buffer).await;
}

async fn handle_client_inner(
    client: TcpStream,
    config: CaptureProxyConfig,
    buffer: Arc<CaptureBuffer>,
) -> io::Result<()> {
    let mut reader = BufReader::new(client);

    // Read the first line to determine request type
    let mut first_line = String::new();
    reader.read_line(&mut first_line).await?;
    let first_line = first_line.trim_end().to_string();

    if first_line.is_empty() {
        return Ok(());
    }

    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 3 {
        return Ok(());
    }

    let method = parts[0];
    let target = parts[1];

    if method.eq_ignore_ascii_case("CONNECT") {
        handle_connect(reader, target, &config, &buffer).await
    } else {
        handle_http(reader, &first_line, &config, &buffer).await
    }
}

/// Handle HTTPS CONNECT tunnel.
async fn handle_connect(
    mut client: BufReader<TcpStream>,
    target_host: &str,
    config: &CaptureProxyConfig,
    buffer: &Arc<CaptureBuffer>,
) -> io::Result<()> {
    let start = std::time::Instant::now();

    // Read remaining headers (consume until empty line)
    loop {
        let mut line = String::new();
        client.read_line(&mut line).await?;
        if line.trim().is_empty() {
            break;
        }
    }

    // Connect to upstream proxy
    let mut upstream = match TcpStream::connect(&config.upstream_addr).await {
        Ok(s) => s,
        Err(_) => {
            // Send error to client
            let client_inner = client.into_inner();
            let mut w = client_inner;
            let _ = w
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n")
                .await;
            buffer.push(build_connect_record(
                target_host,
                &config.tool_name,
                0,
                0,
                start,
                false,
            ));
            return Ok(());
        }
    };

    // Send CONNECT to upstream proxy
    let connect_req = format!("CONNECT {target_host} HTTP/1.1\r\nHost: {target_host}\r\n\r\n");
    upstream.write_all(connect_req.as_bytes()).await?;

    // Read upstream response first line
    let mut upstream_reader = BufReader::new(upstream);
    let mut status_line = String::new();
    upstream_reader.read_line(&mut status_line).await?;

    let upstream_ok = status_line.contains("200");

    // Forward remaining headers from upstream
    let mut upstream_headers = status_line.clone();
    loop {
        let mut line = String::new();
        upstream_reader.read_line(&mut line).await?;
        upstream_headers.push_str(&line);
        if line.trim().is_empty() {
            break;
        }
    }

    // Send upstream response back to client
    let mut client_inner = client.into_inner();
    client_inner.write_all(upstream_headers.as_bytes()).await?;

    if !upstream_ok {
        buffer.push(build_connect_record(
            target_host,
            &config.tool_name,
            0,
            0,
            start,
            false,
        ));
        return Ok(());
    }

    // Bridge the two streams, counting bytes
    let upstream_inner = upstream_reader.into_inner();
    let (mut cr, mut cw) = io::split(client_inner);
    let (mut ur, mut uw) = io::split(upstream_inner);

    let bytes_up = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let bytes_down = Arc::new(std::sync::atomic::AtomicU64::new(0));

    let bu = bytes_up.clone();
    let bd = bytes_down.clone();

    let client_to_upstream = tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match cr.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    bu.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
                    if uw.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = uw.shutdown().await;
    });

    let upstream_to_client = tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match ur.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    bd.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
                    if cw.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = cw.shutdown().await;
    });

    // Wait for either direction to finish
    let _ = tokio::try_join!(client_to_upstream, upstream_to_client);

    let up = bytes_up.load(std::sync::atomic::Ordering::Relaxed);
    let down = bytes_down.load(std::sync::atomic::Ordering::Relaxed);
    buffer.push(build_connect_record(
        target_host,
        &config.tool_name,
        up,
        down,
        start,
        true,
    ));

    Ok(())
}

/// Handle plain HTTP request (absolute URI form).
async fn handle_http(
    mut client: BufReader<TcpStream>,
    first_line: &str,
    config: &CaptureProxyConfig,
    buffer: &Arc<CaptureBuffer>,
) -> io::Result<()> {
    let start = std::time::Instant::now();
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    let method = parts[0].to_string();
    let url = parts[1].to_string();

    // Read request headers
    let mut req_headers = Vec::new();
    loop {
        let mut line = String::new();
        client.read_line(&mut line).await?;
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            req_headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }

    // Connect to upstream and forward
    let mut upstream = TcpStream::connect(&config.upstream_addr).await?;

    // Forward the request
    upstream
        .write_all(format!("{first_line}\r\n").as_bytes())
        .await?;
    for (k, v) in &req_headers {
        upstream
            .write_all(format!("{k}: {v}\r\n").as_bytes())
            .await?;
    }
    upstream.write_all(b"\r\n").await?;

    // Read response status
    let mut upstream_reader = BufReader::new(upstream);
    let mut status_line = String::new();
    upstream_reader.read_line(&mut status_line).await?;

    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Read response headers
    let mut res_headers = Vec::new();
    let mut content_length: u64 = 0;
    loop {
        let mut line = String::new();
        upstream_reader.read_line(&mut line).await?;
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let k = k.trim().to_string();
            let v = v.trim().to_string();
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.parse().unwrap_or(0);
            }
            res_headers.push((k, v));
        }
    }

    // Forward response to client
    let mut client_inner = client.into_inner();
    client_inner.write_all(status_line.as_bytes()).await?;
    for (k, v) in &res_headers {
        client_inner
            .write_all(format!("{k}: {v}\r\n").as_bytes())
            .await?;
    }
    client_inner.write_all(b"\r\n").await?;

    // Forward response body
    if content_length > 0 {
        let mut remaining = content_length;
        let mut buf = [0u8; 8192];
        while remaining > 0 {
            let to_read = buf.len().min(remaining as usize);
            let n = upstream_reader.read(&mut buf[..to_read]).await?;
            if n == 0 {
                break;
            }
            client_inner.write_all(&buf[..n]).await?;
            remaining -= n as u64;
        }
    }

    buffer.push(crate::capture::build_http_record(
        crate::capture::HttpRecordParams {
            method,
            url,
            tool: config.tool_name.clone(),
            status: status_code,
            size: content_length,
            start,
            req_headers,
            res_headers,
        },
    ));

    Ok(())
}
