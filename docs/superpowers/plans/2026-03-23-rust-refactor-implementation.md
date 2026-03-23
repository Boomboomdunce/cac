# Rust Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a new Rust-based universal command privacy wrapper in `rust/`, with `claude` as the first adapter and cross-platform foundations for macOS, Linux, and Windows.

**Architecture:** The implementation creates a Rust workspace with clear crate boundaries for core policy modeling, secure state storage, launch orchestration, diagnostics, sidecar coordination, platform capabilities, and target-specific adapters. The first vertical slice reaches a working `ccp run --profile ... -- claude` flow backed by structured policy and a Node runtime hook, while preserving the existing Bash implementation as a reference.

**Tech Stack:** Rust stable, Cargo workspace, `clap`, `serde`, `serde_json`, `tokio`, `tracing`, `tempfile`, `camino`, `thiserror`, `assert_cmd`, `predicates`, `insta`, Node.js for the Claude preload hook.

---

## Planned File Structure

**Create:**

- `rust/Cargo.toml`
- `rust/rust-toolchain.toml`
- `rust/apps/ccp/Cargo.toml`
- `rust/apps/ccp/src/main.rs`
- `rust/crates/core/Cargo.toml`
- `rust/crates/core/src/lib.rs`
- `rust/crates/core/src/profile.rs`
- `rust/crates/core/src/policy.rs`
- `rust/crates/core/src/capability.rs`
- `rust/crates/core/src/launch_plan.rs`
- `rust/crates/core/src/adapter.rs`
- `rust/crates/store/Cargo.toml`
- `rust/crates/store/src/lib.rs`
- `rust/crates/store/src/layout.rs`
- `rust/crates/store/src/profile_store.rs`
- `rust/crates/store/src/secret_store.rs`
- `rust/crates/store/src/error.rs`
- `rust/crates/launcher/Cargo.toml`
- `rust/crates/launcher/src/lib.rs`
- `rust/crates/launcher/src/builder.rs`
- `rust/crates/launcher/src/exec.rs`
- `rust/crates/launcher/src/env_plan.rs`
- `rust/crates/launcher/src/session.rs`
- `rust/crates/doctor/Cargo.toml`
- `rust/crates/doctor/src/lib.rs`
- `rust/crates/doctor/src/report.rs`
- `rust/crates/doctor/src/checks.rs`
- `rust/crates/sidecar-proto/Cargo.toml`
- `rust/crates/sidecar-proto/src/lib.rs`
- `rust/crates/sidecar/Cargo.toml`
- `rust/crates/sidecar/src/lib.rs`
- `rust/crates/sidecar/src/server.rs`
- `rust/crates/sidecar/src/session.rs`
- `rust/crates/installer/Cargo.toml`
- `rust/crates/installer/src/lib.rs`
- `rust/crates/platform-macos/Cargo.toml`
- `rust/crates/platform-macos/src/lib.rs`
- `rust/crates/platform-linux/Cargo.toml`
- `rust/crates/platform-linux/src/lib.rs`
- `rust/crates/platform-windows/Cargo.toml`
- `rust/crates/platform-windows/src/lib.rs`
- `rust/crates/adapters/claude/Cargo.toml`
- `rust/crates/adapters/claude/src/lib.rs`
- `rust/crates/adapters/claude/src/policy.rs`
- `rust/crates/adapters/claude/src/env.rs`
- `rust/crates/runtime-hooks/node/Cargo.toml`
- `rust/crates/runtime-hooks/node/src/lib.rs`
- `rust/crates/runtime-hooks/node/src/bundle.rs`
- `rust/hooks/node/claude-preload.js`
- `rust/tests/integration/cli_profile.rs`
- `rust/tests/integration/launch_plan.rs`
- `rust/tests/integration/claude_adapter.rs`
- `rust/tests/e2e/run_claude_smoke.rs`
- `rust/tests/fixtures/fake_claude.js`
- `rust/README.md`

**Modify later in the plan:**

- `README.md`
- `.gitignore`

## Task 1: Scaffold the Rust Workspace

**Files:**

- Create: `rust/Cargo.toml`
- Create: `rust/rust-toolchain.toml`
- Create: `rust/apps/ccp/Cargo.toml`
- Create: `rust/apps/ccp/src/main.rs`
- Create: `rust/README.md`
- Modify: `.gitignore`

- [ ] **Step 1: Write the failing smoke test for the new CLI binary**

```rust
use assert_cmd::Command;

#[test]
fn ccp_help_exits_successfully() {
    Command::cargo_bin("ccp").unwrap().arg("--help").assert().success();
}
```

Place this in `rust/tests/integration/cli_profile.rs`.

- [ ] **Step 2: Run the test to verify the workspace does not exist yet**

Run: `cd rust && cargo test --test cli_profile`

Expected: FAIL with an error similar to `could not find Cargo.toml`.

- [ ] **Step 3: Create the workspace manifest and binary crate skeleton**

Use this minimal root manifest:

```toml
[workspace]
members = [
  "apps/ccp",
]
resolver = "2"
```

Use this binary skeleton:

```rust
fn main() {
    println!("ccp bootstrap");
}
```

- [ ] **Step 4: Replace the bootstrap binary with a real Clap-based help command**

Use this starter in `rust/apps/ccp/src/main.rs`:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ccp")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Profile,
    Run,
    Doctor,
}

fn main() {
    let _ = Cli::parse();
}
```

- [ ] **Step 5: Re-run the smoke test**

Run: `cd rust && cargo test --test cli_profile`

Expected: PASS for `ccp_help_exits_successfully`.

- [ ] **Step 6: Commit the workspace scaffold**

```bash
git add .gitignore rust/Cargo.toml rust/rust-toolchain.toml rust/apps/ccp rust/README.md rust/tests/integration/cli_profile.rs
git commit -m "feat: scaffold rust workspace and ccp cli"
```

## Task 2: Model Core Policies and Capabilities

**Files:**

- Create: `rust/crates/core/Cargo.toml`
- Create: `rust/crates/core/src/lib.rs`
- Create: `rust/crates/core/src/profile.rs`
- Create: `rust/crates/core/src/policy.rs`
- Create: `rust/crates/core/src/capability.rs`
- Create: `rust/crates/core/src/launch_plan.rs`
- Create: `rust/crates/core/src/adapter.rs`
- Modify: `rust/Cargo.toml`

- [ ] **Step 1: Write failing unit tests for capability evaluation and policy merge**

Add tests like:

```rust
#[test]
fn required_capability_mismatch_is_rejected() {
    let required = CapabilitySet::from(["node_preload", "proxy"]);
    let provided = CapabilitySet::from(["proxy"]);
    assert!(required.is_subset_of(&provided) == false);
}

#[test]
fn adapter_policy_overrides_profile_defaults() {
    let merged = PrivacyPolicy::default()
        .with_blocked_host("example.com")
        .merge(PrivacyPolicy::default().with_blocked_host("statsig.anthropic.com"));
    assert!(merged.blocked_hosts.contains("statsig.anthropic.com"));
}
```

- [ ] **Step 2: Run core tests before the crate exists**

Run: `cd rust && cargo test -p core`

Expected: FAIL with `package ID specification 'core' did not match any packages`.

- [ ] **Step 3: Implement minimal core types**

Start with:

```rust
pub struct Profile {
    pub name: String,
    pub adapter: String,
}

pub struct LaunchPlan {
    pub target: String,
    pub args: Vec<String>,
}
```

Then grow them only enough to satisfy current tests.

- [ ] **Step 4: Implement `CapabilitySet`, `PrivacyPolicy`, and `TargetAdapter` interfaces**

Include:

- deterministic merge order
- required vs preferred capabilities
- explicit adapter identity

- [ ] **Step 5: Re-run the core tests**

Run: `cd rust && cargo test -p core`

Expected: PASS for the initial capability and policy tests.

- [ ] **Step 6: Commit the core modeling layer**

```bash
git add rust/Cargo.toml rust/crates/core
git commit -m "feat: add core policy and capability models"
```

## Task 3: Add Secure State Storage and Profile Persistence

**Files:**

- Create: `rust/crates/store/Cargo.toml`
- Create: `rust/crates/store/src/lib.rs`
- Create: `rust/crates/store/src/layout.rs`
- Create: `rust/crates/store/src/profile_store.rs`
- Create: `rust/crates/store/src/secret_store.rs`
- Create: `rust/crates/store/src/error.rs`
- Modify: `rust/Cargo.toml`
- Test: `rust/tests/integration/cli_profile.rs`

- [ ] **Step 1: Extend integration tests to cover profile creation and reading**

Add:

```rust
#[test]
fn profile_create_writes_profile_to_state_root() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    assert!(temp.path().join("profiles/work.json").exists());
}
```

- [ ] **Step 2: Run the integration test to verify profile commands do not exist**

Run: `cd rust && cargo test --test cli_profile profile_create_writes_profile_to_state_root -- --exact`

Expected: FAIL with a clap subcommand or implementation error.

- [ ] **Step 3: Implement the store crate with an explicit directory layout**

Start with this shape:

```text
<state-root>/
├─ profiles/
├─ identities/
├─ certs/
├─ hooks/
├─ sessions/
├─ audit/
└─ config/
```

- [ ] **Step 4: Enforce secure file creation**

Implement permission handling so secrets and credential-bearing files are created with owner-only permissions where supported. Add platform-specific no-op or equivalent handling for Windows if exact POSIX permissions are unavailable.

- [ ] **Step 5: Implement `ccp profile create`, `ccp profile show`, and `ccp profile list`**

Wire the CLI to `store` through typed commands instead of direct filesystem logic.

- [ ] **Step 6: Re-run profile integration tests**

Run: `cd rust && cargo test --test cli_profile`

Expected: PASS for help and profile storage tests.

- [ ] **Step 7: Commit the store and profile workflow**

```bash
git add rust/Cargo.toml rust/crates/store rust/apps/ccp/src/main.rs rust/tests/integration/cli_profile.rs
git commit -m "feat: add secure state store and profile commands"
```

## Task 4: Implement Launch Planning and Generic Command Execution

**Files:**

- Create: `rust/crates/launcher/Cargo.toml`
- Create: `rust/crates/launcher/src/lib.rs`
- Create: `rust/crates/launcher/src/builder.rs`
- Create: `rust/crates/launcher/src/exec.rs`
- Create: `rust/crates/launcher/src/env_plan.rs`
- Create: `rust/crates/launcher/src/session.rs`
- Modify: `rust/apps/ccp/src/main.rs`
- Test: `rust/tests/integration/launch_plan.rs`

- [ ] **Step 1: Write a failing integration test for a generic command run**

Use a trivial process first:

```rust
#[test]
fn run_executes_generic_command_under_profile() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["run", "--profile", "work", "--", "env"])
        .assert()
        .success();
}
```

- [ ] **Step 2: Run the launch test to verify `run` is unimplemented**

Run: `cd rust && cargo test --test launch_plan`

Expected: FAIL because `run` does not create a launch plan or spawn the target process.

- [ ] **Step 3: Implement `LaunchPlanBuilder`**

The builder should assemble:

- target command and args
- selected adapter
- merged policy
- environment variables
- runtime hooks
- sidecar requirement

- [ ] **Step 4: Implement generic process execution**

Start with `std::process::Command` and preserve stdin/stdout/stderr behavior. Do not add sidecar coordination yet beyond placeholder session data.

- [ ] **Step 5: Implement `ccp run --profile <name> -- <command>`**

Wire the CLI command into `store`, `core`, and `launcher`.

- [ ] **Step 6: Re-run launch planning tests**

Run: `cd rust && cargo test --test launch_plan`

Expected: PASS for generic command execution.

- [ ] **Step 7: Commit the generic launcher**

```bash
git add rust/Cargo.toml rust/crates/launcher rust/apps/ccp/src/main.rs rust/tests/integration/launch_plan.rs
git commit -m "feat: add generic launch planning and command execution"
```

## Task 5: Add Diagnostics with `doctor`

**Files:**

- Create: `rust/crates/doctor/Cargo.toml`
- Create: `rust/crates/doctor/src/lib.rs`
- Create: `rust/crates/doctor/src/report.rs`
- Create: `rust/crates/doctor/src/checks.rs`
- Modify: `rust/apps/ccp/src/main.rs`
- Test: `rust/tests/integration/cli_profile.rs`

- [ ] **Step 1: Write failing tests for `ccp doctor --profile work`**

Check for:

- missing profile
- existing profile
- machine-readable JSON output

- [ ] **Step 2: Run the doctor tests before implementation**

Run: `cd rust && cargo test --test cli_profile doctor`

Expected: FAIL due to missing subcommands or missing diagnostics output.

- [ ] **Step 3: Implement typed diagnostic reports**

Use a result type like:

```rust
pub struct DoctorReport {
    pub ok: bool,
    pub checks: Vec<CheckResult>,
}
```

- [ ] **Step 4: Implement the first checks**

Start with:

- profile existence
- state root layout
- adapter resolution
- secret permission sanity

- [ ] **Step 5: Add `--json` output**

This should serialize the same report structure used for human output.

- [ ] **Step 6: Re-run the doctor tests**

Run: `cd rust && cargo test --test cli_profile doctor`

Expected: PASS for the initial doctor coverage.

- [ ] **Step 7: Commit diagnostics support**

```bash
git add rust/Cargo.toml rust/crates/doctor rust/apps/ccp/src/main.rs rust/tests/integration/cli_profile.rs
git commit -m "feat: add doctor diagnostics command"
```

## Task 6: Introduce Sidecar Protocol and Minimal Sidecar Session Support

**Files:**

- Create: `rust/crates/sidecar-proto/Cargo.toml`
- Create: `rust/crates/sidecar-proto/src/lib.rs`
- Create: `rust/crates/sidecar/Cargo.toml`
- Create: `rust/crates/sidecar/src/lib.rs`
- Create: `rust/crates/sidecar/src/server.rs`
- Create: `rust/crates/sidecar/src/session.rs`
- Modify: `rust/crates/launcher/src/session.rs`
- Test: `rust/tests/integration/launch_plan.rs`

- [ ] **Step 1: Write a failing test that asserts a privacy-sensitive adapter marks sidecar as required**

Example:

```rust
#[test]
fn claude_launch_plan_requires_sidecar() {
    let plan = make_claude_plan();
    assert!(plan.sidecar.required);
}
```

- [ ] **Step 2: Run the test to confirm sidecar metadata is missing**

Run: `cd rust && cargo test --test launch_plan claude_launch_plan_requires_sidecar -- --exact`

Expected: FAIL because `LaunchPlan` has no sidecar session data yet.

- [ ] **Step 3: Implement protocol types and a local in-process sidecar stub**

Do not build full DNS/TLS enforcement yet. First establish:

- session creation
- session metadata
- versioned request and response types

- [ ] **Step 4: Extend `LaunchPlan` and `launcher` to carry sidecar requirements**

The launcher may use an in-process or no-op server in this step as long as the session contract is real.

- [ ] **Step 5: Re-run the sidecar session tests**

Run: `cd rust && cargo test --test launch_plan`

Expected: PASS for the new sidecar requirement assertions.

- [ ] **Step 6: Commit sidecar foundations**

```bash
git add rust/Cargo.toml rust/crates/sidecar-proto rust/crates/sidecar rust/crates/launcher/src/session.rs rust/tests/integration/launch_plan.rs
git commit -m "feat: add sidecar protocol and session foundations"
```

## Task 7: Add the Claude Adapter and Node Runtime Hook

**Files:**

- Create: `rust/crates/adapters/claude/Cargo.toml`
- Create: `rust/crates/adapters/claude/src/lib.rs`
- Create: `rust/crates/adapters/claude/src/policy.rs`
- Create: `rust/crates/adapters/claude/src/env.rs`
- Create: `rust/crates/runtime-hooks/node/Cargo.toml`
- Create: `rust/crates/runtime-hooks/node/src/lib.rs`
- Create: `rust/crates/runtime-hooks/node/src/bundle.rs`
- Create: `rust/hooks/node/claude-preload.js`
- Create: `rust/tests/integration/claude_adapter.rs`
- Create: `rust/tests/fixtures/fake_claude.js`

- [ ] **Step 1: Write failing tests for Claude policy generation**

Cover:

- blocked telemetry hosts are present
- Node preload is required
- adapter marks sidecar as required
- environment policy includes Claude-specific toggles

- [ ] **Step 2: Run the Claude adapter tests before the adapter exists**

Run: `cd rust && cargo test --test claude_adapter`

Expected: FAIL because the adapter crate and hook assets do not exist.

- [ ] **Step 3: Implement the Claude adapter as a thin policy provider**

Expose functions or a trait impl that returns:

- blocked hosts
- environment variable overrides
- required capabilities
- runtime hook bundle reference

- [ ] **Step 4: Port the Node preload logic into `rust/hooks/node/claude-preload.js`**

Keep only the logic needed for:

- blocked domain interception
- scoped TLS/mTLS hook points
- controlled fetch interception

Do not copy Bash concerns into the hook.

- [ ] **Step 5: Implement runtime hook bundling**

The Rust hook crate should make the preload artifact available to the launcher, either by embedding it with `include_str!` or by copying it into a session-specific hook directory.

- [ ] **Step 6: Re-run adapter tests**

Run: `cd rust && cargo test --test claude_adapter`

Expected: PASS for policy, hook, and capability assertions.

- [ ] **Step 7: Commit the Claude adapter vertical slice**

```bash
git add rust/Cargo.toml rust/crates/adapters/claude rust/crates/runtime-hooks/node rust/hooks/node/claude-preload.js rust/tests/integration/claude_adapter.rs rust/tests/fixtures/fake_claude.js
git commit -m "feat: add claude adapter and node runtime hook"
```

## Task 8: Wire `ccp run` Through the Claude Adapter

**Files:**

- Modify: `rust/apps/ccp/src/main.rs`
- Modify: `rust/crates/launcher/src/builder.rs`
- Modify: `rust/crates/launcher/src/env_plan.rs`
- Modify: `rust/crates/launcher/src/exec.rs`
- Modify: `rust/crates/store/src/profile_store.rs`
- Test: `rust/tests/e2e/run_claude_smoke.rs`

- [ ] **Step 1: Write a failing end-to-end test using the fake Claude fixture**

Structure it like:

```rust
#[test]
fn run_claude_injects_expected_environment() {
    let temp = tempfile::tempdir().unwrap();
    let fake = fixture_fake_claude_path();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["run", "--profile", "work", "--", fake.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicates::str::contains("CCP_SESSION_ID"));
}
```

- [ ] **Step 2: Run the end-to-end test to confirm Claude-specific execution is not wired**

Run: `cd rust && cargo test --test run_claude_smoke`

Expected: FAIL because the launcher does not yet adapt the target command as Claude.

- [ ] **Step 3: Add adapter resolution in the CLI and launcher**

Profiles must resolve adapters by name, then the launch builder must merge adapter policy into the final launch plan.

- [ ] **Step 4: Inject the Node preload and adapter environment into Claude launches**

At minimum ensure:

- session id is exported
- runtime hook location is exported
- Claude-specific environment overrides are present

- [ ] **Step 5: Re-run the end-to-end Claude smoke test**

Run: `cd rust && cargo test --test run_claude_smoke`

Expected: PASS with the fixture observing expected environment.

- [ ] **Step 6: Commit the Claude launch path**

```bash
git add rust/apps/ccp/src/main.rs rust/crates/launcher rust/crates/store/src/profile_store.rs rust/tests/e2e/run_claude_smoke.rs
git commit -m "feat: wire claude adapter into ccp run"
```

## Task 9: Add Platform Capability Crates and Refuse Unsupported Launches

**Files:**

- Create: `rust/crates/platform-macos/Cargo.toml`
- Create: `rust/crates/platform-macos/src/lib.rs`
- Create: `rust/crates/platform-linux/Cargo.toml`
- Create: `rust/crates/platform-linux/src/lib.rs`
- Create: `rust/crates/platform-windows/Cargo.toml`
- Create: `rust/crates/platform-windows/src/lib.rs`
- Modify: `rust/crates/core/src/capability.rs`
- Modify: `rust/crates/launcher/src/builder.rs`
- Modify: `rust/crates/doctor/src/checks.rs`
- Test: `rust/tests/integration/launch_plan.rs`

- [ ] **Step 1: Write failing tests for capability mismatch refusal**

Add a test asserting:

```rust
#[test]
fn launcher_refuses_when_required_capabilities_are_missing() {
    let err = build_plan_for_unsupported_platform().unwrap_err();
    assert!(err.to_string().contains("required capability"));
}
```

- [ ] **Step 2: Run the capability refusal tests before platform crates exist**

Run: `cd rust && cargo test --test launch_plan launcher_refuses_when_required_capabilities_are_missing -- --exact`

Expected: FAIL because the launcher has no platform capability provider.

- [ ] **Step 3: Implement platform capability providers**

Each platform crate should expose:

- platform identity
- provided capability set
- platform-specific doctor checks

- [ ] **Step 4: Update the launcher to compare adapter requirements against platform capabilities**

The launcher must return a typed error instead of silently proceeding.

- [ ] **Step 5: Re-run capability tests**

Run: `cd rust && cargo test --test launch_plan`

Expected: PASS for launch refusal and capability evaluation.

- [ ] **Step 6: Commit platform capability gating**

```bash
git add rust/Cargo.toml rust/crates/platform-macos rust/crates/platform-linux rust/crates/platform-windows rust/crates/core/src/capability.rs rust/crates/launcher/src/builder.rs rust/crates/doctor/src/checks.rs rust/tests/integration/launch_plan.rs
git commit -m "feat: add platform capability providers and launch refusal"
```

## Task 10: Harden Secrets, Audit Events, and Sidecar Metadata

**Files:**

- Modify: `rust/crates/store/src/secret_store.rs`
- Modify: `rust/crates/core/src/launch_plan.rs`
- Modify: `rust/crates/core/src/policy.rs`
- Modify: `rust/crates/sidecar/src/session.rs`
- Modify: `rust/crates/doctor/src/report.rs`
- Test: `rust/tests/integration/cli_profile.rs`
- Test: `rust/tests/integration/launch_plan.rs`

- [ ] **Step 1: Write failing tests for secret redaction and audit serialization**

Cover:

- profile output redacts proxy credentials
- doctor JSON does not leak secrets
- launch plan debug output excludes secret values

- [ ] **Step 2: Run the tests to verify secret handling is still too permissive**

Run: `cd rust && cargo test --test cli_profile --test launch_plan`

Expected: FAIL because redaction and audit behavior are not fully implemented.

- [ ] **Step 3: Implement explicit redaction helpers**

For example:

```rust
pub fn redact_proxy_url(raw: &str) -> String {
    raw.replace(":password@", ":***@")
}
```

Use a real parser instead of raw string replacement in the final implementation.

- [ ] **Step 4: Add audit event models that separate redacted and secret-bearing fields**

Do not reuse the same struct for both internal execution and external reporting.

- [ ] **Step 5: Re-run the hardening tests**

Run: `cd rust && cargo test --test cli_profile --test launch_plan`

Expected: PASS for redaction and audit checks.

- [ ] **Step 6: Commit security hardening**

```bash
git add rust/crates/store/src/secret_store.rs rust/crates/core/src/launch_plan.rs rust/crates/core/src/policy.rs rust/crates/sidecar/src/session.rs rust/crates/doctor/src/report.rs rust/tests/integration/cli_profile.rs rust/tests/integration/launch_plan.rs
git commit -m "feat: harden secrets and audit reporting"
```

## Task 11: Update Documentation and Transition Guidance

**Files:**

- Modify: `README.md`
- Modify: `rust/README.md`
- Modify: `docs/superpowers/specs/2026-03-23-rust-refactor-design.md`

- [ ] **Step 1: Write a failing doc checklist in the plan execution notes**

The checklist should require:

- top-level README mentions the Rust rewrite status
- `rust/README.md` explains workspace purpose and how to run tests
- the design spec links to the implementation plan

- [ ] **Step 2: Update docs only after the code path is stable**

Do not change install guidance to default to Rust until the Rust path can create profiles, run generic commands, run the Claude adapter, and report diagnostics.

- [ ] **Step 3: Add concrete Rust usage examples**

Include:

- `cargo run -p ccp -- profile create work --adapter claude`
- `cargo run -p ccp -- doctor --profile work`
- `cargo run -p ccp -- run --profile work -- claude`

- [ ] **Step 4: Run a final Rust test sweep before doc finalization**

Run: `cd rust && cargo test`

Expected: PASS across unit, integration, and e2e tests that are valid on the current platform.

- [ ] **Step 5: Commit documentation updates**

```bash
git add README.md rust/README.md docs/superpowers/specs/2026-03-23-rust-refactor-design.md
git commit -m "docs: add rust workspace usage and transition guidance"
```

## Task 12: Final Verification Gate Before Recommending Migration

**Files:**

- No new files
- Verify: `rust/`
- Verify: `README.md`

- [ ] **Step 1: Run formatting**

Run: `cd rust && cargo fmt --all --check`

Expected: PASS.

- [ ] **Step 2: Run linting**

Run: `cd rust && cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 3: Run the full Rust test suite**

Run: `cd rust && cargo test`

Expected: PASS.

- [ ] **Step 4: Run a manual smoke command**

Run: `cd rust && cargo run -p ccp -- --help`

Expected: help output with `profile`, `run`, and `doctor`.

- [ ] **Step 5: Record any remaining cross-platform gaps explicitly**

Do not declare feature parity with the Bash implementation unless the missing capabilities are listed and accepted.

- [ ] **Step 6: Commit final verification notes if code changes were needed**

```bash
git add rust
git commit -m "chore: finalize rust refactor verification"
```

## Execution Notes

- Implement this plan with `@superpowers:test-driven-development` discipline for each task.
- Before claiming completion, use `@superpowers:verification-before-completion`.
- If executing task-by-task in this session, use `@superpowers:executing-plans`.
- If executing with delegated workers in a future session, use `@superpowers:subagent-driven-development`.
- Do not delete the existing Bash implementation until the Rust version passes the verification gate and an explicit migration decision is made.
