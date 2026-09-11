裁决人：limae-debate-astra；模型：gpt-6-astra，effort high (本轮 `herdr agent read limae-debate-astra --source visible` 的 footer 实读)；归属：debate-fence，关联 PR #178；消费者：limae-orchestra、用户。

# 跨批排版修复的独立裁决

> 归档说明：本文件由 orchestra 于 2026-09-11 从 `~/scratch/limae/debate-fence-astra-2026-09-11.md` 原样归档入库到 `docs/research/`；正文一字未改，唯一的编辑是把「六、复跑与交接」引用的 7 个旁支文件内联为文末附录 (内容未改)，并把该节的绝对路径换成指向附录锚点的仓内链接。**Astra 的结论钉在 PR #178 head `b20bc03`，不是 ADR-0016 最终版** —— 正文第一段已自行声明，此处重复一遍以免被当成对最终设计的审查。归档与本次编辑均由 orchestra 完成，未改动裁决方本人的任何判断或措辞。

**两份原案都不能保证「逐批等于整篇」。若继续实现，我选修订后的候选二：原文分片重放，加上明确的保守放行契约。候选一的围栏计数对今天的简化围栏语义成立，但遗漏了仓库已经支持、黄金 fixture 已经覆盖的跨行代码 span。候选二保留了过去，却仍然不知道未来；这是其最关键的洞，加锁和检查行数都补不上。**

本轮独立读取了完整 brief、`gh pr diff 178`、PR 描述与 ADR 正文，没有读取另一裁决方的结论。PR head 为 `b20bc03270c4d6622750acfa7469cc3741a54db5`，源码检出为 `0f70af8dc77be0b547e7bec8b537bdab9f04ee67`，这几处解析与修复源码相对 `origin/main` 无差异。PR 原文：[PR #178](https://github.com/Luolc/limae/pull/178)。以下实测日期为 2026-09-11。

## 一、决定性的实测反例：需要未来信息

输入分两批，每批都以 LF 结尾：

```json
["示例 `foo()\n", "bar()`。\n"]
```

| 处理方式 | 拼接后的结果 (JSON 字符串) |
| --- | --- |
| 整篇 `Pipeline::fix` | `"示例 \u0060foo()\nbar()\u0060。\n"` |
| 候选一，计数加合成起始围栏 | `"示例 \u0060foo ()\nbar () \u0060。\n"` |
| 候选二，前缀重放截本批 | `"示例 \u0060foo ()\nbar()\u0060。\n"` |

表里的 `\u0060` 是反引号，只为让 Markdown 表格不吞定界符。实验原始输出保留真实字符。

原因可以直接从源码追到：`rust/markdown.rs:140` 的 `flush` 把同一段落的行连接起来，再寻找等长反引号闭合串。第一批到达时，开启反引号尚未配对，`foo()` 作为普通文本被改成 `foo ()`。第二批到达后，整篇解析才知道第一行也是代码。候选二重算时确实恢复了前缀里的 `foo()`，但它只输出当前批，屏幕上的第一批已经收不回来。

**对照臂**：第一批完全相同，把第二批换成 `"\n结束。\n"`。空行截断段落，整篇修复此时也应当得到 `foo ()`，两个方案都与整篇一致。两种未来需要不同的第一批输出，第一批可观察到的全部输入却相同。

这给出了一个条件性的不可能性结论：在「只能替换当前批、必须及时返回、不能回改前批」的宿主合同下，任何算法都无法对全部合法输入保证逐字节等于整篇修复。磁盘状态、常驻进程、增量解析器都只能保存过去，不能创造未来。要么允许漏修歧义区段，要么改变显示协议以支持延迟提交或回改。

这也不是临时造出来才有的边角输入。对仓内 **37 个不带 `.conf`、可按 LF 分批、默认配置可成功修复的 `.in` fixture**，逐行分批实跑：

| fixture | 候选一等于整篇 | 候选二等于整篇 |
| --- | --- | --- |
| `inline-disable-next-line.in` | 否 | 是 |
| `inline-disable-range.in` | 否 | 是 |
| `span-across-blockquote-lines.in` | 否 | 否 |
| `span-across-line-break.in` | 否 | 否 |
| `span-closes-at-line-start.in` | 否 | 是 |
| `span-opens-at-line-end.in` | 否 | 否 |

其余 31 个两者都相同。这是对照整篇 pipeline 的比较，不是跑全部质量门，也没有把未覆盖的带配置 fixture 算作通过。PR 的验收第 1 条目前只排除了跨批指令 fixture，仍会被上述代码 span fixture 击穿。

## 二、两个方案各自对在哪里

### 候选一

**成立的部分**：今天 `spec/rules.md:26` 与 `rust/markdown.rs:95` 的围栏合同确实是布尔翻转：去掉前导空白，以三个反引号或三个波浪号开头即翻转。按原始完整行计算、对前缀求和取奇偶，足以恢复这一条规则的入口状态。一批多行不构成障碍，前提是遍历批内每一行。

模拟中，普通围栏的开启、正文、闭合、未闭合 EOF 都与整篇一致。拆成两半的围栏也因 partial 哨兵向后传播而保持原样。先写自己的不可变计数，再等待较小 index 的设计，较容易容纳并发和乱序；末批不立即删除目录也有实际意义。

**不成立的部分**：ADR §二 所说「段落级分组不影响结果」不能由 250 篇样本支持，现有黄金 fixture 已给出反例。把成对指令排除也不够，`disable-next-line` 同样跨批；它的语义是下一行，不是下一批。此外，禁用指令可能故意保护需要逐字展示的病句或命令，将违例称为「只多修，不会修坏代码」没有依据。

布尔不是所有 Markdown 的充分状态，但它确实是本仓**当前简化围栏规则**的充分状态。应批评的是把这一条的充分性扩大为整个 pipeline 的充分性，不能把实际存在的简化规则说成作者凭空发明的解析器。

### 候选二

**成立的部分**：重放原始前缀会自然带入围栏状态、成对禁用、`disable-next-line`、之前已经确定的段落结构。实测两种禁用都与整篇一致。原文必须保持原文；如果把上次修后的前缀再喂进去，先前误修就会污染后续上下文。

**需要撤回的理由**：brief 说 `render::prose_length` 已有能处理各种 Markdown 代码块的块解析器，这与源码不符。`rust/hook/render.rs:142` 同样只是 `in_fence` 奇偶扫描。真正修复时走的是 `rust/markdown.rs`，它有跨行 span 分组，但围栏部分仍是简化扫描，也没有一般缩进代码块豁免。

我实测了四反引号外层内放三反引号、反引号块中夹波浪号、四空格缩进代码，**整篇 pipeline 本身就会改其中的 `foo()`**。两方案与整篇相等，不能据此说代码受到正确保护。CommonMark 的围栏闭合要求同种字符且长度不短于开启，四空格缩进代码又有自己的块规则；这些是另一套目标语义。[CommonMark 0.31.2 §4.4–4.5](https://spec.commonmark.org/0.31.2/#fenced-code-blocks)

更贴近日常输入的是引用内围栏：分批 `"> ```rust\n"`、`"> foo()\n"`、`"> ```\n"`。整篇 linter 目前通过跨行等长反引号配对保护它，两个流式方案都先把第二批改成 `> foo ()`。因此，不能把跨行 span 一律当作理论上才会出现的输入。

复用 pipeline 是维护方向上的优点，**并不自动得到未来兼容性**。以后 pipeline 增加依赖后文的规则，前缀重放仍可能先修错前批；需要把可流式提交的范围作为共享解析层的契约来维护。

## 三、候选二其余边界

### 并发与乱序

按 brief 五步直接实现共享累积文件，会读到过时前缀，并按完成顺序而非 index 顺序追加。

**已做两个独立进程的受控实验**：磁盘最初有 index 0 的 `"前文\n"`。index 1 的 `"```\n"` 与 index 2 的 `"foo()\n"` 同时读到这份前缀，强制 index 2 先追加。结果：

- index 2 输出 `"foo ()\n"`，代码被改。
- 磁盘最终是 `"前文\nfoo()\n```\n"`，顺序错误。
- 同样数据按 index 更新的对照臂输出 `foo()`，与整篇一致。

这是共享文件算法的进程级实验，**没有重新测 Claude Code 本轮是否并行调 hook**。并行宿主行为的现有依据是 ADR 与 `parts.rs`、`state.rs` 标注的 2026-09-01 亲测记录，本人核到了代码承接，未独立复采宿主调度。

修法优先沿用已有 `state::keep` 的**按 index 存原始分片，临时文件写完再 rename**：每批先发布原文，然后有界等待、只读取同代消息的 `0..k`，按 index 重放。较大 index 即使已经写入，也不能算进本批前缀。它避免了一份可变累积文本的读改写事务。若坚持共享累积文件，互斥锁之外还必须有 `next_index`：加锁只保证互斥，不保证 index 顺序；更不能让晚批拿着锁等被它挡住的早批。

修复失败也要保留已收到的原始分片，否则本批失败会制造后续永远补不齐的洞。状态写不成则后续也只能放行，不能将不完整前缀当完整前缀。

### `message_id` 复用、重复投递与旧文件

`index==0` 在本进程里把 P 当空，并不能保证其他并发批读不到旧一代的缓存。按 index 存文件也有同一个问题。

合成实验里，当前代 index 0 已覆盖，index 1 尚未到但目录里有旧代 index 1，index 2 已到；文件编号连续检查完全通过。真正缺失的 index 1 应是围栏开行，旧文件却是普通文本，当前代码仍被改为 `foo ()`。**编号齐全只证明有这些文件，不证明它们属于同一次消息。**

另外，`state::identifier` 将非常规字符折叠为 `_`，只留前 64 个字符；它是路径清洗，不是无碰撞标识。不同输入可能落到同一路径。源码足以确立这个机制，不代表真实宿主已经发生过 ID 碰撞。

建议定义消息实例身份：至少带 session，若同 session 可复用 message ID，需要宿主提供各批共有的 turn / generation 身份。`turn_id`、`prompt_id` 在已知 payload 字段中，但它们能否承担这个身份，本轮未验证，不可直接当保证。字段格式若保证 UUID，可验证格式再使用；否则用完整身份的安全编码或摘要，不能靠折叠和截断建立唯一性。同 index 重送相同内容应幂等，内容冲突则停止相信这一实例，不能悄悄覆盖。

若宿主连同所有可用身份字段一起复用，又可能投递上一代迟到的 batch，单靠本地文件无法识别来源；那是协议缺少信息，不能写成「index 0 清空就解决」。

### LF、CRLF、空批与 partial

最终用于裁决的模拟调用真实 `Pipeline::fix`，保留原始字节，不经过 CLI 文件 I/O。完整 CRLF 分批的正文加围栏实测，两方案与整篇相同，CRLF 保留。

此处有一个测量陷阱：`limae --fix FILE` 先通过 `FileText::read` 将 CRLF / 裸 CR 归一化成 LF，文件被修改时写出 LF；直接 `Pipeline::fix` 不经过这层。第一版实验发现差异后已改用直接 pipeline，并重跑全部结果。hook 应复用修复 pipeline，不能拿 CLI 文件输出声称已经验过 hook 的结尾字节保留。

应将截取契约写成：**按原始 LF 字节确定边界，在修后文本里跳过 `P` 的 LF 数，保留余下全部字节**，包括 CR 与尾 LF。不用含义模糊的 `lines(delta)`。例如 `"甲\n乙\n".split('\n')` 有一个虚拟尾空串，而把 `"乙\n"` 按另一种 API 数成一行，再取前者最后一段，结果会是空串。空 delta 必须直接返回空，不允许 `[-0:]` 变成整篇。

已有 partial 补丁避免了补完一行时重复输出整条拼合行，这一方向正确。候选二在保存真实前缀后，后续重新站到 LF 边界可以恢复处理，比候选一整条消息永久失效更有恢复能力。但「当前前缀以 LF 结尾」不足以授权修改**本批末尾尚未完成的行**；非末批的 partial 尾部也应原样放行。即使所有行都完整，第一节的跨行代码反例仍在。

### 很大的消息与成本

每个前缀重放一次，总处理量是 `sum(|P_k|)`。等长批次下，即使 fixer 单次线性，总量也约为最终字节数乘批次数的一半；既不是只修最终文档一次，也不能用一次 13 ms 给整个消息背书。候选一读取此前所有计数也有随批次数累计的文件操作，不能说整数使代价为零。

本轮 release 库、默认配置、每次一个短命进程的合成测量：

| 输入 | 读数 |
| --- | --- |
| 10,800 字节、400 行，单次 3 次测量 | 中位数 32.07 ms |
| 108,000 字节、4,000 行，单次 3 次测量 | 中位数 131.98 ms |
| 12,960 字节切 120 批 | 整篇一次 42.07 ms；全部前缀逐次重放合计 4,469.84 ms |

这些包括进程启动、pipeline 初始化与 stdin/stdout；不包括 hook 的目录扫描、兄弟等待、JSON 与宿主排队。合计值是串行 CPU/墙钟工作量观察，不能直接当成用户会等 4.47 秒；宿主是否重叠执行会影响体感。与 E 的语料、机器时点和测法也不同，不能据此判 E 的 13 ms 是假的。

实现应给原始字节数、分片数量、单次等待设上限，超过就停止修复这一实例并有限期清理。不能任意截掉旧前缀继续修，那会重新丢失围栏和指令状态。目录过期回收是保留期限，不是内存、磁盘容量上限。

### 行内禁用指令

前缀重放实测能处理范围禁用和 next-line 禁用，围栏内部的同形指令仍由原 pipeline 决定，不需要 hook 再解析一遍。非法指令导致 pipeline 返回错误时，应放行并记枚举诊断，不能退回“不带指令状态的逐批修”。配置也应在一个可解释的消息实例内保持一致；若配置在消息中途改变，逐批等于某一次整篇修复本就没有唯一比较基准。

## 四、第三个方案与推荐取舍

**没有一个只换状态存储形式的第三方案，能同时保住逐批立即显示和全输入整篇等价。** 常驻 worker 解决初始化成本与本地状态，并不解决未来信息；为此引入服务生命周期目前不划算。把中间批输出空字符串、末批输出整篇，需要另测宿主是否真正抑制原文及其 timeout / 故障体验，A′ 的标记探针没有证明这个新合同，不能直接上线。

可落地的推荐是「候选二的修订版」，接受明确的漏修：

1. 用已有按 index 的原始分片缓存重建本批的完整前缀，补齐实例身份、乱序等待和资源上限；无需第二份围栏语法。
2. 保持 `Pipeline::fix` 作为规则语义正本，直接调用库，按 LF 原始边界截取当前批。
3. **只提交已经确定不会被后文改判的修改**。让共享 Markdown 层识别尚未结束的段落 / 尚未配对的反引号所影响的范围，这部分原样显示。可以先采取更保守的策略：非末批的未闭合尾块整个不修，只修已闭合的块；段落、引用块与围栏边界仍由共享层给出，不在 hook 手抄判断。
4. 已显示过的保守放行文本以后也不补发，允许与整篇存在“少修”的差异。末批可以用完整消息解析当前批，但也不能回改前批。验收重点是被整篇判为代码的内容没有被提前改写，以及普通可确定的正文仍然得到修复。

第 3 项是**设计建议，本轮没有实现或证明一个完备的稳定范围 API**。它需要在实现任务里用第一节的两种未来作对照，不能在 ADR 中提前写成已验证。若暂时不愿增加这个共享层能力，可以更粗地从发现反引号歧义起保守放行后续内容，代价是更多漏修，需明确接受。

若产品坚持完全等于整篇，则应先改变显示合同，确认宿主支持延迟提交或替换整条消息；在那之前，两份候选都不是满足要求的实现 brief。

同时把两个验收目标拆清楚：**等于当前 limae** 与 **不破坏 CommonMark 所认为的代码**。四反引号、混用围栏、缩进代码的实测表明，这两者今天并不等价。若后者也是要求，应在共享解析层另行修规范与黄金集，不能靠 hook 方案宣称已经获得。

## 五、A–F 的判据与推广边界

以下 A–F 按 brief 的编号，PR 自己的 B/C/D 编号不同。原始 58 条 payload 与数百篇真实 transcript 未提供给本轮实验，我没有重新统计它们；实测使用仓内 fixture 和合成文本。

| 读数 | 判据评价与有效范围 |
| --- | --- |
| A：52 个非末批均以 LF 结尾 | 结尾字节检查有分辨力；6 个末批的不同结果可排除“检测器恒报有 LF”。但同一读数兼容“宿主保证永远如此”与“本次样本恰好如此”，不能推出“永远”。它也不证明一批一行或调用串行。partial 哨兵需要用真正不以 LF 结尾的非末批及其后继批验收。 |
| A′：5 个非末批标记都显示 | 标记与批次位置逐一对上，对“中途替换被采纳”是有分辨力的证据。不能顺带证明空替换、回改前批、输出任意长度、并发顺序或不同宿主版本。本人未重新做屏幕探针。 |
| B′：一批含两段 | 一条真样本已足以否定“一批一行”。两方案都必须逐行处理批内部；本轮 `plain_multiple_lines` 也验证了多行批模拟。 |
| C：248/250 与整篇相同 | 字节比较有分辨力，但这组实验没有直接测试“带围栏状态后就充分”。原有段落 / 跨行代码 fixture 实测推翻了充分性；0.8% 是该样本比例，不是线上失败概率。归一化尾换行可以用于当时的内容诊断，却不能用于 hook 字节合同验收。 |
| D：677/677 行数不变 | 增行对照可以证明计数器会看到行数变化；它不证明任意输入、未来规则都保持行数，也不证明行的身份、顺序、字节结尾或过去输出稳定。第一节反例的所有臂 LF 数都不变。当前 pipeline 按 LF 拆分并逐行修后拼接，是额外的源码依据；计数本身不足以承诺截取永远正确。 |
| E：12.9 KB 一次 13 ms | 一个具体样本的单次成本读数，不是前缀重放总量或最坏耗时。PR 用 polish 的 timeout 与机械修实际耗时比较，不能得出实际速度相差三个数量级；timeout 是预算，不是实际延迟。 |
| F：未闭合 / 已闭合围栏不改，无围栏会改 | 三臂有分辨力，已独立复现，足以证明当前简单围栏形态在 EOF 仍保护代码。不能推广到尚未配对的 inline span、引用内围栏、不同长度 / 字符的嵌套情况或缩进代码。 |

因此，我没有发现 A、C、D、F 所描述的基础观测本身“全都无用”。错误主要出在**把能分辨一个问题的观察，用来回答另一个问题**：尾 LF 不等于完整上下文；行数不变不等于可在线提交；等于整篇不等于所有代码安全；观测零反例不等于宿主保证。

## 六、复跑与交接

本轮所有实验调用的是从指定源码编译的真实 release pipeline；候选一、二外围调度由 scratch 模拟，没有声称已跑尚未实现的新 hook。候选一用 ADR 明许的“合成起始围栏再剥离”实现入口状态；候选二用 LF 偏移截取，实现了 brief 的前缀 partial 放行。模拟与原案的这些对应关系使上面的反例可以复跑。

```sh
CARGO_TARGET_DIR=/tmp/limae-debate-astra-build-2026-09-11 cargo build --release --locked
rustc --edition=2024 <附录 A 的内容另存为 pipeline.rs> --extern limae=/tmp/limae-debate-astra-build-2026-09-11/release/liblimae.rlib -L dependency=/tmp/limae-debate-astra-build-2026-09-11/release/deps -O -o /tmp/limae-debate-astra-pipeline-2026-09-11
python <附录 B 的内容另存为 probe.py>
python <附录 C 的内容另存为 race.py>
```

执行位置为本 worktree；看到 build / rustc / Python 各退出 0，结果中的指定 `equal=false` 反例与有序对照 `equal=true` 即复现成功。Python 探针输出的是比较读数，反例存在是此次实验成功，不是测试器遗漏退出状态。

配套文件原本在 `~/scratch/limae/`，orchestra 归档时已按「目录布局」的规矩清理掉 scratch 副本，改为原样内联进本文件末尾的附录 (内容未改，只是换了容器)：

- [附录 A](#appendix-astra-pipeline) (`debate-fence-astra-pipeline-2026-09-11.rs`)：真实 pipeline 的 stdin/stdout 驱动。
- [附录 B](#appendix-astra-probe)、[附录 D](#appendix-astra-results) (`debate-fence-astra-probe-2026-09-11.py`、`debate-fence-astra-results-2026-09-11.jsonl`)：20 个合成用例、37 个 fixture 的比较与单次成本。
- [附录 C](#appendix-astra-race)、[附录 E](#appendix-astra-race-results) (`debate-fence-astra-race-2026-09-11.py`、`debate-fence-astra-race-results-2026-09-11.jsonl`)：独立进程共享前缀乱序实验及有序对照。
- [附录 F](#appendix-astra-cost) (`debate-fence-astra-cost-2026-09-11.json`)：120 批前缀重放成本。
- [附录 G](#appendix-astra-boundaries) (`debate-fence-astra-boundaries-2026-09-11.json`)：旧代分片混用与字符串分割边界读数。

本报告与上述附录仍等 limae-orchestra、用户消费；触发点是该设计裁决完成、证据入库或明确不采纳时，由 orchestra 逐项记录后续消费者 (内容已入库，不再有删除动作)。临时构建位于 `/tmp/limae-debate-astra-build-2026-09-11`，不占仓库目录，且随裁决时的临时 worktree 一起已不存在。仓库文件在裁决当时未修改，未 commit、push、开 PR，也未向 PR 发布审查评论；本次归档是后续 PR 的入库动作，不是裁决本身的一部分。

## 附录：复跑用旁支文件 (归档时由 orchestra 内联，内容未改)

以下 7 个文件是裁决方在 `~/scratch/limae/` 下留的复跑脚本与数据，随报告一起原样内联，只是从独立文件换成了本文件内的围栏代码块；每个文件内部的内容 (含其自带的归属注释) 未作任何改动。

<a id="appendix-astra-pipeline"></a>
### `debate-fence-astra-pipeline-2026-09-11.rs`

```rust
// 归属：debate-fence，关联 PR #178，消费者 limae-orchestra；裁决消费后清理。
use std::io::{self, Read, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let p = limae::pipeline::Pipeline::new()?;
    let fixed = p.fix(&input, &limae::config::ResolvedConfig::default())?;
    io::stdout().write_all(fixed.as_bytes())?;
    Ok(())
}
```

<a id="appendix-astra-probe"></a>
### `debate-fence-astra-probe-2026-09-11.py`

```python
# 归属：debate-fence，关联 PR #178，消费者 limae-orchestra；合成实验，裁决消费后清理。
import json, os, pathlib, subprocess, tempfile, time, statistics
BIN = '/tmp/limae-debate-astra-pipeline-2026-09-11'
ROOT = pathlib.Path('/home/ubuntu/wt/limae/debate-a')

def emit(**kw):
    print(json.dumps(kw, ensure_ascii=False), flush=True)

with tempfile.TemporaryDirectory(prefix='limae-astra-probe-2026-09-11-') as tmp:
    file = pathlib.Path(tmp) / 'case.md'
    def fix(s):
        proc = subprocess.run([BIN], input=s.encode(), cwd=tmp, capture_output=True)
        if proc.returncode != 0:
            raise RuntimeError((proc.returncode, proc.stderr.decode()))
        return proc.stdout.decode()
    def slice_at_line(s, n):
        pos = 0
        for _ in range(n):
            pos = s.index('\n', pos) + 1
        return s[pos:]
    def fence_count(s):
        return sum(line.lstrip().startswith(('```','~~~')) for line in s.split('\n'))
    def stream1(parts):
        parity = 0
        invalid = False
        out = []
        for i,d in enumerate(parts):
            invalid |= i < len(parts)-1 and not d.endswith('\n')
            if invalid:
                out.append(d)
            else:
                v = fix(('```\n' if parity else '') + d)
                out.append(v[4:] if parity else v)
            parity ^= fence_count(d) % 2
        return ''.join(out)
    def stream2(parts):
        p = ''
        out = []
        for d in parts:
            if p and not p.endswith('\n'):
                v = d
            else:
                t = p + d
                f = fix(t)
                v = d if f.count('\n') != t.count('\n') else slice_at_line(f,p.count('\n'))
            out.append(v)
            p += d
        return ''.join(out)
    emit(task='debate-fence', pr=178, consumer='limae-orchestra', engine='Pipeline::fix default config', head='0f70af8dc77be0b547e7bec8b537bdab9f04ee67')
    cases = {
      'plain_multiple_lines': ['甲A。\n\n乙B。\n\n','末C。'],
      'fence_unclosed': ['```rust\n','fn main() {\n','    foo();\n'],
      'fence_closed': ['```rust\n','fn main() {\n','    foo();\n','```\n','中A\n'],
      'unfenced_control': ['fn main() {\n','    foo();\n'],
      'inline_future_close': ['示例 `foo()\n','bar()`。\n'],
      'inline_blank_control': ['示例 `foo()\n','\n结束。\n'],
      'inline_middle': ['示例 `\n','foo()\n','bar()`。\n'],
      'directive_range': ['<!-- limae-disable -->\n','中A\n','<!-- limae-enable -->\n','中B\n'],
      'directive_next': ['<!-- limae-disable-next-line -->\n','中A\n','中B\n'],
      'crlf': ['甲A\r\n\r\n','```\r\n','foo()\r\n','```\r\n','乙B\r\n'],
      'split_crlf': ['甲A\r','\n','乙B\r\n'],
      'partial_prose': ['中','A\n','乙B\n'],
      'split_fence': ['``','`\n','foo()\n'],
      'empty_final': ['甲A\n\n',''],
      'blank_delta': ['甲A\n','\n','乙B'],
      'nested_fence_long': ['````markdown\n','```rust\n','foo()\n','```\n','````\n'],
      'mixed_fences': ['```rust\n','~~~\n','foo()\n','```\n'],
      'indented_code': ['    foo()\n','    中A\n'],
      'quote_fence': ['> ```rust\n','> foo()\n','> ```\n'],
      'future_pair_reclassifies_old_pair': ['`` unpaired `foo()`\n','bar() ``\n'],
    }
    for name, parts in cases.items():
        raw = ''.join(parts)
        whole = fix(raw)
        a,b = stream1(parts),stream2(parts)
        emit(case=name,parts=parts,whole=whole,candidate1=a,candidate2=b,equal1=a==whole,equal2=b==whole,lf_before=raw.count('\n'),lf_after=whole.count('\n'))
    mismatches = []
    total = 0
    for src in sorted((ROOT/'spec/fixtures').glob('*.in')):
        if src.with_suffix('.conf').exists():
            continue
        s = src.read_bytes().decode()
        parts = s.splitlines(keepends=True)
        if ''.join(parts) != s or any(not p.endswith('\n') for p in parts[:-1]):
            continue
        try:
            whole = fix(s)
            a,b = stream1(parts),stream2(parts)
        except RuntimeError:
            continue
        total += 1
        if a != whole or b != whole:
            mismatches.append({'fixture':src.name,'candidate1_equal':a==whole,'candidate2_equal':b==whole})
    emit(fixture_total=total,mismatches=mismatches)
    for n in (400,4000):
        raw = '这是中文A,正文(x)。\n' * n
        times=[]
        for _ in range(3):
            t=time.perf_counter(); fix(raw); times.append((time.perf_counter()-t)*1000)
        emit(benchmark_bytes=len(raw.encode()),lines=n,median_ms=round(statistics.median(times),2),all_ms=times)
```

<a id="appendix-astra-race"></a>
### `debate-fence-astra-race-2026-09-11.py`

```python
# 归属：debate-fence，关联 PR #178，消费者 limae-orchestra；合成并发实验，裁决消费后清理。
import json, multiprocessing as mp, pathlib, subprocess, tempfile
BIN='/tmp/limae-debate-astra-pipeline-2026-09-11'
def fix(s):
    p=subprocess.run([BIN], input=s.encode(), capture_output=True, check=True)
    return p.stdout.decode()
def worker(i,delta,state,barrier,committed,out):
    prefix=pathlib.Path(state).read_text()
    barrier.wait()
    fixed=fix(prefix+delta)
    suffix=fixed.split('\n',prefix.count('\n'))[-1]
    if i==1:
        committed.wait()
    with open(state,'a') as f:
        f.write(delta)
    if i==2:
        committed.set()
    out.put((i,prefix,suffix))
if __name__=='__main__':
    with tempfile.TemporaryDirectory(prefix='limae-astra-race-2026-09-11-') as tmp:
        state=pathlib.Path(tmp)/'prefix'
        state.write_text('前文\n')
        barrier=mp.Barrier(2); event=mp.Event(); out=mp.Queue()
        jobs=[mp.Process(target=worker,args=(i,d,str(state),barrier,event,out)) for i,d in [(1,'```\n'),(2,'foo()\n')]]
        for p in jobs:p.start()
        for p in jobs:
            p.join(10)
            assert p.exitcode==0,p.exitcode
        result=sorted([out.get(timeout=1),out.get(timeout=1)])
        expected=fix('前文\n```\nfoo()\n')
        actual='前文\n'+''.join(v for _,_,v in result)
        print(json.dumps(dict(task='debate-fence',pr=178,consumer='limae-orchestra',mode='controlled independent processes, not live host',results=result,disk=state.read_text(),whole=expected,stream=actual,equal=actual==expected),ensure_ascii=False))
        prefix='前文\n'; serial='前文\n'
        for d in ['```\n','foo()\n']:
            v=fix(prefix+d).split('\n',prefix.count('\n'))[-1]
            serial+=v; prefix+=d
        print(json.dumps(dict(control='ordered prefix updates',whole=expected,stream=serial,equal=serial==expected),ensure_ascii=False))
```

<a id="appendix-astra-results"></a>
### `debate-fence-astra-results-2026-09-11.jsonl`

```jsonl
{"task": "debate-fence", "pr": 178, "consumer": "limae-orchestra", "engine": "Pipeline::fix default config", "head": "0f70af8dc77be0b547e7bec8b537bdab9f04ee67"}
{"case": "plain_multiple_lines", "parts": ["甲A。\n\n乙B。\n\n", "末C。"], "whole": "甲 A。\n\n乙 B。\n\n末 C。", "candidate1": "甲 A。\n\n乙 B。\n\n末 C。", "candidate2": "甲 A。\n\n乙 B。\n\n末 C。", "equal1": true, "equal2": true, "lf_before": 4, "lf_after": 4}
{"case": "fence_unclosed", "parts": ["```rust\n", "fn main() {\n", "    foo();\n"], "whole": "```rust\nfn main() {\n    foo();\n", "candidate1": "```rust\nfn main() {\n    foo();\n", "candidate2": "```rust\nfn main() {\n    foo();\n", "equal1": true, "equal2": true, "lf_before": 3, "lf_after": 3}
{"case": "fence_closed", "parts": ["```rust\n", "fn main() {\n", "    foo();\n", "```\n", "中A\n"], "whole": "```rust\nfn main() {\n    foo();\n```\n中 A\n", "candidate1": "```rust\nfn main() {\n    foo();\n```\n中 A\n", "candidate2": "```rust\nfn main() {\n    foo();\n```\n中 A\n", "equal1": true, "equal2": true, "lf_before": 5, "lf_after": 5}
{"case": "unfenced_control", "parts": ["fn main() {\n", "    foo();\n"], "whole": "fn main () {\n    foo ();\n", "candidate1": "fn main () {\n    foo ();\n", "candidate2": "fn main () {\n    foo ();\n", "equal1": true, "equal2": true, "lf_before": 2, "lf_after": 2}
{"case": "inline_future_close", "parts": ["示例 `foo()\n", "bar()`。\n"], "whole": "示例 `foo()\nbar()`。\n", "candidate1": "示例 `foo ()\nbar () `。\n", "candidate2": "示例 `foo ()\nbar()`。\n", "equal1": false, "equal2": false, "lf_before": 2, "lf_after": 2}
{"case": "inline_blank_control", "parts": ["示例 `foo()\n", "\n结束。\n"], "whole": "示例 `foo ()\n\n结束。\n", "candidate1": "示例 `foo ()\n\n结束。\n", "candidate2": "示例 `foo ()\n\n结束。\n", "equal1": true, "equal2": true, "lf_before": 3, "lf_after": 3}
{"case": "inline_middle", "parts": ["示例 `\n", "foo()\n", "bar()`。\n"], "whole": "示例 `\nfoo()\nbar()`。\n", "candidate1": "示例 `\nfoo ()\nbar () `。\n", "candidate2": "示例 `\nfoo ()\nbar()`。\n", "equal1": false, "equal2": false, "lf_before": 3, "lf_after": 3}
{"case": "directive_range", "parts": ["<!-- limae-disable -->\n", "中A\n", "<!-- limae-enable -->\n", "中B\n"], "whole": "<!-- limae-disable -->\n中A\n<!-- limae-enable -->\n中 B\n", "candidate1": "<!-- limae-disable -->\n中 A\n<!-- limae-enable -->\n中 B\n", "candidate2": "<!-- limae-disable -->\n中A\n<!-- limae-enable -->\n中 B\n", "equal1": false, "equal2": true, "lf_before": 4, "lf_after": 4}
{"case": "directive_next", "parts": ["<!-- limae-disable-next-line -->\n", "中A\n", "中B\n"], "whole": "<!-- limae-disable-next-line -->\n中A\n中 B\n", "candidate1": "<!-- limae-disable-next-line -->\n中 A\n中 B\n", "candidate2": "<!-- limae-disable-next-line -->\n中A\n中 B\n", "equal1": false, "equal2": true, "lf_before": 3, "lf_after": 3}
{"case": "crlf", "parts": ["甲A\r\n\r\n", "```\r\n", "foo()\r\n", "```\r\n", "乙B\r\n"], "whole": "甲 A\r\n\r\n```\r\nfoo()\r\n```\r\n乙 B\r\n", "candidate1": "甲 A\r\n\r\n```\r\nfoo()\r\n```\r\n乙 B\r\n", "candidate2": "甲 A\r\n\r\n```\r\nfoo()\r\n```\r\n乙 B\r\n", "equal1": true, "equal2": true, "lf_before": 6, "lf_after": 6}
{"case": "split_crlf", "parts": ["甲A\r", "\n", "乙B\r\n"], "whole": "甲 A\r\n乙 B\r\n", "candidate1": "甲A\r\n乙B\r\n", "candidate2": "甲 A\r\n乙 B\r\n", "equal1": false, "equal2": true, "lf_before": 2, "lf_after": 2}
{"case": "partial_prose", "parts": ["中", "A\n", "乙B\n"], "whole": "中 A\n乙 B\n", "candidate1": "中A\n乙B\n", "candidate2": "中A\n乙 B\n", "equal1": false, "equal2": false, "lf_before": 2, "lf_after": 2}
{"case": "split_fence", "parts": ["``", "`\n", "foo()\n"], "whole": "```\nfoo()\n", "candidate1": "```\nfoo()\n", "candidate2": "```\nfoo()\n", "equal1": true, "equal2": true, "lf_before": 2, "lf_after": 2}
{"case": "empty_final", "parts": ["甲A\n\n", ""], "whole": "甲 A\n\n", "candidate1": "甲 A\n\n", "candidate2": "甲 A\n\n", "equal1": true, "equal2": true, "lf_before": 2, "lf_after": 2}
{"case": "blank_delta", "parts": ["甲A\n", "\n", "乙B"], "whole": "甲 A\n\n乙 B", "candidate1": "甲 A\n\n乙 B", "candidate2": "甲 A\n\n乙 B", "equal1": true, "equal2": true, "lf_before": 2, "lf_after": 2}
{"case": "nested_fence_long", "parts": ["````markdown\n", "```rust\n", "foo()\n", "```\n", "````\n"], "whole": "````markdown\n```rust\nfoo ()\n```\n````\n", "candidate1": "````markdown\n```rust\nfoo ()\n```\n````\n", "candidate2": "````markdown\n```rust\nfoo ()\n```\n````\n", "equal1": true, "equal2": true, "lf_before": 5, "lf_after": 5}
{"case": "mixed_fences", "parts": ["```rust\n", "~~~\n", "foo()\n", "```\n"], "whole": "```rust\n~~~\nfoo ()\n```\n", "candidate1": "```rust\n~~~\nfoo ()\n```\n", "candidate2": "```rust\n~~~\nfoo ()\n```\n", "equal1": true, "equal2": true, "lf_before": 4, "lf_after": 4}
{"case": "indented_code", "parts": ["    foo()\n", "    中A\n"], "whole": "    foo ()\n    中 A\n", "candidate1": "    foo ()\n    中 A\n", "candidate2": "    foo ()\n    中 A\n", "equal1": true, "equal2": true, "lf_before": 2, "lf_after": 2}
{"case": "quote_fence", "parts": ["> ```rust\n", "> foo()\n", "> ```\n"], "whole": "> ```rust\n> foo()\n> ```\n", "candidate1": "> ```rust\n> foo ()\n> ```\n", "candidate2": "> ```rust\n> foo ()\n> ```\n", "equal1": false, "equal2": false, "lf_before": 3, "lf_after": 3}
{"case": "future_pair_reclassifies_old_pair", "parts": ["`` unpaired `foo()`\n", "bar() ``\n"], "whole": "`` unpaired `foo()`\nbar() ``\n", "candidate1": "`` unpaired `foo()`\nbar () ``\n", "candidate2": "`` unpaired `foo()`\nbar() ``\n", "equal1": false, "equal2": true, "lf_before": 2, "lf_after": 2}
{"fixture_total": 37, "mismatches": [{"fixture": "inline-disable-next-line.in", "candidate1_equal": false, "candidate2_equal": true}, {"fixture": "inline-disable-range.in", "candidate1_equal": false, "candidate2_equal": true}, {"fixture": "span-across-blockquote-lines.in", "candidate1_equal": false, "candidate2_equal": false}, {"fixture": "span-across-line-break.in", "candidate1_equal": false, "candidate2_equal": false}, {"fixture": "span-closes-at-line-start.in", "candidate1_equal": false, "candidate2_equal": true}, {"fixture": "span-opens-at-line-end.in", "candidate1_equal": false, "candidate2_equal": false}]}
{"benchmark_bytes": 10800, "lines": 400, "median_ms": 32.07, "all_ms": [32.900107093155384, 32.066744985058904, 31.86706092674285]}
{"benchmark_bytes": 108000, "lines": 4000, "median_ms": 131.98, "all_ms": [131.97949004825205, 158.98602490779012, 124.43548103328794]}
```

<a id="appendix-astra-race-results"></a>
### `debate-fence-astra-race-results-2026-09-11.jsonl`

```jsonl
{"task": "debate-fence", "pr": 178, "consumer": "limae-orchestra", "mode": "controlled independent processes, not live host", "results": [[1, "前文\n", "```\n"], [2, "前文\n", "foo ()\n"]], "disk": "前文\nfoo()\n```\n", "whole": "前文\n```\nfoo()\n", "stream": "前文\n```\nfoo ()\n", "equal": false}
{"control": "ordered prefix updates", "whole": "前文\n```\nfoo()\n", "stream": "前文\n```\nfoo()\n", "equal": true}
```

<a id="appendix-astra-cost"></a>
### `debate-fence-astra-cost-2026-09-11.json`

```json
{"task": "debate-fence", "pr": 178, "consumer": "limae-orchestra", "bytes": 12960, "batches": 120, "once_ms": 42.07, "replay_total_ms": 4469.84, "mode": "one standalone process per fix, no hook disk/host overhead"}
```

<a id="appendix-astra-boundaries"></a>
### `debate-fence-astra-boundaries-2026-09-11.json`

```json
{"task": "debate-fence", "pr": 178, "consumer": "limae-orchestra", "mode": "synthetic stale generation", "contiguous": true, "expected_current": "foo()", "replay_current": "foo ()", "fold_collision": true, "empty_suffix_bug": ""}
```
