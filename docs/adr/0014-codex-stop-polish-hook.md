# ADR-0014：Codex 用 Stop 在原回复下追加润色版

- 日期：2026-09-05
- 承接：ADR-0009 的润色、计数、记录、失败放行与会话态边界

## 背景

ADR-0009 的显示合同建立在 Claude Code 的 `MessageDisplay` 上：宿主把流式回复分批交给 hook，末批可以用 `displayContent` 替换。但 Codex 没有这个事件，不能照搬同一份宿主输入输出。

OpenAI 的 [Hooks 文档](https://learn.chatgpt.com/zh-Hans/docs/hooks) 在 2026-09-05 记载：Codex 的 `Stop` 输入含完整的 `last_assistant_message`、`session_id` 与 `turn_id`；输出中的 `systemMessage` 会在界面或事件流中显示为 warning。`decision: "block"` 与 `reason` 则会让 Codex 继续运行，产生一个新的模型轮次。

用户接受 Codex 保留原回复、在下方再显示一份润色版。本仓需要先在 limae 内试用，不改用户级 Codex 配置，也不把润色稿写回 transcript。

## 决定

### 一、Codex 挂 Stop，返回 systemMessage

Codex 的 `Stop` hook 直接润色 `last_assistant_message`，以 `systemMessage` 返回既有的润色块。原回复先由宿主正常显示，warning 随后出现在下方；hook 不改 transcript。

输出不带 `decision: "block"`、`reason` 或 `continue: false`，不向宿主请求续跑。输入没有完整回复、回复低于阈值或任一步失败时，hook 退出 0 且不输出，界面只保留原回复。

Claude Code 的 `MessageDisplay` 批缓存与 `Stop` 回注路径不变。两种宿主共用 `_block` 之后的润色、改动计数、确定性修复、A/B 台账、单路记录与诊断。

### 二、本仓配置只开单路试用

把 Codex hook 配置提交在 `.codex/config.toml`，只由信任这个 checkout 的 Codex 会话加载；不改 `~/.codex/config.toml`。试用配置固定 `LIMAE_HOOK_AB_RATE=0`，每条合格回复只追加一份润色版。A/B 能力仍留在实现中，但不是本轮 Codex 试用的界面。

配置通过 `git rev-parse --show-toplevel` 找到本 checkout 的 `.venv/bin/limae`。新 checkout 须先 `uv sync`；缺少可执行文件时沿用失败放行，只显示原回复。

### 三、润色子进程带禁用标记

hook 调任何预设或自定义引擎时，都给子进程设置 `LIMAE_HOOK_DISABLE=1`。即使被调用的引擎也是 Codex，且将来能读到项目级或用户级同类 hook，子进程触发的 hook 也会立即退出，不会递归润色润色器自己的回复。

Codex 输入没有 Claude Code 的 `message_id`，单路记录改用同一事件的 `turn_id` 命名；`session_id` 继续划分会话态目录。存储位置、权限、保留期与不进仓库的边界均沿用 ADR-0009 与 ADR-0012。

## 后果

- Codex 用户会看到原回复与下面一块 warning 样式的润色版，而不是屏幕内替换。hook 没有改 `last_assistant_message`；Codex 是否另把 warning 事件保存在会话记录里，是另一个问题，本 ADR 不作断言。
- `systemMessage` 在官方合同里是 warning。本适配不返回官方明定会续跑的 `decision: "block"` / `reason`；是否出现额外模型轮次以本仓真实试用的事件流为准。
- 本仓试用稳定后，是否推广到用户级配置仍须在 `machine-setup` 另走 PR，本 ADR 不授权推广。

## 实测

2026-09-05 在装机版 `codex-cli 0.153.0` 的真实 TUI 做了两组对照：

- 固定输出组：同一次合成回复下，配置 `Stop` hook 时，原回复后出现唯一一条 `Hook · LIMAE_FIXED_SYSTEM_MESSAGE`；把 `hooks.Stop` 置空后，只出现原回复。两边都回到输入框。
- 真实润色组：正臂让本 ADR 的 hook 处理一段 220 字的合成回复，原回复后出现唯一一条 `Hook` warning，含完整润色稿与「3 处改动」，约 13 秒后回到输入框；对照臂使用同一正文、同一 `gpt-5.6-sol high`，只把 `hooks.Stop` 置空，没有出现 warning。正臂捕获到的画面里没有第二条助手回复或第二次 hook，footer 仍是 `gpt-5.6-sol high`。

任务使用的是 Git linked worktree。该 worktree 自己的 `.codex/config.toml` 在这次装机版实测中没有触发 hook，所以上述真实润色正臂把同一份 `hooks.Stop` 作为会话级配置传入；这条观察不拿来证明提交后的 repo-local 配置可安装。最终配置另在普通 clone 中验收，主 checkout 合入后的热加载与 resume 边界由常驻会话单独验证。

## 状态

accepted (2026-09-05)。
