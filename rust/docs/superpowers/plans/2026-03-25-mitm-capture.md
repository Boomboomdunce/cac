# MITM Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an initial MITM HTTPS capture path for Claude sessions with separate MITM certificates, merged Node trust, explicit readiness checks, and HTTP/1.1 interception support.

**Architecture:** Keep the current mTLS path intact and add a parallel MITM certificate hierarchy plus a merged CA bundle for Node trust. Extend the sidecar capture proxy so CONNECT can optionally terminate TLS, inspect HTTP/1.1 traffic, and still use the configured upstream proxy.

**Tech Stack:** Rust, Tokio, rcgen, rustls/tokio-rustls, existing Tauri desktop and CLI launch pipeline

---

### Task 1: Certificate Materials And Trust Bundle

**Files:**
- Modify: `crates/store/src/cert_store.rs`
- Modify: `crates/store/src/lib.rs`
- Test: `crates/store/src/cert_store.rs`

- [ ] **Step 1: Write the failing test**
Add a test that `ensure_mitm_certificates` creates a MITM root CA, a merged Node bundle, and preserves the existing mTLS CA paths.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p store ensure_mitm`
Expected: FAIL because the MITM certificate APIs do not exist yet.

- [ ] **Step 3: Write minimal implementation**
Add MITM certificate material types and generation helpers using owner-only files under `certs/mitm`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p store ensure_mitm`
Expected: PASS.

### Task 2: Claude Launch Trust Wiring

**Files:**
- Modify: `apps/ccp/src/main.rs`
- Modify: `tests/e2e/run_claude_smoke.rs`
- Modify: `tests/fixtures/fake_claude.js`

- [ ] **Step 1: Write the failing test**
Extend the existing Claude smoke test to expect the merged CA bundle path in `NODE_EXTRA_CA_CERTS`.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p ccp --test run_claude_smoke -- --nocapture`
Expected: FAIL because launch still points to the mTLS CA only.

- [ ] **Step 3: Write minimal implementation**
Ensure launch materializes MITM certificates and uses the merged bundle for Node trust injection.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p ccp --test run_claude_smoke -- --nocapture`
Expected: PASS.

### Task 3: MITM Readiness Checks

**Files:**
- Modify: `crates/doctor/src/checks.rs`
- Modify: `apps/ccp-desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/ccp-desktop/src/locales/en.json`
- Modify: `apps/ccp-desktop/src/locales/zh-CN.json`
- Test: `apps/ccp-desktop/src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Write the failing test**
Add tests for MITM readiness status when the MITM CA or merged bundle is missing.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p ccp-desktop mitm -- --nocapture`
Expected: FAIL because readiness checks do not exist.

- [ ] **Step 3: Write minimal implementation**
Expose readiness state and explicit warnings from doctor/desktop setup APIs.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p ccp-desktop mitm -- --nocapture`
Expected: PASS.

### Task 4: HTTP/1.1 MITM CONNECT Handling

**Files:**
- Modify: `crates/sidecar/src/proxy.rs`
- Modify: `crates/sidecar/src/capture.rs`
- Modify: `crates/sidecar/src/lib.rs`
- Test: `crates/sidecar/src/proxy.rs`

- [ ] **Step 1: Write the failing test**
Add an async test that drives a CONNECT request through the local capture proxy, performs a TLS handshake with the MITM leaf certificate, and verifies an HTTP/1.1 request/response body is captured.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p sidecar mitm -- --nocapture`
Expected: FAIL because CONNECT still bridges raw bytes.

- [ ] **Step 3: Write minimal implementation**
Add optional MITM mode for CONNECT handling, host certificate issuance, bounded text-body capture, and upstream TLS forwarding.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p sidecar mitm -- --nocapture`
Expected: PASS.

### Task 5: End-To-End Verification

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Run targeted verification**
Run:
`cargo test -p store ensure_mitm`
`cargo test -p ccp --test run_claude_smoke -- --nocapture`
`cargo test -p ccp-desktop mitm -- --nocapture`
`cargo test -p sidecar mitm -- --nocapture`

- [ ] **Step 2: Run broader regression checks**
Run:
`cargo test -p ccp --test cli_profile -- --nocapture`
`cargo test -p ccp --test launch_plan -- --nocapture`
`cargo test -p ccp-desktop -- --nocapture`

- [ ] **Step 3: Update documentation**
Document MITM readiness behavior, trust model, and current protocol limitations.
