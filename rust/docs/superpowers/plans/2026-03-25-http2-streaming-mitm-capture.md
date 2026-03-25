# HTTP/2 Streaming MITM Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend CCP MITM capture so HTTPS traffic routed through CCP can surface HTTP/1.1, HTTP/2, streaming, and long-lived responses in the GUI, with clear fallback behavior for non-HTTP TLS.

**Architecture:** Upgrade `sidecar` from final-record-only capture to incremental capture records that can be updated in place. After MITM TLS succeeds, select an HTTP/1.1 or HTTP/2 handler from ALPN, stream bodies through while appending bounded previews, and emit updates that the GUI merges by record ID.

**Tech Stack:** Rust, `tokio`, `tokio-rustls`, `h2`, Tauri, React, TypeScript

---

### Task 1: Upgrade Capture Record Model

**Files:**
- Modify: `rust/crates/sidecar/src/capture.rs`
- Modify: `rust/crates/sidecar/src/lib.rs`
- Modify: `rust/apps/ccp-desktop/src/pages/TrafficCapture.tsx`
- Modify: `rust/apps/ccp-desktop/src/components/RequestDetail.tsx`

- [ ] Add protocol, completion, truncation, connection ID, and stream ID fields to `CapturedRequest`.
- [ ] Add `CaptureBuffer::create`, `CaptureBuffer::replace`, and `CaptureBuffer::snapshot` support for in-place updates keyed by record ID.
- [ ] Update frontend request types to match the expanded schema.
- [ ] Change frontend event handling from append-only to upsert-by-`id`.
- [ ] Surface completion/protocol hints in table rows and detail views.

### Task 2: Add HTTP/2 MITM Handling

**Files:**
- Modify: `rust/crates/sidecar/Cargo.toml`
- Modify: `rust/crates/sidecar/src/proxy.rs`

- [ ] Add an `h2` dependency.
- [ ] Stop forcing ALPN to only `http/1.1`; advertise `h2` and `http/1.1`.
- [ ] Split MITM handling into HTTP/1.1, HTTP/2, and fallback paths based on negotiated ALPN.
- [ ] Implement an HTTP/2 proxy loop that forwards headers/data/trailers between client and upstream while updating capture records incrementally.
- [ ] Preserve bounded body previews and binary-safe fallback previews.

### Task 3: Improve Streaming and Long-Lived HTTP/1.1 Capture

**Files:**
- Modify: `rust/crates/sidecar/src/proxy.rs`

- [ ] Refactor HTTP/1.1 MITM handling to create records early and update them as request/response bodies stream.
- [ ] Support incremental chunked-response previews instead of waiting for the full body to finish.
- [ ] Mark long-lived responses as incomplete while open and complete when closed.
- [ ] Keep non-text payloads safe by previewing text only and recording truncation flags.

### Task 4: Preserve Clear Fallback Semantics

**Files:**
- Modify: `rust/crates/sidecar/src/proxy.rs`
- Modify: `rust/apps/ccp-desktop/src/locales/en.json`
- Modify: `rust/apps/ccp-desktop/src/locales/zh-CN.json`

- [ ] Record `CONNECT` metadata whenever MITM succeeds but the decrypted traffic is not decodable as HTTP/1.1 or HTTP/2.
- [ ] Distinguish protocol fallback from outright proxy failure in the captured record/status text.
- [ ] Update GUI copy to explain protocol visibility and fallback states.

### Task 5: Keep GUI Self-Traffic Out of Capture Results

**Files:**
- Modify: `rust/apps/ccp-desktop/src-tauri/src/commands/mod.rs`

- [ ] Ensure egress-IP probing uses the upstream proxy directly instead of the local capture sidecar.
- [ ] Verify the traffic page no longer shows GUI self-check requests as Claude traffic.

### Task 6: Verification

**Files:**
- Modify: `rust/crates/sidecar/src/proxy.rs`
- Modify: `rust/apps/ccp-desktop/src-tauri/src/commands/mod.rs`

- [ ] Add/expand `sidecar` tests for:
- [ ] HTTP/1.1 MITM body capture
- [ ] HTTP/2 unary capture
- [ ] HTTP/2 streaming capture updates
- [ ] MITM fallback to `CONNECT` metadata
- [ ] Add/expand `ccp-desktop` tests for setup/profile update behavior touched by the new schema.
- [ ] Run `cargo test -p sidecar -- --nocapture`.
- [ ] Run `cargo test -p ccp-desktop -- --nocapture`.
- [ ] Run `npm run build` in `rust/apps/ccp-desktop`.
- [ ] Run `cargo test --workspace -- --nocapture`.
