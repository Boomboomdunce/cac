<div align="center">

# cac

**Rust Workspace for Claude Code Privacy Isolation**

**[中文](#中文) | [English](#english)**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)]()
[![Language](https://img.shields.io/badge/Language-Rust-orange.svg)]()

</div>

---

<a id="中文"></a>

## 中文

> **[Switch to English](#english)**

### 为什么这个分支的 cac 是 Rust 版

`feat/rust-workspace-foundation` 分支的重点已经不是顶层 Bash wrapper，而是 `rust/` 里的新工作区。这里的核心二进制是 `ccp`，目标是把项目重构为一个通用的“命令隐私代理”：

- 用独立 profile 隔离 Claude Code 看到的设备身份
- 在进程启动时注入代理、环境变量硬化、运行时 hook 和 sidecar 会话信息
- 保留 Claude 适配器能力，同时把整体架构扩展到跨平台、可适配更多命令

当前已经实现的第一个适配器是 `claude`。这个分支里顶层 Bash `cac`、`src/`、`install.sh` 仍然保留在仓库中，主要用于迁移对照；README 以 Rust 工作区的真实用法为准。

### 当前真实能力

| 分类 | 能力 | 当前状态 |
|:---|:---|:---|
| **Core** | `ccp profile create/activate/show/list/delete` | 已实现 |
| **Core** | `ccp run` 启动通用命令，`claude` 为首个适配器 | 已实现 |
| **Core** | `ccp doctor` 人类可读/JSON 诊断 | 已实现 |
| **Runtime** | 代理注入与启动前 TCP 可达性检查 | 已实现 |
| **Runtime** | Node preload hook + sidecar session 元数据 | 已实现 |
| **Privacy** | 遥测环境变量硬化、第三方 Anthropic 端点清理 | 已实现 |
| **Privacy** | DNS / `net` / `tls` / `fetch` 遥测拦截 | 已实现 |
| **Identity** | `uuid` / `stable_id` / `user_id` / `machine_id` / `hostname` / `mac_address` / `tz` / `lang` 生成 | 已实现 |
| **Identity** | Claude 持久身份同步：`statsig.stable_id.*`、`.claude.json.userID` | 已实现 |
| **Security** | 每 profile mTLS 证书与 CA 材料生成、运行时注入 | 已实现 |
| **Ops** | `ccp setup` / `ccp uninstall` / `ccp pause` / `ccp resume` | 已实现 |
| **Platform** | macOS / Linux / Windows 平台能力层与 CI 验证矩阵 | 已实现 |

更细的覆盖情况见 [rust/docs/coverage-matrix.md](rust/docs/coverage-matrix.md)。

### 仓库结构

```text
rust/
├── apps/ccp/                  # CLI 入口，产出二进制 ccp
├── crates/core/               # profile / policy / capability / launch plan 基础类型
├── crates/store/              # 状态目录布局与持久化
├── crates/launcher/           # 启动计划构建与进程执行
├── crates/doctor/             # 诊断检查与报告渲染
├── crates/sidecar-proto/      # sidecar 协议模型
├── crates/sidecar/            # 会话与审计相关模型
├── crates/adapters/claude/    # Claude 适配器
├── crates/runtime-hooks/node/ # Node 运行时 hook 打包
├── crates/platform-*/         # macOS / Linux / Windows 平台能力实现
└── tests/                     # integration / e2e 测试
```

### 前置条件

1. 已安装 Rust 工具链（`cargo` / `rustc` 可用）。
2. 已安装 Claude Code。
3. 如果你打算执行 `ccp setup` 生成全局 `claude` wrapper，那么真实的 `claude` 必须已经在 `PATH` 里可被发现。
4. 如果要通过代理运行，请准备可访问的 HTTP / HTTPS / SOCKS5 代理地址。

### 快速开始

> 默认情况下，`ccp` 把状态写到当前工作目录下的 `./ccp-state`。以下命令如果在 `rust/` 目录内执行，状态目录就是 `rust/ccp-state`。也可以通过 `CCP_STATE_ROOT` 自定义。

```bash
cd rust

# 1. 创建 profile（Claude 适配器）
cargo run -p ccp -- profile create work --adapter claude --proxy http://127.0.0.1:8080

# 2. 激活 profile
cargo run -p ccp -- profile activate work

# 3. 做一次诊断
cargo run -p ccp -- doctor --profile work

# 4. 通过包装层启动 Claude Code
cargo run -p ccp -- run -- claude
```

如果不传 `--profile`，`ccp run` 会优先使用当前已激活的 profile。

### Profile 创建方式

最小示例：

```bash
cd rust
cargo run -p ccp -- profile create work --adapter claude
```

带代理：

```bash
cd rust
cargo run -p ccp -- profile create work --adapter claude --proxy http://127.0.0.1:8080
```

带 Claude provider 参数：

```bash
cd rust
cargo run -p ccp -- profile create work --adapter claude \
  --base-url https://example.invalid \
  --auth-token your-token
```

如果创建时没有显式传 `--base-url` / `--auth-token` / `--api-key`，当前实现还会尝试从用户的 `~/.claude/settings.json` 快照 Claude provider 配置。

### 可选安装：生成 `ccp` / `claude` wrapper

如果你不想每次都写 `cargo run -p ccp -- ...`，可以让 Rust 版在用户目录安装 wrapper：

```bash
cd rust
cargo run -p ccp -- setup
```

默认行为：

- 在 `~/bin` 生成 `ccp` 和 `claude` wrapper
- 自动探测 shell rc 文件（如 `~/.zshrc` / `~/.bashrc`）
- 追加 `# >>> ccp >>>` 到 `# <<< ccp <<<` 的 PATH 配置块

安装完成后，重新打开终端，或手动重新加载 shell 配置。

卸载：

```bash
cd rust
cargo run -p ccp -- uninstall
```

### 命令

| 命令 | 说明 |
|:---|:---|
| `ccp version` | 输出版本与安装方式 |
| `ccp profile create <name> --adapter claude` | 创建 profile |
| `ccp profile activate <name>` | 激活 profile |
| `ccp profile show <name>` | 显示 profile（敏感字段会脱敏） |
| `ccp profile list` | 列出所有 profile，并标记 active / paused 状态 |
| `ccp profile delete <name>` | 删除 profile 及其状态材料 |
| `ccp doctor --profile <name>` | 执行诊断检查 |
| `ccp doctor --profile <name> --json` | 输出 JSON 诊断结果 |
| `ccp run [--profile <name>] -- <command...>` | 通过隐私包装层运行命令 |
| `ccp setup [--bin-dir <dir>] [--shell-rc <file>]` | 安装 `ccp` / `claude` wrapper |
| `ccp uninstall` | 删除 wrapper 和安装记录 |
| `ccp pause` | 暂停包装层，后续 `run` 直接透传原命令 |
| `ccp resume` | 恢复包装层 |

### 工作原理

```text
command / claude
        |
        v
      ccp
        |
        +--> 读取 active profile / policy / adapter
        +--> 组装 launch plan
        +--> 检查平台 capability
        +--> 检查代理是否可达
        +--> 注入代理、环境变量硬化、mTLS、identity shim
        +--> 注入 Node preload hook 与 sidecar session 元数据
        |
        v
   real claude process
```

对 Claude 适配器，当前 Rust 路径会做这些实际动作：

- 设置 `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` / `NO_PROXY`
- 清理 `ANTHROPIC_BASE_URL` / `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_API_KEY`
- 设置 `DO_NOT_TRACK`、`OTEL_SDK_DISABLED`、`DISABLE_TELEMETRY` 等硬化变量
- 注入 Node preload hook，拦截 DNS / `net` / `tls` / `fetch` 遥测路径
- 生成并注入 mTLS 证书、密钥和 CA
- 导出隔离后的 `HOSTNAME` / `COMPUTERNAME` / `TZ` / `LANG`
- 通过平台 shim 隔离 `hostname`、`machine-id`、`ioreg`、`ifconfig` 等查询结果

### 状态目录

```text
ccp-state/
├── profiles/               # profile JSON
├── identities/<name>/      # uuid / stable_id / user_id / machine_id / hostname / mac_address / tz / lang
├── certs/                  # CA 与每 profile 的 client cert / key
├── hooks/                  # 运行时 hook 物料
├── sessions/               # 启动会话数据
├── sidecar/                # sidecar 状态
├── audit/                  # 审计输出
└── config/                 # install.json / real_claude_path / blocked_hosts 等
```

### 验证

```bash
cd rust

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```

快速查看 CLI：

```bash
cd rust
cargo run -p ccp -- --help
```

### 迁移说明

- 这个分支的 README 故意以 Rust 工作区为主，不再把 Bash `cac` 当成默认路径来写。
- 顶层 Bash 实现仍然保留在仓库里，便于对照、迁移和回归验证。
- 当前真正落地的 adapter 只有 `claude`；更通用的 adapter 生态属于后续扩展，不是这个基础分支的交付重点。

---

<a id="english"></a>

## English

> **[切换到中文](#中文)**

### Why this branch is Rust-first

On `feat/rust-workspace-foundation`, the center of gravity is no longer the top-level Bash wrapper. The real implementation target is the `rust/` workspace, where the primary binary is `ccp`.

This branch turns the project into a generic command privacy proxy that:

- isolates the device identity presented to Claude Code through per-profile state
- injects proxy settings, environment hardening, runtime hooks, and sidecar session metadata at launch time
- keeps Claude support as the first adapter while expanding the architecture toward cross-platform, adapter-based execution

The first implemented adapter is `claude`. The legacy Bash `cac`, `src/`, and `install.sh` are still kept in the repository for migration reference, but this README documents the real Rust path of this branch.

### Current capabilities

| Area | Capability | Status |
|:---|:---|:---|
| **Core** | `ccp profile create/activate/show/list/delete` | Implemented |
| **Core** | `ccp run` for generic command launch, with `claude` as the first adapter | Implemented |
| **Core** | `ccp doctor` in human-readable and JSON modes | Implemented |
| **Runtime** | proxy injection and pre-launch TCP reachability checks | Implemented |
| **Runtime** | Node preload hook and sidecar session metadata | Implemented |
| **Privacy** | telemetry env hardening and third-party Anthropic endpoint unsetting | Implemented |
| **Privacy** | DNS / `net` / `tls` / `fetch` telemetry interception | Implemented |
| **Identity** | generation of `uuid`, `stable_id`, `user_id`, `machine_id`, `hostname`, `mac_address`, `tz`, and `lang` | Implemented |
| **Identity** | Claude persistent identity sync for `statsig.stable_id.*` and `.claude.json.userID` | Implemented |
| **Security** | per-profile mTLS material generation and runtime injection | Implemented |
| **Ops** | `ccp setup` / `ccp uninstall` / `ccp pause` / `ccp resume` | Implemented |
| **Platform** | macOS / Linux / Windows platform layers and CI validation matrix | Implemented |

For a more detailed parity view, see [rust/docs/coverage-matrix.md](rust/docs/coverage-matrix.md).

### Workspace layout

```text
rust/
├── apps/ccp/                  # CLI entrypoint, builds the ccp binary
├── crates/core/               # profile / policy / capability / launch plan types
├── crates/store/              # state-root layout and persistence
├── crates/launcher/           # launch-plan assembly and process execution
├── crates/doctor/             # diagnostic checks and report rendering
├── crates/sidecar-proto/      # sidecar protocol model
├── crates/sidecar/            # session and audit-facing foundations
├── crates/adapters/claude/    # Claude adapter
├── crates/runtime-hooks/node/ # packaged Node runtime hooks
├── crates/platform-*/         # macOS / Linux / Windows platform providers
└── tests/                     # integration and e2e tests
```

### Prerequisites

1. A working Rust toolchain (`cargo` and `rustc`).
2. Claude Code installed.
3. If you plan to run `ccp setup`, the real `claude` executable must already be discoverable in `PATH`.
4. If you want proxied execution, have a reachable HTTP / HTTPS / SOCKS5 proxy ready.

### Quick start

> By default, `ccp` writes state into `./ccp-state` under the current working directory. If you run the commands below inside `rust/`, your state root will be `rust/ccp-state`. You can override it with `CCP_STATE_ROOT`.

```bash
cd rust

# 1. Create a profile for the Claude adapter
cargo run -p ccp -- profile create work --adapter claude --proxy http://127.0.0.1:8080

# 2. Activate it
cargo run -p ccp -- profile activate work

# 3. Run diagnostics
cargo run -p ccp -- doctor --profile work

# 4. Launch Claude Code through the privacy wrapper
cargo run -p ccp -- run -- claude
```

If you omit `--profile`, `ccp run` uses the currently active profile.

### Creating profiles

Minimal example:

```bash
cd rust
cargo run -p ccp -- profile create work --adapter claude
```

With a proxy:

```bash
cd rust
cargo run -p ccp -- profile create work --adapter claude --proxy http://127.0.0.1:8080
```

With explicit Claude provider settings:

```bash
cd rust
cargo run -p ccp -- profile create work --adapter claude \
  --base-url https://example.invalid \
  --auth-token your-token
```

If `--base-url`, `--auth-token`, or `--api-key` are not passed explicitly, the current implementation also attempts to snapshot Claude provider settings from `~/.claude/settings.json`.

### Optional install: generate `ccp` / `claude` wrappers

If you do not want to keep using `cargo run -p ccp -- ...`, the Rust path can install user-level wrappers:

```bash
cd rust
cargo run -p ccp -- setup
```

By default this will:

- generate `ccp` and `claude` wrappers in `~/bin`
- detect a shell rc file such as `~/.zshrc` or `~/.bashrc`
- append a PATH block delimited by `# >>> ccp >>>` and `# <<< ccp <<<`

After setup, reopen the terminal or reload your shell config.

Uninstall:

```bash
cd rust
cargo run -p ccp -- uninstall
```

### Commands

| Command | Description |
|:---|:---|
| `ccp version` | Print version and install method |
| `ccp profile create <name> --adapter claude` | Create a profile |
| `ccp profile activate <name>` | Activate a profile |
| `ccp profile show <name>` | Show a profile with sensitive fields redacted |
| `ccp profile list` | List profiles and mark active / paused state |
| `ccp profile delete <name>` | Delete a profile and its persisted materials |
| `ccp doctor --profile <name>` | Run diagnostics |
| `ccp doctor --profile <name> --json` | Emit JSON diagnostics |
| `ccp run [--profile <name>] -- <command...>` | Run a command through the privacy wrapper |
| `ccp setup [--bin-dir <dir>] [--shell-rc <file>]` | Install `ccp` / `claude` wrappers |
| `ccp uninstall` | Remove generated wrappers and install metadata |
| `ccp pause` | Pause the wrapper so future `run` calls pass through directly |
| `ccp resume` | Resume wrapper behavior |

### How it works

```text
command / claude
        |
        v
      ccp
        |
        +--> loads active profile / policy / adapter
        +--> builds a launch plan
        +--> checks platform capabilities
        +--> verifies proxy reachability
        +--> injects proxy, env hardening, mTLS, and identity shims
        +--> attaches Node preload hooks and sidecar session metadata
        |
        v
   real claude process
```

For the Claude adapter, the current Rust path actually does the following:

- sets `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, and `NO_PROXY`
- unsets `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, and `ANTHROPIC_API_KEY`
- sets hardening variables such as `DO_NOT_TRACK`, `OTEL_SDK_DISABLED`, and `DISABLE_TELEMETRY`
- injects a Node preload hook that intercepts DNS / `net` / `tls` / `fetch` telemetry paths
- generates and injects mTLS cert, key, and CA materials
- exports isolated `HOSTNAME`, `COMPUTERNAME`, `TZ`, and `LANG`
- isolates hostname and device-identification lookups through platform shims such as `hostname`, `machine-id`, `ioreg`, and `ifconfig`

### State root

```text
ccp-state/
├── profiles/               # profile JSON
├── identities/<name>/      # uuid / stable_id / user_id / machine_id / hostname / mac_address / tz / lang
├── certs/                  # CA plus per-profile client cert / key
├── hooks/                  # runtime hook assets
├── sessions/               # launch-session data
├── sidecar/                # sidecar state
├── audit/                  # audit outputs
└── config/                 # install.json / real_claude_path / blocked_hosts and related files
```

### Verification

```bash
cd rust

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```

Quick CLI smoke check:

```bash
cd rust
cargo run -p ccp -- --help
```

### Migration notes

- This branch intentionally documents the Rust workspace as the primary path.
- The top-level Bash implementation is still present for comparison, migration, and regression checks.
- `claude` is the only fully implemented adapter today; broader adapter expansion is future product work, not the goal of this foundation branch.
