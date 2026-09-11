# 手册：在本仓开关排版修复的 hook，与怎么查它的诊断

本文是操作手册，不是决策记录 —— 挂哪个 hook、为什么按批就地修、跨批为什么靠前缀重放、两道 fail-open 是什么，正本都在 `docs/adr/0016-hook-mechanical-only.md`，本文不重复。文件名里的 polish 是历史：这条 hook 曾经调模型润色，2026-09-11 起只做 `limae --fix` 那一套机械排版修复，不再调任何模型 (ADR-0016)。

一句话：`limae hook` 在 Claude Code 显示每一批回复的时候，把这一批过一遍本仓的确定性修复，修后文本只替换屏幕上的这一批；transcript 与模型看到的内容不动。任何一步出问题都只留下原文 (fail-open)，并在会话态诊断里记一行。

**Codex 那侧的 hook 已废弃** (ADR-0016 §六)：Codex 没有等价于 `MessageDisplay` 的替换事件，`.codex/config.toml` 已删除；历史读数见 `docs/adr/0014-codex-stop-polish-hook.md`「实测」一节。

## 一、怎么开

hook 配置写进 `.claude/settings.local.json`。这个文件不入库 (见 `.gitignore`)，只影响在本仓开的会话。

```json
{
  "hooks": {
    "MessageDisplay": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "\"$CLAUDE_PROJECT_DIR/target/debug/limae\" hook"
          }
        ]
      }
    ]
  }
}
```

两处要注意：

- **只挂 `MessageDisplay`，不要 `timeout`**：一批的成本是整篇重修的十几毫秒 (release 构建，ADR-0016 读数 C) 加最多 2 秒的兄弟批等待，落在宿主给 `MessageDisplay` 的默认 10 秒之内。`Stop` 不用挂：进程收到它只是静默退出 0，挂着是白起一次进程。
- **走 `target/debug/limae` 这个已建好的 binary，不要写 `cargo run`**：这条命令每批新行都要起一次进程 (一条回复十几次到上百次)，而且各批是并发派发的 —— `cargo run` 会让它们排在 Cargo 的 build 目录锁上，一个一个来。没有 `target/debug/limae` 就先 `cargo build`；**改完 Rust 源码要重建**，否则挂着的是上一次建出来的那个。

改完**重开一个 Claude Code 会话**才生效。

## 二、怎么关

按影响面从小到大：

1. **只关本次**：把 `LIMAE_HOOK_DISABLE=1` 加进 `.claude/settings.local.json` 的 `env` 表 (或在启动 agent 的环境里设)，进程一进来就退出，连 stdin 都不读。
2. **关掉这台机器上的本仓**：删掉上面那段 `hooks`，重开会话。
3. **彻底**：删掉 `.claude/settings.local.json`。

## 三、旋钮与配置

只剩一个环境变量：

| 变量 | 默认 | 管什么 |
| --- | --- | --- |
| `LIMAE_HOOK_DISABLE` | 空 | 非空即全关 |

`LIMAE_HOOK_MIN_CHARS`、`LIMAE_HOOK_AB_RATE`、`LIMAE_HOOK_TIMEOUT` 已随模型润色与 A/B 一起退役 (ADR-0016 §四)，设了也没有作用。

**修什么由仓级规则配置决定，与 `limae --fix` 读的是同一份**：从事件里的 `cwd` 起按 `rust/config.rs` 的 `find_config` 逐层向上找 `limae.toml` 或 `pyproject.toml` 的 `[tool.limae]`，碰到含 `.git` 的那一层就停；一层都没有就是默认集。所以仓库里 `disable` 掉的规则在屏幕上同样不修。**配置错误的处理与 CLI 相反**：CLI 报错退出 2，hook 这一批原文放行、记一行 `fix` / `config` (见第五节)。没有用户级配置，也不会在家目录创建任何东西 (ADR-0016 §三)。

## 四、会话态目录里有什么

每一批都是一个新进程，跨批的状态只能经磁盘传 (ADR-0016 §二)。状态就是**这条消息到目前为止的原文**，按批分文件：

```text
$TMPDIR/limae-hook/<session_id>/parts/<message_id>.<turn_id>/000000.part
$TMPDIR/limae-hook/<session_id>/parts/<message_id>.<turn_id>/000001.part
$TMPDIR/limae-hook/<session_id>/diagnostics.jsonl
```

- **这个位置没有配置项**，也不接受任何一个项目的配置去改它 —— 分片里是助手回复原文，一旦落进工作树就可能被 `git add` 带走，而本仓是 public 仓；程序另会拒绝任何解析后落在 git 工作树里的状态目录，命中就什么都不写、也不修。
- 目录 0700、文件 0600，重启即随 `$TMPDIR` 一起没。
- **清理只靠年龄，不靠末批**：每个事件都顺手扫一遍 —— 超过 1 小时没动过的消息分片目录、超过 24 小时没动过的会话目录，扫掉。末批不删自己的分片，因为它前面的某一批可能还在跑、还在读同一个目录；被打断的消息根本收不到末批 (2026-09-11 实测：esc 与 Ctrl-C 各一次，被打断的消息所有批次都是 `final: false`，之后没有了)，中断很频繁，所以按年龄扫是常态路径，不是兜底。
- 分片目录里若多出一个 `void` 文件，说明这条消息实例已作废 (同一个 index 被重送了不同内容、或消息超过了字节数 / 分片数上限)：之后各批一律原样上屏，目录留给年龄清理。
- 它含助手回复原文，**不进本仓库、不经 agent 间通道传递** (ADR-0009 §八、ADR-0012 立的边界，ADR-0016 §二 一条不少地继承)。

## 五、出问题了怎么查

**症状永远是「排版没修」**，因为任何失败都退回原文，不会报错、不会卡住。失败会留下痕迹 —— 先看这个文件，再动手复现：

```sh
tail "${TMPDIR:-/tmp}"/limae-hook/*/diagnostics.jsonl
```

**这个文件不存在，说明这个会话还没失败过** —— shell 会报 `no matches found` 或 `No such file`，那不是命令打错了。屏幕上没修又没有这个文件，多半是这一批本来就干净 (与 `--fix` 输出相同时 hook 什么都不输出)，那不算失败，所以不记。

有的话，一行一次失败，四个字段：

| 字段 | 是什么 |
| --- | --- |
| `at` | UTC 时间 |
| `message_id` | 是哪条回复，用来跟屏幕上的回复对上 |
| `step` | 哪一步：`siblings` 等兄弟批次、拼前缀与读前缀上的两个信号，`fix` 跑规则，`display` hook 自己崩了 |
| `kind` | 哪一类，见下表 |

`kind` 是**完整的一张表** —— 按它反查就能定位，漏一类就等于那类失败查不到；`tools/check_repo_contracts.sh` 拿代码里的全集 (Cargo example `hook-kinds` 打印) 对着它核，漏一行门就红：

| `kind` | 什么意思 | 下一步 |
| --- | --- | --- |
| `incomplete` | 前面有一批没在 2 秒内落盘，这一批的前缀拼不全；或者这条消息实例已作废 (同一 index 重送了不同内容、超过字节数或分片数上限)，本批与之后各批都记这一条 | 正常现象，见本节末尾；连续出现且回复不长，去核宿主的分批行为 |
| `partial` | 批次边界不在行边界上：前缀不以换行结尾，或一个中间批自己不以换行结尾 | 去核 ADR-0016 读数 A 的前提 —— 宿主的分批行为变了 |
| `unclosed` | 这一批结尾可能还在一个行内代码 span 里 (有一段反引号串没配对上)，要等下一批才知道，所以这一批原样上屏 | 不是故障，是正常现象：开反引号在行尾的写法每次都会触发一次；下一批照常修 |
| `config` | 仓级规则配置读不了 (不是合法 toml，或触犯 `spec/rules.md`「配置错误」清单)；或者回复里的行内指令点名了一个不存在的规则 id | 在那个仓里跑一次 `limae --all`，CLI 会把配置错误或指令错误打出来 |
| `crashed` | 这一步自己崩了：状态目录不合作、分片读不出来、修后行数与修前不一致 | 这是 bug，请报 |

**这个文件里不会有正文** —— 只有上面四个字段。保留期跟分片一样，随会话目录一起在 24 小时后被清掉。

看完痕迹再复现：

```sh
# 手工喂一个中间批进去，看它输不输出 displayContent。
printf '%s' '{"session_id":"probe","cwd":"'"$PWD"'",
"hook_event_name":"MessageDisplay","turn_id":"t","message_id":"m","index":0,
"final":false,"delta":"你好,世界\n"}' \
  | TMPDIR=$(mktemp -d) ./target/debug/limae hook
```

- 应输出 `{"hookSpecificOutput":{"hookEventName":"MessageDisplay","displayContent":"你好，世界\n"}}`。什么都不输出：先看 `cwd` 所在仓库的配置有没有关掉 zh-typography-1，再看 `$TMPDIR` 里的 `diagnostics.jsonl`。
- 输出了但会话里没效果：多半是会话没重开，或挂着的 binary 是旧的。
- stderr 出现 `limae hook: m/n earlier batches arrived`：宿主是并发派发每一批的，这条说明前面有一批没在 2 秒内落盘。这一批按原文显示，不会拿残缺的前缀去修 —— 只记片数，不记内容。同一件事在 `diagnostics.jsonl` 里是一行 `siblings` / `incomplete`。
- **消息被打断** (esc、Ctrl-C，或 `--resume` 重启)：宿主不再发后续批次，已经上屏的各批都已经各自修过了，什么都不用做。它留下的分片目录会在一小时后被扫掉。
