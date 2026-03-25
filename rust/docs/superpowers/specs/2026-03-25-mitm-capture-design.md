# MITM Capture Design

**Date:** 2026-03-25

## Goal

Add HTTPS MITM capture for `ccp run claude` without breaking the existing upstream-proxy and mTLS flow. The first delivery focuses on Node-based Claude sessions, owner-only certificate storage, explicit health checks, and HTTP/1.1 request/response capture.

## Current State

- The desktop capture proxy accepts HTTP proxy traffic and forwards `CONNECT` upstream without TLS interception.
- Claude launch already injects `NODE_OPTIONS`, `NODE_EXTRA_CA_CERTS`, `CCP_MTLS_*`, and proxy env vars.
- The existing CA under `certs/ca` is for upstream mTLS and must remain separate from MITM.

## Proposed Architecture

### Certificate Model

- Keep the existing mTLS CA unchanged.
- Add a dedicated MITM CA under `certs/mitm`.
- Generate a merged CA bundle for Node trust that contains both the mTLS CA and the MITM CA.
- Cache per-host MITM leaf certificates signed by the MITM CA.

### Launch and Trust Model

- `ccp run claude` continues to inject proxy settings.
- Claude trust is handled per process by pointing `NODE_EXTRA_CA_CERTS` at the merged bundle.
- GUI exposes MITM readiness state and may later add optional system-keychain trust for non-Node clients.

### Proxy Data Flow

1. Client sends `CONNECT host:443`.
2. Sidecar answers `200 Connection Established`.
3. Sidecar terminates TLS locally with a leaf certificate for the requested host.
4. Sidecar opens its own upstream `CONNECT` tunnel to the target through the configured proxy.
5. Sidecar initiates upstream TLS to the real target and forwards decrypted HTTP messages while capturing headers and bounded text bodies.

## Boundaries

- Phase 1 targets HTTP/1.1 request and response capture.
- HTTP/2, WebSocket, binary streaming, and system trust installation stay out of the first delivery unless required to unblock Claude.
- Captured bodies must be size-limited and redactable.

## Risks

- The MITM root private key is highly sensitive and must stay owner-only.
- Some traffic may bypass Node trust injection or use certificate pinning.
- Capturing decrypted payloads increases privacy risk and requires explicit UX warnings and body limits.
- Protocol handling can regress plain CONNECT proxying if not kept behind clear modes and tests.

## Delivery Phases

1. MITM certificate store, merged CA bundle generation, and health checks.
2. Claude launch wiring for merged trust plus explicit readiness surfacing in CLI and GUI.
3. Sidecar HTTP/1.1 MITM interception and bounded capture.
4. Follow-up work for keychain trust, HTTP/2, and richer body handling.
