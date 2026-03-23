# Rust Refactor Design for a Universal Command Privacy Wrapper

**Date:** 2026-03-23

**Status:** Approved in conversation

**Implementation Plan:** [2026-03-23 Rust Refactor Implementation](../plans/2026-03-23-rust-refactor-implementation.md)

**Scope:** Replace the current Bash-heavy `cac` implementation with a new Rust-based system in a dedicated `rust/` subtree. The new system should become a general-purpose command privacy wrapper, with `claude` as the first adapter.

## Summary

The current project is a Bash CLI that wraps `claude`, injects proxy and privacy-related environment variables, writes platform-specific shims, and uses a Node preload script to block telemetry and inject mTLS behavior. That implementation proves the core idea, but it couples product identity to Claude, mixes policy with shell scripting, and is difficult to harden across macOS, Linux, and Windows.

The Rust rewrite should not be a line-by-line port. It should redefine the project as a layered system:

- A general command privacy wrapper
- A Rust core for configuration, launch planning, identity isolation, network policy, certificate management, and diagnostics
- A sidecar control plane for cross-platform network and policy enforcement
- Target-specific adapters, with `claude` as the first and most complete one
- Runtime hooks for cases where process-internal interception is required, such as Node-based CLIs

## Goals

- Preserve the core privacy and isolation capabilities of the current project
- Support macOS, Linux, and Windows from the first Rust version
- Evolve from a Claude-only tool into a general command privacy wrapper
- Keep `claude` as the first high-priority adapter with deep support
- Improve security posture over the current Bash implementation
- Separate cross-platform core behavior from target-specific behavior
- Make the system testable, inspectable, and maintainable

## Non-Goals

- Perfectly identical internal implementation across all three platforms
- Full replacement of runtime-specific hooks with pure Rust from day one
- Backward compatibility with every existing command and flag in the Bash tool
- Migration of the old Bash implementation into the new tree
- Support for every CLI in the first release

## Current System Constraints

The current codebase has three important characteristics:

- The outer control plane is mostly Bash
- Privacy policy is encoded as imperative script logic instead of explicit policy objects
- Claude-specific assumptions are mixed into the product core

Key current components:

- Shell command dispatch and profile management in `src/main.sh`, `src/cmd_env.sh`, and `src/utils.sh`
- Wrapper and platform shims in `src/templates.sh`
- Node preload generation and telemetry interception in `src/dns_block.sh`
- Certificate generation in `src/mtls.sh`

This version is useful as a behavior reference, but not as the long-term architecture baseline.

## Product Direction

The new product direction is:

- The tool is a universal command privacy wrapper
- `claude` is the first adapter, not the whole product
- Core privacy guarantees must be represented as capabilities and policies
- Platform-specific behavior must be isolated behind stable interfaces

This allows the project to support new target commands later without reworking the core.

## High-Level Architecture

The new system should use a mixed architecture:

- Wrapper and launcher logic for process startup and isolation
- A sidecar for cross-platform network control and policy coordination
- Target adapters for program-specific defaults and enforcement
- Runtime hooks where process-internal interception is required

This is preferred over a pure wrapper model because the project must become general-purpose and cross-platform. It is also preferred over a pure sidecar model because some targets, such as `claude`, benefit from deep runtime-specific interception.

### Layer Responsibilities

**CLI layer**

- Human-friendly command interface
- Profile and policy commands
- Status and diagnostics output
- Non-interactive automation support

**Core layer**

- Profile model
- Identity materials
- Policy definitions
- Launch planning
- Audit event schema
- Adapter interfaces

**Launcher layer**

- Build the final launch plan
- Create isolated process context
- Inject environment and runtime hooks
- Connect target process to sidecar session

**Sidecar layer**

- Cross-platform control plane
- DNS and host blocking policy
- TLS and mTLS material distribution
- Audit event collection
- Session-level runtime coordination

**Adapter layer**

- Target-specific defaults
- Target-specific blocked domains
- Target-specific environment policies
- Target-specific runtime hook requirements

**Runtime hook layer**

- Process-internal interception for supported runtimes
- Node preload hook for `claude`
- Future extension point for other runtimes if needed

**Platform layer**

- macOS, Linux, and Windows implementations
- Host identity isolation mechanisms
- Platform capability detection
- Platform-specific diagnostics

## Repository Layout

The repository should keep the existing Bash version at the top level and introduce a dedicated Rust workspace under `rust/`.

```text
rust/
├─ Cargo.toml
├─ apps/
│  └─ ccp/
├─ crates/
│  ├─ core/
│  ├─ launcher/
│  ├─ sidecar/
│  ├─ sidecar-proto/
│  ├─ store/
│  ├─ doctor/
│  ├─ installer/
│  ├─ platform-macos/
│  ├─ platform-linux/
│  ├─ platform-windows/
│  ├─ adapters/
│  │  └─ claude/
│  └─ runtime-hooks/
│     └─ node/
├─ hooks/
│  └─ node/
├─ tests/
│  ├─ integration/
│  ├─ fixtures/
│  └─ e2e/
└─ docs/
```

The binary name `ccp` is a placeholder in this design. Naming can be finalized later without changing the module structure.

## Crate Design

### `apps/ccp`

- Command-line entry point
- Subcommand routing
- TTY-aware prompts
- Machine-readable output mode
- Calls into `core`, `store`, `launcher`, and `doctor`

### `crates/core`

Primary domain types:

- `Profile`
- `IdentityMaterial`
- `LaunchPlan`
- `PrivacyPolicy`
- `NetworkPolicy`
- `TelemetryPolicy`
- `IdentityPolicy`
- `TlsPolicy`
- `RuntimeHookPolicy`
- `AuditPolicy`
- `TargetAdapter`
- `CapabilitySet`

This crate should not contain platform-specific filesystem paths or target-specific shell details.

### `crates/store`

- On-disk state layout
- Safe file creation and permissions
- Profile persistence
- Certificate and key storage
- Migration support between schema versions

This crate should treat secrets and proxy credentials as sensitive data and enforce strict permissions by default.

### `crates/launcher`

- Build a `LaunchPlan` from profile, adapter, target, and platform
- Prepare the runtime environment
- Start or connect to sidecar
- Attach runtime hooks
- Launch the real target command

This crate is the execution bridge between declarative policy and actual process startup.

### `crates/sidecar`

- Expose the local control plane
- Manage per-launch sessions
- Apply DNS and host blocking rules
- Provide TLS and mTLS configuration to launch sessions
- Receive audit and diagnostic events

The sidecar should be optional at startup only in cases where a target and policy do not need it. The default assumption for privacy-sensitive targets should be that sidecar is available.

### `crates/sidecar-proto`

- Shared request and response models between CLI, launcher, and sidecar
- Versioned protocol definitions
- Serialization contracts

This keeps sidecar RPC types out of business logic crates.

### `crates/doctor`

- Detect local proxy conflicts
- Detect missing runtime hooks
- Detect capability mismatches
- Validate secret and file permissions
- Produce human-readable and machine-readable diagnostic output

This replaces the current Bash `check` logic with a structured diagnostic system.

### `crates/installer`

- Install launchers or wrappers
- Manage platform-specific integration
- Set up default directories
- Validate PATH integration where needed

This crate should avoid opaque remote-execution behavior. Installation must be explicit and auditable.

### `crates/platform-macos`

- macOS-specific platform integration
- Hostname and hardware identity strategies available on macOS
- Native diagnostic helpers

### `crates/platform-linux`

- Linux-specific platform integration
- Machine identity isolation strategies
- Linux diagnostic helpers

### `crates/platform-windows`

- Windows-specific process, environment, and identity isolation implementation
- Capability detection for Windows-specific privacy features
- Windows diagnostic helpers

### `crates/adapters/claude`

- Claude-specific blocked hosts
- Claude-specific environment policies
- Claude-specific runtime requirements
- Claude-specific compatibility checks

This crate should be the only place where Claude behavior is encoded as product knowledge.

### `crates/runtime-hooks/node`

- Generate or embed the Node preload artifact
- Implement runtime interception for `dns`, `net`, `tls`, and `fetch`
- Restrict behavior to the current sidecar or launch session

This crate exists because some target behavior is best controlled from inside the Node process itself.

## Core Capability Model

The Rust version must define privacy behavior in terms of capabilities instead of shell fragments. Each adapter should declare:

- Required capabilities
- Preferred capabilities
- Unsupported environments
- Degradation policy

Example capability categories:

- Proxy injection
- DNS policy enforcement
- Blocked host enforcement
- Stable identity injection
- Locale and timezone isolation
- Hostname isolation
- Machine identifier isolation
- Runtime preload support
- mTLS support
- Audit emission

The launcher should compare:

- Capabilities required by the adapter
- Capabilities provided by the current platform
- Capabilities enabled by the current profile

If the minimum capability set is not satisfied, the launch must fail explicitly. Silent downgrade is not acceptable for privacy-critical behavior.

## Policy Model

The rewrite should convert current hard-coded behavior into structured policies.

### `NetworkPolicy`

- Proxy selection
- Direct egress allowance or denial
- DNS strategy
- Allowed and blocked destinations
- Sidecar requirement

### `TelemetryPolicy`

- Blocked telemetry endpoints
- Environment variable overrides
- Runtime hook requirements
- Error-reporting suppression rules

### `IdentityPolicy`

- Stable identity generation
- Hostname behavior
- Machine identifier behavior
- Locale and timezone behavior
- Per-target identity scope

### `TlsPolicy`

- Custom CA handling
- mTLS client certificate handling
- Target-scoped trust injection
- Per-session certificate selection

### `RuntimeHookPolicy`

- Runtime type
- Hook payload
- Injection mode
- Hook failure behavior

### `AuditPolicy`

- What to log
- What to redact
- Local retention behavior
- Diagnostic bundle generation

## Launch Flow

The default launch flow should be:

1. User invokes `ccp run --profile <name> -- <target command>`
2. CLI loads profile and identifies the target adapter
3. Adapter contributes target-specific policy defaults
4. Core merges profile policy, adapter policy, and platform capabilities into a `LaunchPlan`
5. Launcher creates or reuses a sidecar session if required
6. Launcher requests identity materials and TLS materials from `store`
7. Launcher attaches runtime hooks if required
8. Launcher starts the real target process with the planned environment and session context
9. Sidecar and hooks emit audit events
10. CLI or doctor can inspect the resulting session state

This flow centers all decision-making around the `LaunchPlan`. The launcher executes a plan instead of inventing logic during process start.

## Cross-Platform Strategy

The first Rust version must present a largely consistent user-facing privacy model on:

- macOS
- Linux
- Windows

The internal implementation may differ by platform, but the user-facing guarantees should remain aligned:

- Per-profile isolation
- Independent identity material
- Proxy support with preflight validation
- DNS or host blocking policy
- Telemetry policy injection
- TLS and mTLS support
- Diagnostics and audit visibility

Where a platform cannot provide a feature using the same mechanism, the platform crate should provide:

- The strongest viable implementation
- A precise capability report
- A deterministic failure when the adapter demands more than the platform can provide

The system must never claim full protection while silently omitting a required protection layer.

## Claude Adapter Strategy

`claude` remains the most complete adapter in the first Rust release.

Responsibilities of the Claude adapter:

- Define Claude-specific telemetry endpoints and environment toggles
- Require the Node runtime hook
- Define identity material needed for profile isolation
- Coordinate with sidecar and TLS policy for proxy use
- Perform Claude-specific preflight checks

The current Bash implementation already shows that Claude-specific interception spans:

- Environment variables
- Process wrapper behavior
- Runtime-level Node interception
- Stable identity material updates

Those behaviors should move into the Claude adapter and Node runtime hook instead of staying in the global product core.

## Security Design Principles

The Rust rewrite must improve the security posture of the current project.

Required principles:

- No implicit trust in remotely fetched executable content
- No plaintext secret storage with weak permissions
- No silent network probes to third-party services without explicit diagnostic purpose
- No hidden downgrade when a privacy feature cannot be applied
- No target-specific hacks in shared core crates
- No all-purpose trust expansion unless explicitly scoped to a launch session

Specific improvements over the current project should include:

- Strong default permissions for state, keys, and proxy credentials
- Explicit separation between configuration data and secret material
- Structured audit logging with redaction
- Deterministic launch failure on capability mismatch
- Safer installation flow than the current remote shell script path

## Data and State Layout

The Rust tool should use its own storage root and versioned layout. The exact product root name can be finalized later, but the structure should follow this pattern:

```text
<state-root>/
├─ profiles/
├─ identities/
├─ certs/
├─ hooks/
├─ sessions/
├─ sidecar/
├─ audit/
└─ config/
```

High-level rules:

- Profiles store references, not raw secret duplication
- Secrets are separated from ordinary config
- Sessions are ephemeral where possible
- Hooks are versioned artifacts
- Audit logs are redactable and bounded

## Migration Plan

### Phase 1: Foundation

- Create the `rust/` Cargo workspace
- Implement `core`, `store`, `launcher`, and `apps/ccp`
- Support profile creation, selection, and inspection
- Support launching arbitrary commands with minimal policy application

### Phase 2: Claude Adapter

- Implement `adapters/claude`
- Implement `runtime-hooks/node`
- Support proxy injection, telemetry policy, and runtime interception for Claude
- Reproduce the most important privacy behavior from the current project

### Phase 3: Sidecar and Cross-Platform Hardening

- Implement `sidecar` and `sidecar-proto`
- Add `platform-macos`, `platform-linux`, and `platform-windows`
- Add capability reporting and launch refusal on missing guarantees
- Add structured diagnostics in `doctor`

### Phase 4: Parity and Transition

- Compare old Bash behavior and new Rust behavior using an explicit capability matrix
- Decide which legacy features are intentionally dropped
- Prepare deprecation path for the top-level Bash implementation
- Move documentation and installation guidance toward the Rust version

## Testing Strategy

The rewrite should treat testing as a first-class design requirement.

Test layers:

- Unit tests for policy merge logic, capability evaluation, and store behavior
- Integration tests for launch planning and sidecar session coordination
- Fixture-based tests for adapter and hook generation
- End-to-end tests for launching sample commands on each platform
- Security-focused tests for secret permissions, blocked host behavior, and capability failure paths

Required coverage areas:

- Launch refusal on missing required capabilities
- Correct policy inheritance and override behavior
- Correct redaction in audit output
- Correct sidecar session lifecycle
- Correct Node hook generation for Claude
- Stable behavior across macOS, Linux, and Windows CI jobs where feasible

## Risks

- Windows identity isolation may need different primitives than Unix-like platforms
- Runtime hook behavior can drift as target runtimes evolve
- Sidecar complexity can grow too early if not scoped tightly
- The project can regress into target-specific sprawl if adapter boundaries are not enforced

## Open Questions

- Final product name and binary name
- Final default storage root name
- Whether sidecar is always-on for privacy-sensitive adapters or only started on demand
- Which parts of current Claude identity rewriting should remain adapter-local versus move into shared identity policy helpers

## Decision Summary

The Rust rewrite should proceed as a new product architecture inside `rust/`, not as a shell port. The product becomes a universal command privacy wrapper with:

- A Rust core
- A sidecar-based cross-platform control plane
- Adapter-specific behavior for targets
- Runtime hooks where needed
- Claude as the first adapter

This design preserves the current project’s strongest idea while giving it a safer, more extensible foundation.
