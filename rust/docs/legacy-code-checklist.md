# Legacy `cac` vs Rust `ccp` 逐项核对表

更新日期：2026-03-23

本文只基于当前仓库里的实际代码、测试与已完成的本地实机验证整理，不以 README 口径为准。

状态说明：

- `已真实验证`：已用真实本地 `claude` 进程和真实网络流量验证过。
- `已实现并自动化验证`：代码已实现，且仓库内测试/CI 已覆盖，但本轮未在三端都做真人实机。
- `部分实现`：核心代码存在，但与 legacy 目标或原始意图相比仍有边界/缺口。
- `未实现`：当前代码库里找不到对应实现。

## 核对表

| 能力项 | Legacy Bash 代码 | Rust 代码 | 现状 | 证据 / 说明 |
| --- | --- | --- | --- | --- |
| 安装包装器并写入 shell PATH | `src/cmd_setup.sh`, `src/utils.sh`, `src/templates.sh` | `rust/apps/ccp/src/install.rs`, `rust/tests/integration/cli_profile.rs` | 已实现并自动化验证 | Rust 会生成 `ccp` shim 和 `claude` wrapper，并写入 shell block；`setup_generated_claude_wrapper_routes_through_ccp` 已验证 wrapper 确实通过 `ccp` 启动。 |
| 创建/列出/激活环境 | `src/cmd_env.sh`, `src/main.sh` | `rust/apps/ccp/src/main.rs`, `rust/tests/integration/cli_profile.rs` | 已实现并自动化验证 | Rust 改成 `ccp profile create|list|activate|show|delete`，能力等价但命令面已重设计。 |
| 代理简写格式：`host:port` / `host:port:user:pass` | `src/cmd_env.sh`, `src/utils.sh` | `rust/apps/ccp/src/main.rs`, `rust/tests/integration/cli_profile.rs` | 已实现并自动化验证 | Rust 现在也接受 legacy 的两种简写代理格式，并在 profile 中规范化保存为 URL。 |
| 使用激活环境默认运行 | `src/templates.sh`, `src/main.sh` | `rust/apps/ccp/src/main.rs`, `rust/tests/e2e/run_claude_smoke.rs` | 已实现并自动化验证 | `run_without_profile_uses_active_profile` 已验证不显式传 `--profile` 时会读取 active profile。 |
| 临时停用 / 恢复包装 | `src/cmd_stop.sh` | `rust/apps/ccp/src/main.rs`, `rust/crates/store/src/runtime_state_store.rs`, `rust/tests/e2e/run_claude_smoke.rs` | 已实现并自动化验证 | Legacy 的 `cac stop` / `cac -c` 对应 Rust 的 `ccp pause` / `ccp resume`。 |
| 删除环境与卸载 | `src/cmd_delete.sh` | `rust/apps/ccp/src/main.rs`, `rust/apps/ccp/src/install.rs`, `rust/tests/integration/cli_profile.rs` | 已实现并自动化验证 | Rust 区分 `profile delete` 和 `uninstall`，不再是一个命令同时承担两类职责。 |
| 启动前检测代理可达性 | `src/templates.sh`, `src/utils.sh` | `rust/crates/launcher/src/builder.rs`, `rust/tests/integration/launch_plan.rs` | 已实现并自动化验证 | Rust 在构建 launch plan 时做 TCP reachability 检查，等价于 legacy wrapper 的 `/dev/tcp` pre-flight。 |
| 注入 `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` / `NO_PROXY` | `src/templates.sh` | `rust/crates/launcher/src/builder.rs`, `rust/tests/e2e/run_claude_smoke.rs` | 已真实验证 | 已在真实本地 `claude` 经过 `ccp` 时抓到代理 CONNECT；e2e 也校验了四个环境变量。 |
| 按代理出口推断 `TZ` / `LANG` | `src/cmd_env.sh` | `rust/apps/ccp/src/main.rs`, `rust/tests/integration/cli_profile.rs` | 已实现并自动化验证 | Rust 用 `ipify` + `ip-api` 推断 locale；已由 `profile_create_derives_tz_and_lang_from_proxy_exit_metadata` 覆盖。 |
| 生成伪造身份材料：`uuid` / `stable_id` / `user_id` / `machine_id` / `hostname` / `mac_address` / `tz` / `lang` | `src/cmd_env.sh`, `src/utils.sh` | `rust/crates/store/src/identity_store.rs`, `rust/tests/integration/cli_profile.rs` | 已实现并自动化验证 | Rust 已全部落盘到 `identities/<profile>/`。 |
| 同步 Claude 持久身份：`~/.claude/statsig/*` 与 `~/.claude.json.userID` | `src/templates.sh`, `src/utils.sh` | `rust/apps/ccp/src/main.rs`, `rust/tests/integration/cli_profile.rs`, `rust/tests/e2e/run_claude_smoke.rs` | 已实现并自动化验证 | Rust 现在既会在 `profile activate` 时同步，也会在 `run` 时再次兜底同步；`profile_activate_syncs_claude_identity_files_immediately` 与 `run_claude_syncs_persistent_claude_identity_files` 已覆盖。 |
| 多层环境变量遥测抑制 | `src/templates.sh` | `rust/crates/adapters/claude/src/env.rs`, `rust/tests/integration/claude_adapter.rs`, `rust/tests/e2e/run_claude_smoke.rs` | 已实现并自动化验证 | 包含 `DO_NOT_TRACK`、`OTEL_*`、`SENTRY_DSN`、`DISABLE_TELEMETRY` 等；e2e 会校验注入结果。 |
| 清理 `ANTHROPIC_BASE_URL` / `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_API_KEY` 进程环境 | `src/templates.sh` | `rust/crates/adapters/claude/src/env.rs`, `rust/tests/e2e/run_claude_smoke.rs` | 已实现并自动化验证 | 进程环境里的三项变量会被 unset；fake Claude e2e 已验证这一点。 |
| Claude 用户配置隔离：不再直接读取全局 `~/.claude/settings.json` | Legacy 无 | `rust/apps/ccp/src/main.rs`, `rust/crates/store/src/claude_config_store.rs`, `rust/tests/e2e/run_claude_smoke.rs` | 已真实验证 | Rust 现已为 wrapped Claude materialize 受控 `CLAUDE_CONFIG_DIR`，真实 debug log 已确认 Claude watch 的是 `state/config/claude-config/<profile>/settings.json`，不再直接 watch 用户全局配置文件。 |
| Claude `settings.json` 的 profile 快照隔离 | Legacy 无 | `rust/crates/store/src/claude_config_store.rs`, `rust/tests/e2e/run_claude_smoke.rs` | 已实现并自动化验证 | managed `settings.json` 以 profile 本地快照为基底，不会在每次 `run` 时被用户全局 `.claude/settings.json` 覆写。 |
| Claude provider 改为 profile 显式持有 | Legacy 只会 `unset` 进程环境 | `rust/crates/core/src/profile.rs`, `rust/apps/ccp/src/main.rs`, `rust/crates/store/src/claude_config_store.rs`, `rust/tests/integration/cli_profile.rs`, `rust/tests/e2e/run_claude_smoke.rs` | 已实现并自动化验证 | 新建 claude profile 时会把 provider 从 `--base-url/--auth-token/--api-key` 或当前用户 settings 快照进 profile；managed `settings.json` 随后按 profile provider 生成，`profile show` 会红acted secret。 |
| 写入 Node preload hook | `src/dns_block.sh` | `rust/hooks/node/claude-preload.js`, `rust/apps/ccp/src/main.rs`, `rust/tests/integration/claude_adapter.rs` | 已实现并自动化验证 | Rust 通过 runtime hook materialization + `NODE_OPTIONS=--require ...` 注入。 |
| 阻断遥测域名：`dns.lookup` / `dns.resolve*` / `dns.promises` | `src/dns_block.sh` | `rust/hooks/node/claude-preload.js`, `rust/tests/integration/claude_adapter.rs`, `rust/apps/ccp/src/main.rs` | 已实现并自动化验证 | `doctor` live self-audit 会真实执行 `dns.lookup('statsig.anthropic.com')` 并要求返回 `ECONNREFUSED`。 |
| 阻断 `net.connect` / `net.createConnection` / `tls.connect` / `fetch` 到遥测域名 | `src/dns_block.sh` | `rust/hooks/node/claude-preload.js`, `rust/tests/integration/claude_adapter.rs` | 已实现并自动化验证 | Rust preload 已覆盖 legacy 的 Node monkey patch 范围；`tls.connect` 阻断已有集成测试。 |
| `HOSTALIASES` 备用层 | `src/dns_block.sh`, `src/templates.sh` | `rust/apps/ccp/src/main.rs`, `rust/crates/store/src/blocked_hosts_store.rs`, `rust/tests/e2e/run_claude_smoke.rs` | 已实现并自动化验证 | Rust 会 materialize `config/blocked_hosts` 并导出 `HOSTALIASES`。 |
| mTLS：生成 CA / client cert / client key，并注入 Node 信任链 | `src/mtls.sh`, `src/templates.sh`, `src/dns_block.sh` | `rust/apps/ccp/src/main.rs`, `rust/crates/store/src/cert_store.rs`, `rust/hooks/node/claude-preload.js`, `rust/tests/e2e/run_claude_smoke.rs` | 已实现并自动化验证 | Rust 会导出 `CCP_MTLS_*` / `CAC_MTLS_*` / `NODE_EXTRA_CA_CERTS`，preload 中也会在匹配代理目标时自动注入 cert/key。 |
| Unix 指纹替换：`hostname` / `cat /etc/machine-id` / `ioreg` / `ifconfig` | `src/templates.sh` | `rust/crates/store/src/shim_store.rs`, `rust/tests/e2e/run_claude_smoke.rs` | 已实现并自动化验证 | macOS/Unix 路径已经被 e2e 使用 fake Claude 覆盖。 |
| Windows 指纹替换：`hostname` / `getmac` / `wmic` / `reg query` / `powershell` / `COMPUTERNAME` | Legacy 无完整 Windows 覆盖 | `rust/crates/store/src/shim_store.rs`, `rust/apps/ccp/src/main.rs`, `rust/tests/e2e/run_claude_smoke.rs` | 已实现并自动化验证 | 这是 Rust 超过 legacy 的部分；代码和测试已补齐，但本轮未在真实 Windows 上跑真实 Claude。 |
| `check` / `doctor` 健诊能力 | `src/cmd_check.sh` | `rust/crates/doctor/src/checks.rs`, `rust/apps/ccp/src/main.rs`, `rust/tests/integration/cli_profile.rs` | 已实现并自动化验证 | Rust 输出形式已改为 `doctor`，但能力现已覆盖 profile、identity、mTLS、proxy reachability、exit IP、local proxy conflicts（含本地代理进程、TUN、macOS system proxy、直连/代理同出口）、runtime self-audit、live self-audit（含 legacy 12 层遥测环境变量核对）。 |
| 真实本地 `claude` 走代理且外联目标可观测 | Legacy 无 Rust 同级验证工件 | `rust/apps/ccp/src/main.rs` 运行链路，外加本轮真实抓包/代理日志 | 已真实验证 | 已确认真实本地 `claude` 经 `ccp` 后可观测到 `api.ipify.org`、`api.anthropic.com`、`raw.githubusercontent.com`、`platform.claude.com`、`code.nextcloud.games` 等目标。 |
| 对 `code.nextcloud.games` 的重复 CONNECT 是否由 `ccp` 代理握手失败导致 | Legacy 无此结论 | 本轮真实 `claude --debug api` + 代理 CONNECT 时间戳对齐分析 | 已真实验证 | 当前证据指向“不是 `ccp` 代理握手失败”。CONNECT 均成功，且新增 CONNECT 与 Claude 的新 `[API:request]` 回合对齐。 |
| 旧 CLI 语义完全兼容 | `src/main.sh`, `src/cmd_env.sh`, `src/cmd_stop.sh`, `src/cmd_help.sh` | `rust/apps/ccp/src/main.rs` | 部分实现 | 核心能力保留，但命令模型已改成 `profile`/`run`/`doctor`/`pause`/`resume`/`setup`/`uninstall`，不再兼容 `cac add us1 ...`、`cac us1`、`cac check` 这套原命令。 |

## 关键边界

### 1. “强制走官方端点”仍然不是当前保证

实际代码层面，Rust 版现在做了两件事：

- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_AUTH_TOKEN`
- `ANTHROPIC_API_KEY`
- 为 Claude 生成并注入受控 `CLAUDE_CONFIG_DIR`

这修复了“wrapped Claude 直接读取用户全局 `~/.claude/settings.json`”的问题；并且 provider 现在由 profile 显式持有，managed `settings.json` 会按 profile provider 写回受控 config dir，而不是每次重新读取用户全局 settings。因此：

- “Rust 已成功清理进程环境变量”这个说法成立。
- “wrapped Claude 不再直接读取用户全局设置”这个说法现在成立。
- “profile 现在明确拥有 provider 配置”这个说法现在成立。
- “因此真实 Claude 一定回到 Anthropic 官方端点”这个推论仍然不成立，因为 profile 自身仍然可以显式配置自定义 provider。

如果后续要把“是否允许自定义 provider”也纳入更强的安全策略，就需要再加一层 policy：例如只允许官方端点、只允许白名单 provider、或要求 `doctor` 对 provider 发出告警。

### 2. `code.nextcloud.games` 的重复 CONNECT 目前更像 Claude 自身多回合请求，不像 `ccp` 故障

本轮真实验证里已经确认：

- CONNECT 握手返回成功，不是 407/502/超时。
- `claude --debug api` 中每次新 `[API:request]` 出现时，代理日志里都会出现新的 `code.nextcloud.games:443` CONNECT。
- 在一个包含 2 次 `Read` 工具回合的真实任务中，总共出现了 3 次 `code.nextcloud.games:443` CONNECT：初始模型请求 1 次，后续两个工具回合各 1 次。
- 调试日志里没有看到 `retry`、`timeout`、`abort`、`reconnect` 之类直接失败信号。

基于现有证据，更合理的结论是：

- 这些重复 CONNECT 主要对应 Claude 的多回合 API 请求。
- 目前没有证据表明是 `ccp` 代理层导致的异常重连。
- 至于“为什么 provider 没有复用单条长连接”，更可能是 Claude 客户端请求模型或 `code.nextcloud.games` 服务端的 keep-alive / 会话策略问题，而不是 `ccp` 的 CONNECT 建立失败。

### 3. Node 进程内 monkey patch 仍然是 Rust 版的核心实现手段之一

就当前代码而言，Rust 版虽然整体控制面已经改成 Rust CLI，但对 Claude 的核心隐私强化仍然主要依赖：

- 启动前环境变量重写
- PATH shim
- Node preload monkey patch
- 持久身份文件改写

也就是说，当前 Rust 版不是“完全摆脱 monkey patch 的纯系统级隔离器”；它本质上仍是“Rust 控制面 + Node 运行时 hook”的架构。

## 当前结论

如果只看“legacy Bash 现有能力是否已大体迁到 Rust”，答案是：核心能力已经基本迁完，而且现阶段找不到明确的“legacy 已实现但 Rust 代码层仍缺失”的核心能力项。

当前仍然没有刻意去追求的，主要只剩两类：

1. 旧 CLI 的命令词、帮助文案、版本输出、交互提示并未做 1:1 兼容，这是保留核心能力前提下的有意重设计。
2. 原始安全意图里更强的“系统级隔离”与“provider 强策略”还可以继续加强，当前最值得继续补的是这两件事：

1. 决定是否要把当前 Node 级阻断继续下沉到进程外/系统级，避免被非 Node 子进程或原始 IP 连接绕过。
2. 决定是否把 provider 再加一层强策略控制，例如官方端点强制模式或 provider 白名单。
