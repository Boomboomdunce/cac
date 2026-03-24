//! Detect egress IP by making a request through the proxy to an IP echo service.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

/// Detect egress IP by sending an HTTP request through the given proxy.
///
/// `proxy_addr` is host:port of the proxy (e.g., "127.0.0.1:17080" or "1.2.3.4:1080").
pub async fn detect_egress_ip(proxy_addr: &str) -> Result<String, String> {
    let result = timeout(Duration::from_secs(10), detect_inner(proxy_addr)).await;
    match result {
        Ok(Ok(ip)) => Ok(ip),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("timeout".to_string()),
    }
}

async fn detect_inner(proxy_addr: &str) -> Result<String, String> {
    // Connect to proxy
    let stream = TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy: {e}"))?;
    let mut stream = BufReader::new(stream);

    // Send HTTP request through proxy (absolute URI form)
    let request = "GET http://ifconfig.me/ip HTTP/1.1\r\nHost: ifconfig.me\r\nConnection: close\r\nUser-Agent: ccp/1.0\r\n\r\n";
    stream
        .get_mut()
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("write request: {e}"))?;

    // Read response
    let mut status_line = String::new();
    stream
        .read_line(&mut status_line)
        .await
        .map_err(|e| format!("read status: {e}"))?;

    if !status_line.contains("200") {
        return Err(format!("HTTP {}", status_line.trim()));
    }

    // Skip headers
    loop {
        let mut line = String::new();
        stream
            .read_line(&mut line)
            .await
            .map_err(|e| format!("read header: {e}"))?;
        if line.trim().is_empty() {
            break;
        }
    }

    // Read body (the IP address)
    let mut body = String::new();
    stream
        .read_line(&mut body)
        .await
        .map_err(|e| format!("read body: {e}"))?;

    let ip = body.trim().to_string();
    if ip.is_empty() {
        Err("empty response".to_string())
    } else {
        Ok(ip)
    }
}
