# Rust Workspace

This workspace hosts the Rust rewrite of the project as a universal command privacy wrapper. The current binary is `ccp`, and `claude` is the first adapter wired into the new launch flow.

The Rust path is not the default installation target yet. The top-level Bash `cac` remains the primary user-facing tool while the Rust workspace continues to harden cross-platform behavior and migration guidance.

## Current Scope

The workspace already covers these core paths:

- Create and persist profiles with `ccp profile create`
- Activate profiles with `ccp profile activate`
- Run diagnostics with `ccp doctor`
- Launch generic commands through `ccp run`
- Install a user-bin `claude` wrapper with `ccp setup`
- Apply the Claude adapter, Node preload hook, sidecar session metadata, and capability checks
- Refuse launch when a configured proxy is unreachable
- Generate per-profile identity and mTLS materials
- Isolate hostname / machine-id / platform UUID at runtime and sync Claude persistent IDs
- Isolate Windows hostname / machine GUID / platform UUID / MAC lookups through command shims and `COMPUTERNAME`
- Verify runtime hardening from inside a wrapped live `node` process during `ccp doctor`
- Pause and resume wrapper behavior with `ccp pause` and `ccp resume`
- Delete profiles and clean their persisted materials with `ccp profile delete`

## Workspace Layout

- `apps/ccp`: CLI entrypoint
- `crates/core`: shared policy, profile, capability, and launch-plan types
- `crates/store`: state-root layout and persistence helpers
- `crates/launcher`: launch-plan assembly and process execution
- `crates/doctor`: human and JSON diagnostics
- `crates/sidecar-proto`: versioned sidecar protocol types
- `crates/sidecar`: in-memory sidecar/session foundations and audit-facing models
- `crates/adapters/claude`: Claude-specific policy and runtime hook bundle
- `crates/runtime-hooks/node`: Node hook packaging
- `crates/platform-*`: per-platform capability providers

## Usage

```bash
cd rust

cargo run -p ccp -- profile create work --adapter claude
cargo run -p ccp -- profile activate work
cargo run -p ccp -- doctor --profile work
cargo run -p ccp -- run -- claude
cargo run -p ccp -- setup
```

By default `ccp` stores state under `./ccp-state`. Set `CCP_STATE_ROOT` to point at a different state directory during testing or development.

`ccp setup` discovers the real `claude`, generates `ccp` and `claude` shims in a user bin directory, and can append that directory to a shell rc file. `ccp uninstall` removes those generated artifacts and the recorded state root.

## Coverage Status

See [docs/coverage-matrix.md](./docs/coverage-matrix.md) for the current Bash-to-Rust coverage matrix, including the remaining partially completed and unimplemented items.

## Verification

```bash
cd rust

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```

The repository also includes a GitHub Actions matrix in [`.github/workflows/rust-ci.yml`](../.github/workflows/rust-ci.yml) to run the same verification on macOS, Linux, and Windows.

For a quick smoke check:

```bash
cd rust
cargo run -p ccp -- --help
```

## Migration Notes

- Treat the Rust workspace as the migration track, not the default install path.
- Keep top-level install guidance pointing at Bash `cac` until the Rust path is explicitly promoted.
- Use the Rust commands above to validate behavior while comparing old and new implementations.
