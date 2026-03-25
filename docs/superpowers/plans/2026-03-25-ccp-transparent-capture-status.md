# CCP Transparent Capture Status Summary

Date: 2026-03-25

## Current Workspace and Branch

- Main repository worktree:
  - `/Users/kpas/SynologyDrive/Github/cac`
  - branch: `docs/rust-refactor-design`
- Active implementation worktree used for the Rust/desktop changes:
  - `/Users/kpas/SynologyDrive/Github/cac/.worktrees/rust-workspace-foundation`
  - branch: `feat/rust-workspace-foundation`
- Current Rust workspace root:
  - `/Users/kpas/SynologyDrive/Github/cac/.worktrees/rust-workspace-foundation/rust`

This means the current development is happening in a dedicated git worktree, not in the main repository checkout.

## What Has Been Implemented

### 1. CLI and GUI state-root alignment

- CLI and GUI were aligned around the same CCP state-root behavior.
- GUI and CLI now share the same `.ccp-rust` state layout expectations.
- Related setup, wrapper-install, profile-state, and diagnostics logic was extended in the desktop app.

### 2. Explicit proxy capture path

- The original Rust sidecar explicit-proxy backend remains in place.
- CLI still injects `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` / `NODE_USE_ENV_PROXY` where applicable.
- Stale `sidecar_port` files now produce an explicit warning instead of silently masking the failure.

### 3. MITM certificate preparation and trust flow

- Desktop setup flow now supports preparing CCP-managed MITM materials.
- System trust installation/removal hooks were added for macOS.
- Claude launched through CCP already trusts the CCP MITM root through managed Node trust configuration.

### 4. HTTP body capture in the Rust sidecar

- The Rust sidecar path was extended to capture HTTP message bodies after MITM.
- Coverage exists for:
  - HTTP/1.1 bodies
  - HTTP/2 request/response bodies
  - HTTP/2 streaming responses
  - fallback when a CONNECT tunnel does not yield decodable HTTP

### 5. Desktop traffic-capture backend abstraction

- Desktop capture manager now supports two backend families:
  - explicit local proxy backend
  - transparent local-capture backend
- Initial transparent backend integration was added through `mitmdump --mode local`.
- Desktop UI was updated to show:
  - active backend kind
  - backend-specific hints
  - setup gating and readiness warnings

### 6. Transparent-capture readiness detection

- Desktop no longer treats "`mitmdump` binary exists" as equivalent to "transparent capture is usable".
- On macOS, desktop now checks the Mitmproxy Redirector extension state through `systemextensionsctl list`.
- If the redirector is not enabled, GUI and backend status now report transparent capture as unavailable with explicit guidance.

## Key Files Changed In This Phase

- `apps/ccp-desktop/src-tauri/src/mitmproxy_backend.rs`
- `apps/ccp-desktop/src-tauri/src/capture_manager.rs`
- `apps/ccp-desktop/src-tauri/src/commands/mod.rs`
- `apps/ccp-desktop/src/pages/TrafficCapture.tsx`
- `apps/ccp-desktop/src/pages/Settings.tsx`
- `apps/ccp-desktop/src/components/SetupAssistant.tsx`
- `apps/ccp-desktop/src/locales/en.json`
- `apps/ccp-desktop/src/locales/zh-CN.json`

There are also broader in-progress changes elsewhere in the worktree related to setup, doctor, launcher, store, sidecar, and CLI behavior.

## Verified Results

The following verification commands were run successfully in the Rust worktree:

- `cargo test -p ccp-desktop -- --nocapture`
- `npm run build` in `rust/apps/ccp-desktop`
- `cargo test --workspace -- --test-threads=1`

These checks passed after the readiness-detection fixes.

## Real Root Cause Confirmed On This macOS Machine

The most important confirmed result is:

- `mitmdump --mode local` did not capture traffic on this machine even outside CCP integration.
- This was traced to the Mitmproxy Redirector macOS system extension not being fully approved.
- `systemextensionsctl list` currently shows:
  - `org.mitmproxy.macos-redirector.network-extension ... [activated waiting for user]`

This means the failure was not primarily a CCP GUI bug. CCP initially misclassified transparent capture as available, but the actual runtime blocker on this machine is macOS system-extension approval.

## Important Architecture Conclusions

### 1. Using `mitmproxy` today introduces an external tool dependency

- `mitmproxy` main product is not a single Rust-native library we can embed directly today.
- The main product is Python-driven and depends on `mitmproxy_rs` for several lower-level features.
- Depending on `mitmdump` conflicts with the product goal of shipping CCP as a self-controlled binary/runtime.

### 2. Re-implementing or forking the local-capture backend in Rust is feasible

- The transparent/local-capture idea can be implemented natively inside CCP.
- The correct reference layer is not only the `mitmproxy` Python repository, but especially `mitmproxy_rs`.
- The current local checkout for `mitmproxy` can be used as architecture reference.
- The local checkout for `mitmproxy_rs` should be used for lower-level platform capability study and possible vendoring/forking.

### 3. Replacing `mitmdump` with Rust code does not remove macOS platform constraints

- For macOS local application interception, the constraint is the operating system, not Python specifically.
- If CCP wants true transparent capture of local app traffic on macOS without relying on `HTTP_PROXY`, the likely path is still a system-level interception mechanism such as Network Extension / System Extension.
- Rewriting in Rust can remove the external `mitmdump` dependency.
- Rewriting in Rust does not inherently remove the need for Apple approval, signing, notarization, or user approval if the chosen interception model requires those OS facilities.

### 4. Old transparent routing and local app capture are different things

- macOS `pf`-based transparent proxying is not the same as per-process local transparent capture.
- The current user goal is much closer to local per-process capture for Claude/Node than to classic gateway-style transparent proxying.
- That is why `mitmproxy local` was investigated rather than only the older `transparent` mode.

## Current Product State

### Works now

- Explicit local proxy capture path through CCP sidecar
- MITM preparation and trust tooling
- HTTP/1.1 and HTTP/2 body capture on the Rust sidecar path
- Setup assistant and settings improvements
- Better stale-port and readiness diagnostics

### Partially works now

- Transparent backend integration through `mitmdump`
- GUI readiness and warning logic around transparent capture

### Does not work end-to-end yet on this macOS machine

- Transparent local capture through the Mitmproxy backend

Reason:

- The Mitmproxy Redirector system extension is still in `waiting for user` state on this machine.

## Recommended Next Step

The recommended direction is:

1. Stop treating `mitmdump` as the long-term production backend.
2. Study `/Users/kpas/SynologyDrive/Github/mitmproxy_rs` as the real technical reference for local redirect/capture primitives.
3. Produce a CCP-native transparent-capture design that:
   - keeps CCP MITM and HTTP parsing in Rust
   - replaces `mitmdump` with CCP-owned platform backends
   - separates backend design by platform:
     - macOS
     - Windows
     - Linux
4. Decide explicitly whether macOS support should:
   - use a signed system extension path
   - or fall back to explicit proxy capture only

## Notes For Resume

- The desktop transparent-capture readiness code is already in place and tested.
- The currently observed macOS blocker is real and reproducible.
- The next meaningful design/implementation phase should focus on CCP-native transparent capture, not further polishing `mitmdump` integration.
