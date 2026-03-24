# Rust Coverage Matrix

Updated: 2026-03-23

This document tracks how the Rust rewrite (`ccp`) compares to the legacy Bash `cac` implementation.

## Completed

- Profile creation, listing, and display via `ccp profile create|list|show`
- Active profile workflow via `ccp profile activate` plus `ccp run -- <command...>`
- Runtime pause/resume workflow via `ccp pause` and `ccp resume`
- Profile deletion and state cleanup via `ccp profile delete`
- Rust-native install and uninstall flow via `ccp setup` and `ccp uninstall`
- Generated `ccp` shim and global `claude` wrapper installation through a user bin directory
- Generic command launch via `ccp run --profile <name> -- <command...>`
- Claude adapter integration with Node preload hook and sidecar session metadata
- Proxy persistence and runtime injection through `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, and `NO_PROXY`
- Pre-launch TCP refusal when the configured proxy is unreachable
- DNS / `net` / `tls` / `fetch` telemetry blocking in the Claude preload hook
- Multi-layer telemetry environment hardening and third-party Anthropic endpoint unsetting
- Per-profile identity material generation:
  `uuid`, `stable_id`, `user_id`, `machine_id`, `hostname`, `mac_address`, `tz`, `lang`
- Best-effort timezone and locale inference from proxy exit metadata during profile creation
- Runtime identity isolation through `HOSTNAME`, `TZ`, `LANG`, and shimmed `hostname`, `cat /etc/machine-id`, `ioreg`, and `ifconfig`
- Windows runtime identity isolation through injected `COMPUTERNAME` plus shimmed `hostname`, `getmac`, `wmic`, `reg query`, and `powershell`
- Claude persistent identity sync on launch:
  `~/.claude/statsig/statsig.stable_id.*` and `~/.claude.json.userID`
- Per-profile mTLS material generation:
  self-signed CA, client cert, client key, CA injection, and preload-side TLS client auth wiring
- `HOSTALIASES` fallback file generation and runtime export for blocked hosts
- Doctor checks for:
  profile existence, state layout, adapter support, platform capability support, identity materials, mTLS materials, proxy reachability, proxy exit IP, local proxy conflict heuristics, runtime self-audit, runtime live self-audit, secret permission sanity
- Cross-platform capability-provider scaffolding for macOS, Linux, and Windows
- Cross-platform CI matrix for `cargo fmt`, `cargo clippy`, and `cargo test` on macOS, Linux, and Windows

## Not Implemented
- No known feature gaps remain versus the legacy Claude-focused scope or the currently targeted Rust migration scope.

## Future Extensions
- Adapter ecosystem:
  The wrapper architecture is generic, but only the Claude adapter is currently productionized. Additional adapters are product expansion work, not legacy-parity work.

## Intentional Design Differences

- Rust uses explicit `ccp run --profile ... -- <command>` instead of replacing `claude` globally by default.
- Rust treats Claude as the first adapter in a generic wrapper architecture rather than baking all behavior into a single shell wrapper.
- Rust keeps diagnostics as structured checks (`doctor`) instead of reproducing the exact Bash output format of `cac check`.
