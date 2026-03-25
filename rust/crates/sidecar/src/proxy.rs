//! Local capture proxy — sits between tool and upstream proxy.
//!
//! Accepts HTTP/1.1 proxy requests (CONNECT for HTTPS, absolute-URI for HTTP).
//! Forwards them to the configured upstream proxy. When MITM mode is enabled it
//! terminates client TLS locally, opens a second TLS connection upstream, and
//! captures bounded HTTP/1.1 or HTTP/2 payloads.

use crate::capture::{
    build_connect_record, build_http_record, CaptureBuffer, CapturedRequest, HttpRecordParams,
};
use bytes::Bytes;
use h2::client::SendRequest;
use rcgen::{
    CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    RsaKeySize, PKCS_RSA_SHA256,
};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use std::io::Cursor;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Once};
use tokio::io::{
    self, AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_rustls::{TlsAcceptor, TlsConnector};

static RUSTLS_PROVIDER_INIT: Once = Once::new();
static CONNECTION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Optional MITM TLS settings for CONNECT interception.
#[derive(Clone, Debug)]
pub struct MitmProxyConfig {
    pub ca_cert_pem: String,
    pub ca_key_pem: String,
    pub upstream_ca_cert_pem: Option<String>,
    pub max_body_bytes: usize,
}

/// Configuration for the local capture proxy.
#[derive(Clone, Debug)]
pub struct CaptureProxyConfig {
    /// Bind address, e.g. "127.0.0.1:0" for random port.
    pub bind_addr: String,
    /// Upstream proxy address (host:port extracted from proxy URL).
    pub upstream_addr: String,
    /// Tool name to tag captured records with.
    pub tool_name: String,
    /// Optional MITM interception config for HTTPS CONNECT traffic.
    pub mitm: Option<MitmProxyConfig>,
}

#[derive(Clone, Debug)]
struct BodyPreview {
    text: Option<String>,
    truncated: bool,
    bytes_seen: u64,
    binary: bool,
}

impl BodyPreview {
    fn new() -> Self {
        Self {
            text: None,
            truncated: false,
            bytes_seen: 0,
            binary: false,
        }
    }
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
    pub async fn start(config: CaptureProxyConfig, buffer: Arc<CaptureBuffer>) -> io::Result<Self> {
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

async fn handle_connect(
    mut client: BufReader<TcpStream>,
    target_host: &str,
    config: &CaptureProxyConfig,
    buffer: &Arc<CaptureBuffer>,
) -> io::Result<()> {
    let start = std::time::Instant::now();
    let connection_id = CONNECTION_COUNTER.fetch_add(1, Ordering::Relaxed);

    loop {
        let mut line = String::new();
        client.read_line(&mut line).await?;
        if line.trim().is_empty() {
            break;
        }
    }

    if let Some(mitm) = &config.mitm {
        return handle_connect_mitm(client, target_host, config, buffer, start, mitm, connection_id)
            .await;
    }

    handle_connect_passthrough(client, target_host, config, buffer, start, connection_id).await
}

async fn handle_connect_passthrough(
    client: BufReader<TcpStream>,
    target_host: &str,
    config: &CaptureProxyConfig,
    buffer: &Arc<CaptureBuffer>,
    start: std::time::Instant,
    connection_id: u64,
) -> io::Result<()> {
    let upstream = match establish_upstream_tunnel(&config.upstream_addr, target_host).await {
        Ok(stream) => stream,
        Err(_) => {
            let mut client_inner = client.into_inner();
            let _ = client_inner
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n")
                .await;
            buffer.push(build_connect_record(
                target_host,
                &config.tool_name,
                0,
                0,
                start,
                false,
                Some(connection_id),
            ));
            return Ok(());
        }
    };

    let mut client_inner = client.into_inner();
    client_inner
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await?;

    let (mut cr, mut cw) = io::split(client_inner);
    let (mut ur, mut uw) = io::split(upstream);

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
        Some(connection_id),
    ));

    Ok(())
}

async fn handle_connect_mitm(
    client: BufReader<TcpStream>,
    target_host: &str,
    config: &CaptureProxyConfig,
    buffer: &Arc<CaptureBuffer>,
    start: std::time::Instant,
    mitm: &MitmProxyConfig,
    connection_id: u64,
) -> io::Result<()> {
    let authority = target_host.to_string();
    let hostname = strip_port(target_host);
    let upstream_tcp = match establish_upstream_tunnel(&config.upstream_addr, target_host).await {
        Ok(stream) => stream,
        Err(_) => {
            let mut client_inner = client.into_inner();
            let _ = client_inner
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n")
                .await;
            buffer.push(build_connect_record(
                target_host,
                &config.tool_name,
                0,
                0,
                start,
                false,
                Some(connection_id),
            ));
            return Ok(());
        }
    };

    let mut client_inner = client.into_inner();
    client_inner
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await?;

    let client_server_config = build_mitm_server_config(mitm, &hostname)?;
    let acceptor = TlsAcceptor::from(Arc::new(client_server_config));

    let client_tls = acceptor
        .accept(client_inner)
        .await
        .map_err(|err| io_other("accepting client TLS", err))?;

    let client_alpn = client_tls
        .get_ref()
        .1
        .alpn_protocol()
        .map(|value| value.to_vec());
    let upstream_client_config = build_upstream_client_config(mitm, client_alpn.as_deref())?;
    let connector = TlsConnector::from(Arc::new(upstream_client_config));
    let server_name =
        ServerName::try_from(hostname.clone()).map_err(|err| io_other("building SNI", err))?;
    let upstream_tls = connector
        .connect(server_name, upstream_tcp)
        .await
        .map_err(|err| io_other("connecting upstream TLS", err))?;
    let upstream_alpn = upstream_tls
        .get_ref()
        .1
        .alpn_protocol()
        .map(|value| value.to_vec());

    if client_alpn.as_deref() == Some(b"h2".as_slice()) {
        if upstream_alpn.as_deref() != Some(b"h2".as_slice()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "client negotiated h2 but upstream negotiated {:?}",
                    upstream_alpn
                        .as_deref()
                        .map(|value| String::from_utf8_lossy(value).into_owned())
                ),
            ));
        }

        return handle_connect_mitm_h2(
            client_tls,
            upstream_tls,
            &authority,
            target_host,
            config,
            buffer,
            start,
            mitm,
            connection_id,
        )
        .await;
    }

    handle_connect_mitm_h1(
        client_tls,
        upstream_tls,
        &authority,
        target_host,
        config,
        buffer,
        start,
        mitm.max_body_bytes,
        connection_id,
    )
    .await
}

async fn handle_connect_mitm_h1(
    client_tls: tokio_rustls::server::TlsStream<TcpStream>,
    upstream_tls: tokio_rustls::client::TlsStream<TcpStream>,
    authority: &str,
    target_host: &str,
    config: &CaptureProxyConfig,
    buffer: &Arc<CaptureBuffer>,
    start: std::time::Instant,
    max_body_bytes: usize,
    connection_id: u64,
) -> io::Result<()> {
    let mut client_reader = BufReader::new(client_tls);
    let mut upstream_reader = BufReader::new(upstream_tls);
    let mut captured_http = false;

    loop {
        let request = match read_http_message(&mut client_reader).await {
            Ok(Some(request)) => request,
            Ok(None) => break,
            Err(err) => {
                if !captured_http {
                    buffer.push(build_connect_record(
                        target_host,
                        &config.tool_name,
                        0,
                        0,
                        start,
                        true,
                        Some(connection_id),
                    ));
                }
                return Err(err);
            }
        };
        write_http_message(
            upstream_reader.get_mut(),
            &request.start_line,
            &request.headers,
            &request.wire_body,
        )
        .await?;

        let Some((response_start_line, response_headers)) = (match read_http_head(&mut upstream_reader).await {
            Ok(Some(response)) => Some(response),
            Ok(None) => break,
            Err(err) => {
                if !captured_http {
                    buffer.push(build_connect_record(
                        target_host,
                        &config.tool_name,
                        0,
                        0,
                        start,
                        true,
                        Some(connection_id),
                    ));
                }
                return Err(err);
            }
        }) else {
            break;
        };
        write_http_message(
            client_reader.get_mut(),
            &response_start_line,
            &response_headers,
            &[],
        )
        .await?;

        let (method, path) = parse_request_line(&request.start_line);
        let status = parse_status_code(&response_start_line);
        let req_preview = preview_full_body(&request.body, &request.headers, max_body_bytes);
        let record_id = buffer.create(build_http_record(HttpRecordParams {
            method,
            url: build_https_url(authority, &path),
            tool: config.tool_name.clone(),
            protocol: "http/1.1".to_string(),
            connection_id: Some(connection_id),
            stream_id: None,
            status,
            size: 0,
            start,
            complete: false,
            req_headers: request.headers.clone(),
            res_headers: response_headers.clone(),
            req_body: req_preview.text,
            res_body: None,
            req_body_truncated: req_preview.truncated,
            res_body_truncated: false,
        }));
        stream_http1_response_body(
            &mut upstream_reader,
            client_reader.get_mut(),
            &response_start_line,
            &response_headers,
            buffer,
            record_id,
            start,
            max_body_bytes,
        )
        .await?;
        captured_http = true;

        if request.close || should_close(&response_headers) {
            break;
        }
    }

    if !captured_http {
        buffer.push(build_connect_record(
            target_host,
            &config.tool_name,
            0,
            0,
            start,
            true,
            Some(connection_id),
        ));
    }

    Ok(())
}

async fn handle_connect_mitm_h2(
    client_tls: tokio_rustls::server::TlsStream<TcpStream>,
    upstream_tls: tokio_rustls::client::TlsStream<TcpStream>,
    authority: &str,
    target_host: &str,
    config: &CaptureProxyConfig,
    buffer: &Arc<CaptureBuffer>,
    start: std::time::Instant,
    mitm: &MitmProxyConfig,
    connection_id: u64,
) -> io::Result<()> {
    let mut client_conn = h2::server::handshake(client_tls)
        .await
        .map_err(|err| io_other("starting client h2 handshake", err))?;
    let (upstream_send, upstream_conn) = h2::client::handshake(upstream_tls)
        .await
        .map_err(|err| io_other("starting upstream h2 handshake", err))?;
    let upstream_driver = tokio::spawn(async move {
        let _ = upstream_conn.await;
    });
    let upstream_send = Arc::new(tokio::sync::Mutex::new(upstream_send));
    let mut tasks = Vec::new();
    let mut captured_http = false;

    while let Some(result) = client_conn.accept().await {
        let (request, respond) =
            result.map_err(|err| io_other("accepting client h2 stream", err))?;
        captured_http = true;
        let upstream_send = upstream_send.clone();
        let buffer = buffer.clone();
        let authority = authority.to_string();
        let tool_name = config.tool_name.clone();
        let max_body_bytes = mitm.max_body_bytes;
        tasks.push(tokio::spawn(async move {
            if let Err(err) = proxy_h2_stream(
                request,
                respond,
                upstream_send,
                buffer,
                authority,
                tool_name,
                connection_id,
                max_body_bytes,
            )
            .await
            {
                eprintln!("h2 stream proxy error: {err}");
            }
        }));
    }

    for task in tasks {
        let _ = task.await;
    }
    let _ = upstream_driver.await;

    if !captured_http {
        buffer.push(build_connect_record(
            target_host,
            &config.tool_name,
            0,
            0,
            start,
            true,
            Some(connection_id),
        ));
    }

    Ok(())
}

async fn proxy_h2_stream(
    request: http::Request<h2::RecvStream>,
    mut respond: h2::server::SendResponse<Bytes>,
    upstream_send: Arc<tokio::sync::Mutex<SendRequest<Bytes>>>,
    buffer: Arc<CaptureBuffer>,
    authority: String,
    tool_name: String,
    connection_id: u64,
    max_body_bytes: usize,
) -> io::Result<()> {
    let start = std::time::Instant::now();
    let (parts, mut req_body) = request.into_parts();
    let stream_id = Some(req_body.stream_id().as_u32() as u64);
    let request_headers = header_pairs(&parts.headers);
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    let url = build_https_url(&authority, path_and_query);
    let upstream_uri = http::Uri::builder()
        .scheme("https")
        .authority(authority.as_str())
        .path_and_query(path_and_query)
        .build()
        .map_err(|err| io_other("building upstream h2 URI", err))?;
    let record_id = buffer.create(CapturedRequest {
        id: 0,
        timestamp: timestamp_now(),
        tool: tool_name,
        protocol: "h2".to_string(),
        connection_id: Some(connection_id),
        stream_id,
        method: parts.method.as_str().to_string(),
        url,
        status: None,
        size: 0,
        duration: None,
        complete: false,
        request_body_truncated: false,
        response_body_truncated: false,
        category: "normal".to_string(),
        blocked_reason: None,
        request_headers: request_headers.clone(),
        request_body: None,
        response_headers: Vec::new(),
        response_body: None,
    });

    let mut request_builder = http::Request::builder()
        .method(parts.method.clone())
        .uri(upstream_uri)
        .version(parts.version);
    for (name, value) in &parts.headers {
        request_builder = request_builder.header(name, value);
    }
    let upstream_request = request_builder
        .body(())
        .map_err(|err| io_other("building upstream h2 request", err))?;
    let request_end_stream = req_body.is_end_stream();
    let (response_future, mut upstream_body_sender) = {
        let mut sender = upstream_send.lock().await;
        sender
            .send_request(upstream_request, request_end_stream)
            .map_err(|err| io_other("sending upstream h2 request", err))?
    };

    let mut req_preview = BodyPreview::new();
    while let Some(chunk) = req_body.data().await {
        let chunk = chunk.map_err(|err| io_other("reading client h2 request body", err))?;
        append_body_preview(
            &mut req_preview,
            &chunk,
            &request_headers,
            max_body_bytes,
        );
        upstream_body_sender
            .send_data(chunk.clone(), false)
            .map_err(|err| io_other("forwarding client h2 request data", err))?;
        let current_body = req_preview.text.clone();
        let _ = buffer.update(record_id, |record| {
            record.request_body = current_body;
            record.request_body_truncated = req_preview.truncated;
        });
    }
    if let Some(trailers) = req_body.trailers().await.map_err(|err| io_other("reading client h2 request trailers", err))? {
        upstream_body_sender
            .send_trailers(trailers.clone())
            .map_err(|err| io_other("forwarding client h2 request trailers", err))?;
        let trailer_headers = trailer_pairs(&trailers);
        let _ = buffer.update(record_id, |record| {
            record.request_headers.extend(trailer_headers);
        });
    } else if !request_end_stream {
        upstream_body_sender
            .send_data(Bytes::new(), true)
            .map_err(|err| io_other("closing upstream h2 request body", err))?;
    }

    let response = response_future
        .await
        .map_err(|err| io_other("waiting for upstream h2 response", err))?;
    let (response_parts, mut response_body) = response.into_parts();
    let response_headers = header_pairs(&response_parts.headers);
    let response_head = http::Response::builder()
        .status(response_parts.status)
        .version(response_parts.version)
        .body(())
        .map_err(|err| io_other("building downstream h2 response", err))?;
    let response_end_stream = response_body.is_end_stream();
    let mut client_body_sender = respond
        .send_response(response_head, response_end_stream)
        .map_err(|err| io_other("sending downstream h2 response head", err))?;
    let _ = buffer.update(record_id, |record| {
        record.status = Some(response_parts.status.as_u16());
        record.response_headers = response_headers.clone();
    });

    let mut res_preview = BodyPreview::new();
    let mut response_size = 0u64;
    while let Some(chunk) = response_body.data().await {
        let chunk = chunk.map_err(|err| io_other("reading upstream h2 response body", err))?;
        response_size += chunk.len() as u64;
        append_body_preview(
            &mut res_preview,
            &chunk,
            &response_headers,
            max_body_bytes,
        );
        client_body_sender
            .send_data(chunk.clone(), false)
            .map_err(|err| io_other("forwarding upstream h2 response data", err))?;
        let current_body = res_preview.text.clone();
        let _ = buffer.update(record_id, |record| {
            record.size = response_size;
            record.response_body = current_body;
            record.response_body_truncated = res_preview.truncated;
        });
    }
    if let Some(trailers) = response_body
        .trailers()
        .await
        .map_err(|err| io_other("reading upstream h2 response trailers", err))?
    {
        client_body_sender
            .send_trailers(trailers.clone())
            .map_err(|err| io_other("forwarding upstream h2 response trailers", err))?;
        let trailer_headers = trailer_pairs(&trailers);
        let _ = buffer.update(record_id, |record| {
            record.response_headers.extend(trailer_headers);
        });
    } else if !response_end_stream {
        client_body_sender
            .send_data(Bytes::new(), true)
            .map_err(|err| io_other("closing downstream h2 response body", err))?;
    }

    let _ = buffer.update(record_id, |record| {
        record.complete = true;
        record.duration = Some(start.elapsed().as_millis() as u64);
        record.size = response_size;
        record.request_body = req_preview.text.clone();
        record.request_body_truncated = req_preview.truncated;
        record.response_body = res_preview.text.clone();
        record.response_body_truncated = res_preview.truncated;
    });

    Ok(())
}

async fn establish_upstream_tunnel(
    upstream_addr: &str,
    target_host: &str,
) -> io::Result<TcpStream> {
    let mut upstream = TcpStream::connect(upstream_addr).await?;
    let connect_req = format!("CONNECT {target_host} HTTP/1.1\r\nHost: {target_host}\r\n\r\n");
    upstream.write_all(connect_req.as_bytes()).await?;

    let mut upstream_reader = BufReader::new(upstream);
    let mut status_line = String::new();
    upstream_reader.read_line(&mut status_line).await?;
    let upstream_ok = status_line.contains("200");

    loop {
        let mut line = String::new();
        upstream_reader.read_line(&mut line).await?;
        if line.trim().is_empty() {
            break;
        }
    }

    if !upstream_ok {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "upstream CONNECT rejected",
        ));
    }

    Ok(upstream_reader.into_inner())
}

#[derive(Clone, Debug)]
struct ParsedHttpMessage {
    start_line: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    wire_body: Vec<u8>,
    close: bool,
}

async fn read_http_message<R>(reader: &mut R) -> io::Result<Option<ParsedHttpMessage>>
where
    R: AsyncBufRead + Unpin,
{
    let Some((start_line, headers)) = read_http_head(reader).await? else {
        return Ok(None);
    };

    let (body, wire_body) = if header_contains_token(&headers, "transfer-encoding", "chunked") {
        read_chunked_body(reader).await?
    } else {
        let len = content_length(&headers);
        let mut body = vec![0u8; len];
        if len > 0 {
            reader.read_exact(&mut body).await?;
        }
        (body.clone(), body)
    };

    Ok(Some(ParsedHttpMessage {
        start_line,
        headers: headers.clone(),
        body,
        wire_body,
        close: should_close(&headers),
    }))
}

async fn read_http_head<R>(reader: &mut R) -> io::Result<Option<(String, Vec<(String, String)>)>>
where
    R: AsyncBufRead + Unpin,
{
    let mut start_line = String::new();
    loop {
        let bytes = reader.read_line(&mut start_line).await?;
        if bytes == 0 {
            return Ok(None);
        }
        if !start_line.trim().is_empty() {
            break;
        }
        start_line.clear();
    }
    let start_line = start_line.trim_end().to_string();

    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }

    Ok(Some((start_line, headers)))
}

async fn stream_http1_response_body<R, W>(
    reader: &mut R,
    writer: &mut W,
    start_line: &str,
    headers: &[(String, String)],
    buffer: &Arc<CaptureBuffer>,
    record_id: u64,
    start: std::time::Instant,
    max_body_bytes: usize,
) -> io::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut preview = BodyPreview::new();
    let mut total_size = 0u64;
    let status = parse_status_code(start_line);
    if status / 100 == 1 || status == 204 || status == 304 {
        let _ = buffer.update(record_id, |record| {
            record.complete = true;
            record.duration = Some(start.elapsed().as_millis() as u64);
        });
        return Ok(());
    }

    if header_contains_token(headers, "transfer-encoding", "chunked") {
        loop {
            let mut size_line = String::new();
            reader.read_line(&mut size_line).await?;
            if size_line.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected EOF in chunked response body",
                ));
            }
            writer.write_all(size_line.as_bytes()).await?;
            let size_token = size_line
                .trim()
                .split(';')
                .next()
                .unwrap_or_default()
                .trim();
            let chunk_len = usize::from_str_radix(size_token, 16).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid chunk size: {err}"),
                )
            })?;
            if chunk_len == 0 {
                loop {
                    let mut trailer = String::new();
                    reader.read_line(&mut trailer).await?;
                    if trailer.is_empty() {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "unexpected EOF in chunked trailer",
                        ));
                    }
                    writer.write_all(trailer.as_bytes()).await?;
                    if trailer.trim().is_empty() {
                        break;
                    }
                }
                break;
            }

            let mut chunk = vec![0u8; chunk_len];
            reader.read_exact(&mut chunk).await?;
            writer.write_all(&chunk).await?;
            total_size += chunk_len as u64;
            append_body_preview(&mut preview, &chunk, headers, max_body_bytes);

            let mut chunk_crlf = [0u8; 2];
            reader.read_exact(&mut chunk_crlf).await?;
            writer.write_all(&chunk_crlf).await?;
            writer.flush().await?;

            let current_body = preview.text.clone();
            let _ = buffer.update(record_id, |record| {
                record.size = total_size;
                record.response_body = current_body;
                record.response_body_truncated = preview.truncated;
                record.duration = Some(start.elapsed().as_millis() as u64);
            });
        }
    } else if let Some(len) = header_value(headers, "content-length").and_then(|value| value.parse::<usize>().ok()) {
        let mut remaining = len;
        while remaining > 0 {
            let to_read = remaining.min(8192);
            let mut chunk = vec![0u8; to_read];
            reader.read_exact(&mut chunk).await?;
            writer.write_all(&chunk).await?;
            writer.flush().await?;
            remaining -= to_read;
            total_size += to_read as u64;
            append_body_preview(&mut preview, &chunk, headers, max_body_bytes);
            let current_body = preview.text.clone();
            let _ = buffer.update(record_id, |record| {
                record.size = total_size;
                record.response_body = current_body;
                record.response_body_truncated = preview.truncated;
                record.duration = Some(start.elapsed().as_millis() as u64);
            });
        }
    } else if should_close(headers) {
        let mut chunk = [0u8; 8192];
        loop {
            let n = reader.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            writer.write_all(&chunk[..n]).await?;
            writer.flush().await?;
            total_size += n as u64;
            append_body_preview(&mut preview, &chunk[..n], headers, max_body_bytes);
            let current_body = preview.text.clone();
            let _ = buffer.update(record_id, |record| {
                record.size = total_size;
                record.response_body = current_body;
                record.response_body_truncated = preview.truncated;
                record.duration = Some(start.elapsed().as_millis() as u64);
            });
        }
    }

    let _ = buffer.update(record_id, |record| {
        record.complete = true;
        record.size = total_size;
        record.response_body = preview.text.clone();
        record.response_body_truncated = preview.truncated;
        record.duration = Some(start.elapsed().as_millis() as u64);
    });
    Ok(())
}

async fn read_chunked_body<R>(reader: &mut R) -> io::Result<(Vec<u8>, Vec<u8>)>
where
    R: AsyncBufRead + Unpin,
{
    let mut decoded = Vec::new();
    let mut wire = Vec::new();

    loop {
        let mut size_line = String::new();
        reader.read_line(&mut size_line).await?;
        if size_line.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "unexpected EOF in chunked body",
            ));
        }
        wire.extend_from_slice(size_line.as_bytes());
        let size_token = size_line
            .trim()
            .split(';')
            .next()
            .unwrap_or_default()
            .trim();
        let chunk_len = usize::from_str_radix(size_token, 16).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid chunk size: {err}"),
            )
        })?;

        if chunk_len == 0 {
            loop {
                let mut trailer = String::new();
                reader.read_line(&mut trailer).await?;
                if trailer.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "unexpected EOF in chunked trailer",
                    ));
                }
                wire.extend_from_slice(trailer.as_bytes());
                if trailer.trim().is_empty() {
                    break;
                }
            }
            break;
        }

        let mut chunk = vec![0u8; chunk_len];
        reader.read_exact(&mut chunk).await?;
        decoded.extend_from_slice(&chunk);
        wire.extend_from_slice(&chunk);

        let mut chunk_crlf = [0u8; 2];
        reader.read_exact(&mut chunk_crlf).await?;
        wire.extend_from_slice(&chunk_crlf);
    }

    Ok((decoded, wire))
}

async fn write_http_message<W>(
    writer: &mut W,
    start_line: &str,
    headers: &[(String, String)],
    wire_body: &[u8],
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    writer
        .write_all(format!("{start_line}\r\n").as_bytes())
        .await?;
    for (k, v) in headers {
        writer.write_all(format!("{k}: {v}\r\n").as_bytes()).await?;
    }
    writer.write_all(b"\r\n").await?;
    if !wire_body.is_empty() {
        writer.write_all(wire_body).await?;
    }
    writer.flush().await
}

fn content_length(headers: &[(String, String)]) -> usize {
    header_value(headers, "content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn should_close(headers: &[(String, String)]) -> bool {
    header_contains_token(headers, "connection", "close")
}

fn header_contains_token(headers: &[(String, String)], key: &str, token: &str) -> bool {
    header_value(headers, key)
        .map(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case(token))
        })
        .unwrap_or(false)
}

fn header_value<'a>(headers: &'a [(String, String)], key: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

fn parse_request_line(start_line: &str) -> (String, String) {
    let parts: Vec<&str> = start_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("GET").to_string();
    let path = parts.get(1).copied().unwrap_or("/").to_string();
    (method, path)
}

fn parse_status_code(start_line: &str) -> u16 {
    start_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn build_https_url(authority: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        format!("https://{authority}{path}")
    }
}

fn strip_port(target_host: &str) -> String {
    match target_host.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host.to_string(),
        _ => target_host.to_string(),
    }
}

fn preview_full_body(
    body: &[u8],
    headers: &[(String, String)],
    max_body_bytes: usize,
) -> BodyPreview {
    let mut preview = BodyPreview::new();
    if !body.is_empty() {
        append_body_preview(&mut preview, body, headers, max_body_bytes);
    }
    preview
}

fn append_body_preview(
    preview: &mut BodyPreview,
    body: &[u8],
    headers: &[(String, String)],
    max_body_bytes: usize,
) {
    if body.is_empty() {
        return;
    }

    let content_type = header_value(headers, "content-type").unwrap_or_default();
    let looks_text = content_type.starts_with("text/")
        || content_type.contains("json")
        || content_type.contains("xml")
        || content_type.contains("javascript")
        || content_type.contains("x-www-form-urlencoded");

    preview.bytes_seen += body.len() as u64;

    if !looks_text && std::str::from_utf8(body).is_err() {
        preview.binary = true;
        preview.text = Some(format!("[binary body, {} bytes]", preview.bytes_seen));
        return;
    }

    let limit = max_body_bytes.max(1);
    let current = preview.text.get_or_insert_with(String::new);
    if current.len() >= limit {
        preview.truncated = true;
        return;
    }
    let remaining = limit - current.len();
    let slice_len = body.len().min(remaining);
    current.push_str(&String::from_utf8_lossy(&body[..slice_len]));
    if body.len() > slice_len {
        preview.truncated = true;
    }
}

fn header_pairs(headers: &http::HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                value
                    .to_str()
                    .map(str::to_string)
                    .unwrap_or_else(|_| format!("<{} bytes>", value.as_bytes().len())),
            )
        })
        .collect()
}

fn trailer_pairs(headers: &http::HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            (
                format!("trailer:{}", name.as_str()),
                value
                    .to_str()
                    .map(str::to_string)
                    .unwrap_or_else(|_| format!("<{} bytes>", value.as_bytes().len())),
            )
        })
        .collect()
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

fn build_mitm_server_config(mitm: &MitmProxyConfig, hostname: &str) -> io::Result<ServerConfig> {
    install_rustls_provider();
    let issuer_key =
        KeyPair::from_pem(&mitm.ca_key_pem).map_err(|err| io_other("loading MITM key", err))?;
    let issuer = Issuer::from_ca_cert_pem(&mitm.ca_cert_pem, issuer_key)
        .map_err(|err| io_other("loading MITM CA", err))?;

    let mut params = CertificateParams::new(vec![hostname.to_string()])
        .map_err(|err| io_other("building leaf certificate params", err))?;
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, hostname);
    distinguished_name.push(DnType::OrganizationName, "ccp");
    distinguished_name.push(DnType::OrganizationalUnitName, "mitm-leaf");
    params.distinguished_name = distinguished_name;
    params.is_ca = IsCa::NoCa;
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

    let leaf_key = KeyPair::generate_rsa_for(&PKCS_RSA_SHA256, RsaKeySize::_2048)
        .map_err(|err| io_other("generating leaf key", err))?;
    let leaf_cert = params
        .signed_by(&leaf_key, &issuer)
        .map_err(|err| io_other("signing leaf certificate", err))?;

    let cert_chain = load_pem_certs(leaf_cert.pem().as_bytes())?;
    let key = rustls_pemfile::private_key(&mut Cursor::new(leaf_key.serialize_pem().into_bytes()))
        .map_err(|err| io_other("loading leaf private key", err))?
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing private key"))?;

    let mut server = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .map_err(|err| io_other("building server TLS config", err))?;
    server.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(server)
}

fn build_upstream_client_config(
    mitm: &MitmProxyConfig,
    client_alpn: Option<&[u8]>,
) -> io::Result<ClientConfig> {
    install_rustls_provider();
    let mut roots = RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    for cert in native.certs {
        let _ = roots.add(cert);
    }
    if let Some(extra_ca) = &mitm.upstream_ca_cert_pem {
        for cert in load_pem_certs(extra_ca.as_bytes())? {
            let _ = roots.add(cert);
        }
    }

    let mut client = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    client.alpn_protocols = match client_alpn {
        Some(protocol) if protocol == b"h2" => vec![b"h2".to_vec()],
        _ => vec![b"http/1.1".to_vec()],
    };
    Ok(client)
}

fn load_pem_certs(bytes: &[u8]) -> io::Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    rustls_pemfile::certs(&mut Cursor::new(bytes))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| io_other("loading PEM certificates", err))
}

fn io_other(context: &str, err: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::Other, format!("{context}: {err}"))
}

fn install_rustls_provider() {
    RUSTLS_PROVIDER_INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
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

    let mut upstream = TcpStream::connect(&config.upstream_addr).await?;
    upstream
        .write_all(format!("{first_line}\r\n").as_bytes())
        .await?;
    for (k, v) in &req_headers {
        upstream
            .write_all(format!("{k}: {v}\r\n").as_bytes())
            .await?;
    }
    upstream.write_all(b"\r\n").await?;

    let mut upstream_reader = BufReader::new(upstream);
    let mut status_line = String::new();
    upstream_reader.read_line(&mut status_line).await?;

    let status_code = parse_status_code(&status_line);
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

    let mut client_inner = client.into_inner();
    client_inner.write_all(status_line.as_bytes()).await?;
    for (k, v) in &res_headers {
        client_inner
            .write_all(format!("{k}: {v}\r\n").as_bytes())
            .await?;
    }
    client_inner.write_all(b"\r\n").await?;

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

    buffer.push(build_http_record(HttpRecordParams {
        method,
        url,
        tool: config.tool_name.clone(),
        protocol: "http/1.1".to_string(),
        connection_id: None,
        stream_id: None,
        status: status_code,
        size: content_length,
        start,
        complete: true,
        req_headers,
        res_headers,
        req_body: None,
        res_body: None,
        req_body_truncated: false,
        res_body_truncated: false,
    }));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CaptureBuffer, CaptureProxy, CaptureProxyConfig, MitmProxyConfig};
    use bytes::Bytes;
    use rcgen::{
        BasicConstraints, CertificateParams, CertifiedIssuer, DistinguishedName, DnType, IsCa,
        Issuer, KeyPair, KeyUsagePurpose, RsaKeySize, PKCS_RSA_SHA256,
    };
    use rustls::pki_types::ServerName;
    use rustls::{ClientConfig, RootCertStore, ServerConfig};
    use std::io::Cursor;
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::runtime::Runtime;
    use tokio_rustls::{TlsAcceptor, TlsConnector};

    #[test]
    fn mitm_connect_captures_http_bodies() {
        Runtime::new()
            .unwrap()
            .block_on(async { mitm_connect_captures_http_bodies_inner().await });
    }

    #[test]
    fn mitm_connect_falls_back_to_connect_record_when_no_http_message_is_seen() {
        Runtime::new()
            .unwrap()
            .block_on(async { mitm_connect_falls_back_to_connect_record_when_no_http_message_is_seen_inner().await });
    }

    #[test]
    fn mitm_connect_captures_h2_request_and_response_bodies() {
        Runtime::new()
            .unwrap()
            .block_on(async { mitm_connect_captures_h2_request_and_response_bodies_inner().await });
    }

    #[test]
    fn mitm_connect_captures_h2_streaming_response_body() {
        Runtime::new()
            .unwrap()
            .block_on(async { mitm_connect_captures_h2_streaming_response_body_inner().await });
    }

    #[test]
    fn mitm_connect_captures_chunked_http1_response_body() {
        Runtime::new()
            .unwrap()
            .block_on(async { mitm_connect_captures_chunked_http1_response_body_inner().await });
    }

    async fn mitm_connect_captures_http_bodies_inner() {
        let mitm_ca = test_ca("ccp-mitm-test-ca");
        let target_ca = test_ca("ccp-target-test-ca");
        let upstream = tokio::spawn(run_upstream_proxy(
            "api.example.test".to_string(),
            target_ca.ca_cert_pem.clone(),
            target_ca.ca_key_pem.clone(),
            true,
        ));
        let upstream_addr = upstream.await.unwrap();

        let buffer = CaptureBuffer::new(32);
        let proxy = CaptureProxy::start(
            CaptureProxyConfig {
                bind_addr: "127.0.0.1:0".to_string(),
                upstream_addr,
                tool_name: "claude".to_string(),
                mitm: Some(MitmProxyConfig {
                    ca_cert_pem: mitm_ca.ca_cert_pem.clone(),
                    ca_key_pem: mitm_ca.ca_key_pem.clone(),
                    upstream_ca_cert_pem: Some(target_ca.ca_cert_pem.clone()),
                    max_body_bytes: 16 * 1024,
                }),
            },
            buffer.clone(),
        )
        .await
        .unwrap();

        let mut client = TcpStream::connect(proxy.local_addr()).await.unwrap();
        client
            .write_all(
                b"CONNECT api.example.test:443 HTTP/1.1\r\nHost: api.example.test:443\r\n\r\n",
            )
            .await
            .unwrap();

        let mut tunnel_reader = BufReader::new(client);
        let mut status_line = String::new();
        tunnel_reader.read_line(&mut status_line).await.unwrap();
        assert!(status_line.contains("200"));
        loop {
            let mut line = String::new();
            tunnel_reader.read_line(&mut line).await.unwrap();
            if line.trim().is_empty() {
                break;
            }
        }

        let mut roots = RootCertStore::empty();
        super::install_rustls_provider();
        for cert in super::load_pem_certs(mitm_ca.ca_cert_pem.as_bytes()).unwrap() {
            roots.add(cert).unwrap();
        }
        let mut client_config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let tls = TlsConnector::from(Arc::new(client_config))
            .connect(
                ServerName::try_from("api.example.test".to_string()).unwrap(),
                tunnel_reader.into_inner(),
            )
            .await
            .unwrap();
        let mut tls = BufReader::new(tls);

        tls.get_mut()
            .write_all(
                b"POST /v1/messages HTTP/1.1\r\nHost: api.example.test\r\nContent-Type: application/json\r\nContent-Length: 4\r\nConnection: close\r\n\r\nping",
            )
            .await
            .unwrap();
        tls.get_mut().flush().await.unwrap();

        let mut response = Vec::new();
        match tls.read_to_end(&mut response).await {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {}
            Err(err) => panic!("unexpected TLS read error: {err}"),
        }
        let response_text = String::from_utf8_lossy(&response);
        assert!(response_text.contains("HTTP/1.1 200 OK"));
        assert!(response_text.contains("pong"));

        proxy.shutdown().await;

        let records = buffer.snapshot();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].method, "POST");
        assert_eq!(records[0].request_body.as_deref(), Some("ping"));
        assert_eq!(records[0].response_body.as_deref(), Some("pong"));
        assert_eq!(records[0].status, Some(200));
    }

    async fn mitm_connect_falls_back_to_connect_record_when_no_http_message_is_seen_inner() {
        let mitm_ca = test_ca("ccp-mitm-test-ca");
        let target_ca = test_ca("ccp-target-test-ca");
        let upstream = tokio::spawn(run_upstream_proxy(
            "api.example.test".to_string(),
            target_ca.ca_cert_pem.clone(),
            target_ca.ca_key_pem.clone(),
            false,
        ));
        let upstream_addr = upstream.await.unwrap();

        let buffer = CaptureBuffer::new(32);
        let proxy = CaptureProxy::start(
            CaptureProxyConfig {
                bind_addr: "127.0.0.1:0".to_string(),
                upstream_addr,
                tool_name: "claude".to_string(),
                mitm: Some(MitmProxyConfig {
                    ca_cert_pem: mitm_ca.ca_cert_pem.clone(),
                    ca_key_pem: mitm_ca.ca_key_pem.clone(),
                    upstream_ca_cert_pem: Some(target_ca.ca_cert_pem.clone()),
                    max_body_bytes: 16 * 1024,
                }),
            },
            buffer.clone(),
        )
        .await
        .unwrap();

        let mut client = TcpStream::connect(proxy.local_addr()).await.unwrap();
        client
            .write_all(
                b"CONNECT api.example.test:443 HTTP/1.1\r\nHost: api.example.test:443\r\n\r\n",
            )
            .await
            .unwrap();

        let mut tunnel_reader = BufReader::new(client);
        let mut status_line = String::new();
        tunnel_reader.read_line(&mut status_line).await.unwrap();
        assert!(status_line.contains("200"));
        loop {
            let mut line = String::new();
            tunnel_reader.read_line(&mut line).await.unwrap();
            if line.trim().is_empty() {
                break;
            }
        }

        let mut roots = RootCertStore::empty();
        super::install_rustls_provider();
        for cert in super::load_pem_certs(mitm_ca.ca_cert_pem.as_bytes()).unwrap() {
            roots.add(cert).unwrap();
        }
        let mut client_config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let tls = TlsConnector::from(Arc::new(client_config))
            .connect(
                ServerName::try_from("api.example.test".to_string()).unwrap(),
                tunnel_reader.into_inner(),
            )
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        drop(tls);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        proxy.shutdown().await;

        let records = buffer.snapshot();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].method, "CONNECT");
        assert_eq!(records[0].url, "api.example.test:443");
        assert_eq!(records[0].status, Some(200));
    }

    async fn mitm_connect_captures_h2_request_and_response_bodies_inner() {
        let mitm_ca = test_ca("ccp-mitm-test-ca");
        let target_ca = test_ca("ccp-target-test-ca");
        let upstream = tokio::spawn(run_upstream_h2_proxy(
            "api.example.test".to_string(),
            target_ca.ca_cert_pem.clone(),
            target_ca.ca_key_pem.clone(),
            false,
        ));
        let upstream_addr = upstream.await.unwrap();

        let buffer = CaptureBuffer::new(32);
        let proxy = CaptureProxy::start(
            CaptureProxyConfig {
                bind_addr: "127.0.0.1:0".to_string(),
                upstream_addr,
                tool_name: "claude".to_string(),
                mitm: Some(MitmProxyConfig {
                    ca_cert_pem: mitm_ca.ca_cert_pem.clone(),
                    ca_key_pem: mitm_ca.ca_key_pem.clone(),
                    upstream_ca_cert_pem: Some(target_ca.ca_cert_pem.clone()),
                    max_body_bytes: 16 * 1024,
                }),
            },
            buffer.clone(),
        )
        .await
        .unwrap();

        let mut client = TcpStream::connect(proxy.local_addr()).await.unwrap();
        client
            .write_all(
                b"CONNECT api.example.test:443 HTTP/1.1\r\nHost: api.example.test:443\r\n\r\n",
            )
            .await
            .unwrap();
        let mut tunnel_reader = BufReader::new(client);
        let mut status_line = String::new();
        tunnel_reader.read_line(&mut status_line).await.unwrap();
        assert!(status_line.contains("200"));
        loop {
            let mut line = String::new();
            tunnel_reader.read_line(&mut line).await.unwrap();
            if line.trim().is_empty() {
                break;
            }
        }

        let mut roots = RootCertStore::empty();
        super::install_rustls_provider();
        for cert in super::load_pem_certs(mitm_ca.ca_cert_pem.as_bytes()).unwrap() {
            roots.add(cert).unwrap();
        }
        let mut client_config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_config.alpn_protocols = vec![b"h2".to_vec()];
        let tls = TlsConnector::from(Arc::new(client_config))
            .connect(
                ServerName::try_from("api.example.test".to_string()).unwrap(),
                tunnel_reader.into_inner(),
            )
            .await
            .unwrap();
        let (mut send_request, connection) = h2::client::handshake(tls).await.unwrap();
        tokio::spawn(async move {
            let _ = connection.await;
        });

        let request = http::Request::builder()
            .method("POST")
            .uri("/v1/messages")
            .header("content-type", "application/json")
            .body(())
            .unwrap();
        let (response_future, mut send_stream) = send_request.send_request(request, false).unwrap();
        send_stream.send_data(Bytes::from_static(b"ping"), true).unwrap();
        let response = response_future.await.unwrap();
        assert_eq!(response.status(), 200);
        let mut body = response.into_body();
        let mut received = Vec::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            received.extend_from_slice(chunk.as_ref());
        }
        assert_eq!(received, b"pong");

        proxy.shutdown().await;
        let records = buffer.snapshot();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].protocol, "h2");
        assert_eq!(records[0].method, "POST");
        assert_eq!(records[0].request_body.as_deref(), Some("ping"));
        assert_eq!(records[0].response_body.as_deref(), Some("pong"));
        assert!(records[0].stream_id.is_some());
        assert!(records[0].complete);
    }

    async fn mitm_connect_captures_h2_streaming_response_body_inner() {
        let mitm_ca = test_ca("ccp-mitm-test-ca");
        let target_ca = test_ca("ccp-target-test-ca");
        let upstream = tokio::spawn(run_upstream_h2_proxy(
            "api.example.test".to_string(),
            target_ca.ca_cert_pem.clone(),
            target_ca.ca_key_pem.clone(),
            true,
        ));
        let upstream_addr = upstream.await.unwrap();

        let buffer = CaptureBuffer::new(32);
        let proxy = CaptureProxy::start(
            CaptureProxyConfig {
                bind_addr: "127.0.0.1:0".to_string(),
                upstream_addr,
                tool_name: "claude".to_string(),
                mitm: Some(MitmProxyConfig {
                    ca_cert_pem: mitm_ca.ca_cert_pem.clone(),
                    ca_key_pem: mitm_ca.ca_key_pem.clone(),
                    upstream_ca_cert_pem: Some(target_ca.ca_cert_pem.clone()),
                    max_body_bytes: 16 * 1024,
                }),
            },
            buffer.clone(),
        )
        .await
        .unwrap();

        let mut client = TcpStream::connect(proxy.local_addr()).await.unwrap();
        client
            .write_all(
                b"CONNECT api.example.test:443 HTTP/1.1\r\nHost: api.example.test:443\r\n\r\n",
            )
            .await
            .unwrap();
        let mut tunnel_reader = BufReader::new(client);
        let mut status_line = String::new();
        tunnel_reader.read_line(&mut status_line).await.unwrap();
        assert!(status_line.contains("200"));
        loop {
            let mut line = String::new();
            tunnel_reader.read_line(&mut line).await.unwrap();
            if line.trim().is_empty() {
                break;
            }
        }

        let mut roots = RootCertStore::empty();
        super::install_rustls_provider();
        for cert in super::load_pem_certs(mitm_ca.ca_cert_pem.as_bytes()).unwrap() {
            roots.add(cert).unwrap();
        }
        let mut client_config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_config.alpn_protocols = vec![b"h2".to_vec()];
        let tls = TlsConnector::from(Arc::new(client_config))
            .connect(
                ServerName::try_from("api.example.test".to_string()).unwrap(),
                tunnel_reader.into_inner(),
            )
            .await
            .unwrap();
        let (mut send_request, connection) = h2::client::handshake(tls).await.unwrap();
        tokio::spawn(async move {
            let _ = connection.await;
        });

        let request = http::Request::builder()
            .method("GET")
            .uri("/stream")
            .body(())
            .unwrap();
        let (response_future, _) = send_request.send_request(request, true).unwrap();
        let response = response_future.await.unwrap();
        assert_eq!(response.status(), 200);
        let mut body = response.into_body();
        let mut received = Vec::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            received.extend_from_slice(chunk.as_ref());
        }
        assert_eq!(received, b"hello world");

        proxy.shutdown().await;
        let records = buffer.snapshot();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].protocol, "h2");
        assert_eq!(records[0].response_body.as_deref(), Some("hello world"));
        assert!(records[0].complete);
    }

    async fn mitm_connect_captures_chunked_http1_response_body_inner() {
        let mitm_ca = test_ca("ccp-mitm-test-ca");
        let target_ca = test_ca("ccp-target-test-ca");
        let upstream = tokio::spawn(run_upstream_chunked_proxy(
            "api.example.test".to_string(),
            target_ca.ca_cert_pem.clone(),
            target_ca.ca_key_pem.clone(),
        ));
        let upstream_addr = upstream.await.unwrap();

        let buffer = CaptureBuffer::new(32);
        let proxy = CaptureProxy::start(
            CaptureProxyConfig {
                bind_addr: "127.0.0.1:0".to_string(),
                upstream_addr,
                tool_name: "claude".to_string(),
                mitm: Some(MitmProxyConfig {
                    ca_cert_pem: mitm_ca.ca_cert_pem.clone(),
                    ca_key_pem: mitm_ca.ca_key_pem.clone(),
                    upstream_ca_cert_pem: Some(target_ca.ca_cert_pem.clone()),
                    max_body_bytes: 16 * 1024,
                }),
            },
            buffer.clone(),
        )
        .await
        .unwrap();

        let mut client = TcpStream::connect(proxy.local_addr()).await.unwrap();
        client
            .write_all(
                b"CONNECT api.example.test:443 HTTP/1.1\r\nHost: api.example.test:443\r\n\r\n",
            )
            .await
            .unwrap();
        let mut tunnel_reader = BufReader::new(client);
        let mut status_line = String::new();
        tunnel_reader.read_line(&mut status_line).await.unwrap();
        assert!(status_line.contains("200"));
        loop {
            let mut line = String::new();
            tunnel_reader.read_line(&mut line).await.unwrap();
            if line.trim().is_empty() {
                break;
            }
        }

        let mut roots = RootCertStore::empty();
        super::install_rustls_provider();
        for cert in super::load_pem_certs(mitm_ca.ca_cert_pem.as_bytes()).unwrap() {
            roots.add(cert).unwrap();
        }
        let mut client_config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let tls = TlsConnector::from(Arc::new(client_config))
            .connect(
                ServerName::try_from("api.example.test".to_string()).unwrap(),
                tunnel_reader.into_inner(),
            )
            .await
            .unwrap();
        let mut tls = BufReader::new(tls);

        tls.get_mut()
            .write_all(
                b"GET /stream HTTP/1.1\r\nHost: api.example.test\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        tls.get_mut().flush().await.unwrap();

        let mut response = Vec::new();
        match tls.read_to_end(&mut response).await {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {}
            Err(err) => panic!("unexpected TLS read error: {err}"),
        }
        let response_text = String::from_utf8_lossy(&response);
        assert!(response_text.contains("hello "));
        assert!(response_text.contains("world"));

        proxy.shutdown().await;
        let records = buffer.snapshot();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].protocol, "http/1.1");
        assert_eq!(records[0].response_body.as_deref(), Some("hello world"));
        assert!(records[0].complete);
    }

    async fn run_upstream_proxy(
        hostname: String,
        ca_cert_pem: String,
        ca_key_pem: String,
        expect_http_request: bool,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(socket);
            let mut first_line = String::new();
            reader.read_line(&mut first_line).await.unwrap();
            assert!(first_line.starts_with("CONNECT "));
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                if line.trim().is_empty() {
                    break;
                }
            }
            let mut socket = reader.into_inner();
            socket
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();

            let tls_server = TlsAcceptor::from(Arc::new(test_server_config(
                &hostname,
                &ca_cert_pem,
                &ca_key_pem,
            )));
            let tls = tls_server.accept(socket).await.unwrap();
            if !expect_http_request {
                drop(tls);
                return;
            }
            assert_ne!(
                tls.get_ref().1.alpn_protocol(),
                Some(b"h2".as_slice()),
                "http/1.1 upstream test server must not negotiate h2"
            );
            let mut tls = BufReader::new(tls);

            let mut first = String::new();
            tls.read_line(&mut first).await.unwrap();
            assert!(first.starts_with("POST /v1/messages "));
            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                tls.read_line(&mut line).await.unwrap();
                if let Some((k, v)) = line.trim().split_once(':') {
                    if k.eq_ignore_ascii_case("content-length") {
                        content_length = v.trim().parse().unwrap();
                    }
                }
                if line.trim().is_empty() {
                    break;
                }
            }
            let mut body = vec![0u8; content_length];
            tls.read_exact(&mut body).await.unwrap();
            assert_eq!(body, b"ping");

            tls.get_mut()
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 4\r\nConnection: close\r\n\r\npong",
                )
                .await
                .unwrap();
            tls.get_mut().flush().await.unwrap();
        });

        addr
    }

    async fn run_upstream_h2_proxy(
        hostname: String,
        ca_cert_pem: String,
        ca_key_pem: String,
        streaming: bool,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(socket);
            let mut first_line = String::new();
            reader.read_line(&mut first_line).await.unwrap();
            assert!(first_line.starts_with("CONNECT "));
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                if line.trim().is_empty() {
                    break;
                }
            }
            let mut socket = reader.into_inner();
            socket
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();

            let tls_server = TlsAcceptor::from(Arc::new(test_server_config(
                &hostname,
                &ca_cert_pem,
                &ca_key_pem,
            )));
            let tls = tls_server.accept(socket).await.unwrap();
            let mut connection = h2::server::handshake(tls).await.unwrap();
            while let Some(result) = connection.accept().await {
                let (request, mut respond) = result.unwrap();
                let (_parts, mut body) = request.into_parts();
                let mut request_body = Vec::new();
                while let Some(chunk) = body.data().await {
                    request_body.extend_from_slice(&chunk.unwrap());
                }
                let response = http::Response::builder()
                    .status(200)
                    .header("content-type", "text/plain")
                    .body(())
                    .unwrap();
                let mut send = respond.send_response(response, false).unwrap();
                if streaming {
                    send.send_data(Bytes::from_static(b"hello "), false).unwrap();
                    send.send_data(Bytes::from_static(b"world"), true).unwrap();
                } else {
                    assert_eq!(request_body, b"ping");
                    send.send_data(Bytes::from_static(b"pong"), true).unwrap();
                }
            }
        });

        addr
    }

    async fn run_upstream_chunked_proxy(
        hostname: String,
        ca_cert_pem: String,
        ca_key_pem: String,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(socket);
            let mut first_line = String::new();
            reader.read_line(&mut first_line).await.unwrap();
            assert!(first_line.starts_with("CONNECT "));
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                if line.trim().is_empty() {
                    break;
                }
            }
            let mut socket = reader.into_inner();
            socket
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();

            let tls_server = TlsAcceptor::from(Arc::new(test_server_config(
                &hostname,
                &ca_cert_pem,
                &ca_key_pem,
            )));
            let tls = tls_server.accept(socket).await.unwrap();
            let mut tls = BufReader::new(tls);
            let mut first = String::new();
            tls.read_line(&mut first).await.unwrap();
            assert!(first.starts_with("GET /stream "));
            loop {
                let mut line = String::new();
                tls.read_line(&mut line).await.unwrap();
                if line.trim().is_empty() {
                    break;
                }
            }
            tls.get_mut()
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            tls.get_mut().write_all(b"6\r\nhello \r\n").await.unwrap();
            tls.get_mut().flush().await.unwrap();
            tls.get_mut().write_all(b"5\r\nworld\r\n").await.unwrap();
            tls.get_mut().write_all(b"0\r\n\r\n").await.unwrap();
            tls.get_mut().flush().await.unwrap();
        });

        addr
    }

    struct TestCa {
        ca_cert_pem: String,
        ca_key_pem: String,
    }

    fn test_ca(common_name: &str) -> TestCa {
        let mut distinguished_name = DistinguishedName::new();
        distinguished_name.push(DnType::CommonName, common_name);
        distinguished_name.push(DnType::OrganizationName, "ccp-tests");

        let mut params = CertificateParams::default();
        params.distinguished_name = distinguished_name;
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        let signing_key = KeyPair::generate_rsa_for(&PKCS_RSA_SHA256, RsaKeySize::_2048).unwrap();
        let issuer = CertifiedIssuer::self_signed(params, signing_key).unwrap();
        TestCa {
            ca_cert_pem: issuer.pem(),
            ca_key_pem: issuer.key().serialize_pem(),
        }
    }

    fn test_server_config(hostname: &str, ca_cert_pem: &str, ca_key_pem: &str) -> ServerConfig {
        super::install_rustls_provider();
        let issuer_key = KeyPair::from_pem(ca_key_pem).unwrap();
        let issuer = Issuer::from_ca_cert_pem(ca_cert_pem, issuer_key).unwrap();
        let mut params = CertificateParams::new(vec![hostname.to_string()]).unwrap();
        let mut distinguished_name = DistinguishedName::new();
        distinguished_name.push(DnType::CommonName, hostname);
        params.distinguished_name = distinguished_name;
        let leaf_key = KeyPair::generate_rsa_for(&PKCS_RSA_SHA256, RsaKeySize::_2048).unwrap();
        let cert = params.signed_by(&leaf_key, &issuer).unwrap();
        let chain = super::load_pem_certs(cert.pem().as_bytes()).unwrap();
        let key =
            rustls_pemfile::private_key(&mut Cursor::new(leaf_key.serialize_pem().into_bytes()))
                .unwrap()
                .unwrap();
        let mut server = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .unwrap();
        server.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        server
    }
}
