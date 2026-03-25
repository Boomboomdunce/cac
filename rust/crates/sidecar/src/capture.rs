use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;

/// A single captured request/response record.
#[derive(Clone, Debug, Serialize)]
pub struct CapturedRequest {
    pub id: u64,
    pub timestamp: String,
    pub tool: String,
    pub protocol: String,
    pub connection_id: Option<u64>,
    pub stream_id: Option<u64>,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub size: u64,
    pub duration: Option<u64>,
    pub complete: bool,
    pub request_body_truncated: bool,
    pub response_body_truncated: bool,
    pub category: String,
    pub blocked_reason: Option<String>,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Option<String>,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Option<String>,
}

/// Ring buffer that holds captured requests up to a memory limit.
pub struct CaptureBuffer {
    inner: Mutex<BufferInner>,
    tx: broadcast::Sender<CapturedRequest>,
}

struct BufferInner {
    requests: VecDeque<CapturedRequest>,
    next_id: u64,
    max_entries: usize,
}

impl CaptureBuffer {
    pub fn new(max_entries: usize) -> Arc<Self> {
        let (tx, _) = broadcast::channel(4096);
        Arc::new(Self {
            inner: Mutex::new(BufferInner {
                requests: VecDeque::with_capacity(max_entries.min(10000)),
                next_id: 1,
                max_entries,
            }),
            tx,
        })
    }

    /// Create a new captured request. Returns the assigned ID.
    pub fn create(&self, mut req: CapturedRequest) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        let id = inner.next_id;
        inner.next_id += 1;
        req.id = id;

        // Evict oldest if at capacity
        if inner.requests.len() >= inner.max_entries {
            inner.requests.pop_front();
        }
        let clone = req.clone();
        inner.requests.push_back(req);

        // Broadcast to subscribers (ignore if no receivers)
        let _ = self.tx.send(clone);
        id
    }

    /// Backward-compatible append helper.
    pub fn push(&self, req: CapturedRequest) -> u64 {
        self.create(req)
    }

    /// Update an existing captured request in place and broadcast the latest value.
    pub fn update<F>(&self, id: u64, update: F) -> Option<CapturedRequest>
    where
        F: FnOnce(&mut CapturedRequest),
    {
        let mut inner = self.inner.lock().unwrap();
        let req = inner.requests.iter_mut().find(|req| req.id == id)?;
        update(req);
        let clone = req.clone();
        let _ = self.tx.send(clone.clone());
        Some(clone)
    }

    /// Subscribe to new captured requests.
    pub fn subscribe(&self) -> broadcast::Receiver<CapturedRequest> {
        self.tx.subscribe()
    }

    /// Get all currently buffered requests.
    pub fn snapshot(&self) -> Vec<CapturedRequest> {
        let inner = self.inner.lock().unwrap();
        inner.requests.iter().cloned().collect()
    }

    /// Clear all buffered requests.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.requests.clear();
    }
}

/// Helper to build a CapturedRequest for a CONNECT tunnel.
pub fn build_connect_record(
    host: &str,
    tool: &str,
    bytes_up: u64,
    bytes_down: u64,
    start: Instant,
    success: bool,
    connection_id: Option<u64>,
) -> CapturedRequest {
    let now = chrono_now();
    CapturedRequest {
        id: 0, // assigned by buffer
        timestamp: now,
        tool: tool.to_string(),
        protocol: "connect".to_string(),
        connection_id,
        stream_id: None,
        method: "CONNECT".to_string(),
        url: host.to_string(),
        status: if success { Some(200) } else { Some(502) },
        size: bytes_down,
        duration: Some(start.elapsed().as_millis() as u64),
        complete: true,
        request_body_truncated: false,
        response_body_truncated: false,
        category: "normal".to_string(),
        blocked_reason: None,
        request_headers: vec![
            ("Host".to_string(), host.to_string()),
            ("X-Bytes-Up".to_string(), bytes_up.to_string()),
        ],
        request_body: None,
        response_headers: vec![],
        response_body: None,
    }
}

/// Build a record for a blocked DNS request.
pub fn build_blocked_record(host: &str, tool: &str, reason: &str) -> CapturedRequest {
    CapturedRequest {
        id: 0,
        timestamp: chrono_now(),
        tool: tool.to_string(),
        protocol: "connect".to_string(),
        connection_id: None,
        stream_id: None,
        method: "CONNECT".to_string(),
        url: host.to_string(),
        status: None,
        size: 0,
        duration: None,
        complete: true,
        request_body_truncated: false,
        response_body_truncated: false,
        category: "blocked".to_string(),
        blocked_reason: Some(reason.to_string()),
        request_headers: vec![],
        request_body: None,
        response_headers: vec![],
        response_body: None,
    }
}

/// Parameters for building an HTTP record.
pub struct HttpRecordParams {
    pub method: String,
    pub url: String,
    pub tool: String,
    pub protocol: String,
    pub connection_id: Option<u64>,
    pub stream_id: Option<u64>,
    pub status: u16,
    pub size: u64,
    pub start: Instant,
    pub complete: bool,
    pub req_headers: Vec<(String, String)>,
    pub res_headers: Vec<(String, String)>,
    pub req_body: Option<String>,
    pub res_body: Option<String>,
    pub req_body_truncated: bool,
    pub res_body_truncated: bool,
}

/// Build a record for an HTTP (non-TLS) request.
pub fn build_http_record(params: HttpRecordParams) -> CapturedRequest {
    CapturedRequest {
        id: 0,
        timestamp: chrono_now(),
        tool: params.tool,
        protocol: params.protocol,
        connection_id: params.connection_id,
        stream_id: params.stream_id,
        method: params.method,
        url: params.url,
        status: Some(params.status),
        size: params.size,
        duration: Some(params.start.elapsed().as_millis() as u64),
        complete: params.complete,
        request_body_truncated: params.req_body_truncated,
        response_body_truncated: params.res_body_truncated,
        category: "normal".to_string(),
        blocked_reason: None,
        request_headers: params.req_headers,
        request_body: params.req_body,
        response_headers: params.res_headers,
        response_body: params.res_body,
    }
}

fn chrono_now() -> String {
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
