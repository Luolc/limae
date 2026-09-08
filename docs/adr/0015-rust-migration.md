# ADR-0015：Rust 分阶段迁移，Python 保留参考实现

- 日期：2026-09-06。
- 状态：proposed；用户已授权 agent 互审并自动推进至 Rust 发版前，实施由 orchestra 逐项派发；实际 tag / release / publish 前停止。
- 承接：[ADR-0001](0001-standalone-repo-spec-first-shared-fixtures.md)、[ADR-0002](0002-rule-flags-and-rust.md) 的 Rust 方向与 [ADR-0005](0005-agent-native-positioning.md) 的产品边界。

## 背景

本次基线是 `5640370c84745eeb365b91ecd25a91dcf63c443f`，下文的实现观察与测试盘点均截至 2026-09-06。已读取 ADR-0001 至 ADR-0014、[规则规范](../../spec/rules.md)、[fixture 合同](../../spec/README.md) 及全部 49 组共享 fixture。它们覆盖规则文本行为，覆盖不了完整 CLI、配置发现、文件系统和宿主协议。

Python 已实现 21 条确定性规则，包含实验规则；入口仍是 `limae [--fix] FILE...` / `--all`、`limae polish -` 和 `limae hook`。`check` / `format` 子命令、文件 polish 与结构保护器是 [ADR-0008](0008-limae-polish-cli.md) 的后续方向，不能算作已存在的兼容面。规则与配置的现行正本是 `spec/`，旧 ADR 中的旧 id / 名字按 [ADR-0010](0010-rule-id-naming.md)、[ADR-0013](0013-drop-rename-transition-aliases.md) 理解。

## 决定

### 一、推荐范围与终态

推荐第一个里程碑交付**全部现有确定性 lint / fix 的可运行 Rust CLI**：含配置、忽略、行内指令、实验规则及其词表，先在开发工作树显式调用。实验规则仍默认关闭，但不能将「关闭时没报错」当成移植完成。分发、polish、两种宿主 hook 依次验收；Python 在整个迁移期保留参考实现 (reference implementation)，其测试继续运行。

| 阶段 | 实际交付路径与退出判据 |
| --- | --- |
| A：确定性内核与 CLI，首个里程碑 | `limae-rs [--fix] FILE...` / `--all` 可处理真实 Markdown；现有 21 条规则、49 组 fixture、下文 CLI 边界与差分全部通过，未裁决差异为零 |
| B：独立分发与并存 | 在没有 Python、没有源码 checkout 的环境运行单个 binary，实验词表可用；源码包经 Cargo 安装后同样可运行，提供独立 Rust pre-commit opt-in，并核实既有 Python hook 仍可安装运行 |
| C：polish | `limae-rs polish -` 真正执行配置的外部命令，预设探测与 custom 都可用；进程树、输出上限、失败诊断通过受控测试，合成正文完成真实引擎 smoke 验收 |
| D：Claude / Codex hook | `limae-rs hook` 的两种宿主输入输出及会话态记录可用；受控事件回放与两个宿主的实际挂载对照通过，原文、润色显示和失败放行符合既有合同 |
| E：主入口切换与发版准备 | A–D 全部通过后，agent 互审并自动完成仓内 / 本地 CLI 切换为 Rust `limae`，Python 标记 deprecated 并保留 reference / 测试；验证回退，再汇报 crates.io 发布缺口，实际 tag / release / `cargo publish` 前停止 |

过渡期 crate / package 仍叫 `limae`，唯一生产 binary 叫 `limae-rs`，先设 `publish = false`；E 完成时 binary 改回 [ADR-0008 §一](0008-limae-polish-cli.md#一更名-limae) 的 `limae` 终态。A–D 不安装名为 `limae` 的 Rust 文件，不调用 Python 代跑未实现的规则，也不接管现有入口或 Claude / Codex hook 配置。A / B 中未实现的 `polish` / `hook` 调用明确报「未提供此子命令」并退出 2。保留现有裸调用形式，`check` / `format` 的产品入口改造另开任务。

E 将 Python 的开发 / reference 命令显式命名为 `limae-python`，避免同名入口冲突；文档和包说明标记 deprecated，参考测试继续运行，不通过 stdout 弃用横幅破坏协议。仓内默认 hook / dogfood 转向已验收的 Rust；本机全局 CLI 安装与受管 hook 切换由 orchestra 经 machine-setup PR / 既有安装流程协调，不手改家目录。外部消费仓只做必要调查与准备，不把整套跨项目迁移列为本任务前置条件，也不等待人工切换窗口。

本次不做 `contextual`、`quote_style`、新规则、语言探测、LSP、SDK、Python FFI、模型默认调优、文档级 lint、文件 / 多文件 polish。它们均不是 Rust 迁移前置条件；进度只记 [tracker](../tracker.md)，本文任务编号只用于依赖和验收，不维护完成状态。

### 二、一个 package，显式源码路径

采用单 package、一个 library target 加一个薄 binary target；不建多 crate workspace。库承担规则与可测试的业务边界，binary 只组装配置 / 文件 / 输出。没有第二个需要独立发布或依赖隔离的组件时，workspace 只会增加 manifests、可见性和 lint 继承成本。

```text
Cargo.toml                 # 根项目；[lib] / [[bin]] / [[test]] 显式 path
Cargo.lock                 # CLI 应用提交锁文件，与 uv.lock 分立
rust-toolchain.toml        # CI / 本地工具链 pin，A1 引入
rust/lib.rs                # 按实现进度接入模块，不预造空壳
rust/main.rs               # bin = limae-rs
rust/text.rs               # 字符边界、行视图、Finding / RuleId
rust/markdown.rs            # 保护范围
rust/rules/{mod,typography,tells,words}.rs
rust/{config,directives,files,cli,resources}.rs
rust/tests/integration.rs   # 唯一 integration test target，mod golden 等
rust/tests/golden.rs        # 上述 target 的普通模块，不另注册 target
rust/examples/diff_probe.rs # A8b 的 dev-only example：diff-probe
spec/                      # 所有实现的同一份规则、fixture、词表、prompt
src/limae/ + tests/         # Python reference，位置不变
```

`Cargo.toml` 放仓根，源码不挤进 Python 的 `src/limae/`，Rust 测试不混进 Python 的 `tests/`。C / D 到来时才增加 `rust/polish/`、`rust/hook/`。先用同步函数、`&str` / `Path` 和有界的逐文件处理，不为性能推测引入 Tokio、线程池或通用插件注册框架。

### 三、共享资源与必要依赖

推荐 Rust 用 `include_str!` 从根 `spec/wordlists/` **构建时嵌入**，C 再嵌入 `spec/polish/`。源码数据仍只有 `spec/` 一处，`resources.rs` 只负责引用、解析，不手抄词条或 prompt。库初始化对嵌入数据作类型校验，失败返回具名错误，不能退回空词表；文件缺席则构建失败。[include_str! 的官方合同](https://doc.rust-lang.org/std/macro.include_str.html) 支持在编译时将相对源文件路径的 UTF-8 文件包含为字符串。

两种方式的实际用途不同：运行时文件适合用户在不重建程序时修改词表；构建时嵌入适合随版本固定词表、只安装一个 binary。[ADR-0007 §二](0007-ai-tells-rule-family-and-wordlists.md#二词表放-specwordlists是规范的一部分) 没有开放用户词表增删接口，本次也未发现必须运行时编辑的已授权需求。为了这项尚无需求的能力引入资源目录发现、路径配置、资源安装与失配处理，成本不成立；故不增加 `--data-dir`、share 目录或启动解压流程。

本 ADR 将 [spec/README.md](../../spec/README.md#词表) 的「各实现在运行时从这些文件读词表」对 Rust 的要求改为「各实现从同一份 `spec/` 获取数据；Rust 构建时读取，Python 运行时读取」。A4 在引入嵌入的同一 PR 同步该说明；Python 的 `importlib.resources` / wheel 路径不变，规则匹配语义不变。修改词表仍只改 `spec/`，Rust 重新构建发行，不需改规则逻辑。

根 Cargo package 的 `include` 纳入 `rust/`、所需 `spec/`、README；根布局使资源在包内，验收 `cargo package --list`、解包构建和安装。[Cargo include 文档](https://doc.rust-lang.org/cargo/reference/manifest.html#the-exclude-and-include-fields) 只保证源码包内容；[cargo install 文档](https://doc.rust-lang.org/cargo/commands/cargo-install.html) 说明 executable 安装到 `bin`。所以 B 的验收要搬走安装后的 binary，在无源码、无 Python、无附属资源的 cwd 真运行实验规则，不能只看包清单。默认先支持源码 / Git 的 Cargo 安装与预编译 binary；本轮不发布或占名。E 的发布前清单核对名字、发布配置和许可证：2026-09-06 本仓根目录无 license 文件、`pyproject.toml` 无 license 声明，先列缺口，不自行选择许可证，也不阻塞本地实现。PyPI / npm / Homebrew 新分发面另案。

| 依赖 | 引入点与理由 |
| --- | --- |
| [regex](https://docs.rs/regex/latest/regex/) | Markdown / 句式匹配；标准库无正则。只保留容易表达的模式，look-around 改显式邻接判定，不增加 PCRE / fancy-regex |
| [toml](https://docs.rs/toml/latest/toml/) | A5 的配置解析首次引入并独占 manifest / lock；A4 术语表与后续 fixture `.conf` 复用。先用 `Value` 校验已知键，不增加配置框架 |
| [ignore 的 GitignoreBuilder](https://docs.rs/ignore/latest/ignore/gitignore/struct.GitignoreBuilder.html) | 仅解析找到的 `.limae-ignore`；不启用文件 walker 的隐含 `.gitignore` / 全局排除。`add` 可能返回部分错误，必须处理；与 pathspec 的匹配差异由 A 的测试裁定 |
| [clap](https://docs.rs/clap/latest/clap/) | 接入 CLI 时解析重复 flag、`--`、路径和用法错误，避免另写通用参数解析器；错误文案布局不要求复制 argparse |
| [thiserror](https://docs.rs/thiserror/latest/thiserror/) | 库的领域错误枚举保留类别与 source；顶层不需要额外 anyhow 时不加 |

上述官方文档获取于 2026-09-06，只支持列出的 API / 语义，**不证明与 Python 等价**。依赖在首次使用的 PR 引入，锁文件由单一任务持有；A8b 的 JSON 适配仅加 dev-dependency，C / D 的随机采样、安全 OS 封装等有实际调用点再选，不预装依赖清单。

### 四、兼容风险与实现约束

| 面 | 基线观察 → Rust 设计与验收重点 |
| --- | --- |
| look-around / 非重叠匹配 | [zh_format.py](../../src/limae/zh_format.py) 的 CJK 边界、英文括号、单位、破折号、URL 和英文词界大量使用 look-around。Rust regex 不支持它。`用A表示` 有两个相邻的零宽命中，不能改成消费两字符后直接 `find_iter`；邻接扫描同时保留原 matcher 的非重叠约束，如标点规则不能擅自改为逐标点计数 |
| Unicode 与坐标 | Python `re` / 字符切片按码点；Rust regex 给字节偏移，[char_indices](https://doc.rust-lang.org/std/primitive.str.html#method.char_indices) 可建立 UTF-8 边界。内部 span 用字节范围，邻接与 20 / 40 字符窗口、snippet 前后 12 字符用 Unicode scalar 计数；不用 byte 长度替代字数，不加 grapheme 切分。CJK / ASCII 类照 spec 的范围，不能换成整个 Han / `\w` |
| 大小写、空白与行 | Python 英文词表使用 `re.IGNORECASE`，术语锚点用 `str.lower()`；二者不能统一成 ASCII lowercase。默认分别复刻 check 的 `splitlines()` 与 fix 的 `split("\n")`，保留两个行视图及 CLI 的换行读取边界；不先改成单一 `str::lines()`。空白判定沿用 Python `isspace()` / `re` 基线，覆盖 U+001C–U+001F，不直接换成 Rust 的 Unicode White_Space。两者均作显式差分；确有规范冲突时由 agents 在 PR 提交证据、裁定并同步规范，不以忽略差异代替兼容 |
| Markdown 保护 | `_protected` 是简化的块扫描，围栏遇两种前缀任一种就切换；行内代码允许同段跨行，块边界、连续引用和等长定界符决定 span。保留行首 / 行尾零长 span，定界符仍属正文。`_prose_spans` 处理假名引用、嵌套、代码交错、链接 destination 与 URL 尾标点；不以 CommonMark parser 的更完整语义替换本仓规则 |
| 保护与修复 | 消费匹配有任意 overlap 即豁免；零宽插空格要看两侧，区间端点也可能被豁免。每轮先算保护 / 指令，再按 [规范的修复顺序](../../spec/rules.md#修复顺序) 修正文片段，重新扫描直到不动点；不能把原文的所有 edit 一次反向应用。Python 的代码定界符空格在片段末处理，Rust 按规范阶段实现并对照交互；不复制其 `\x00` 临时替换技巧 |
| findings / 词表 | 检查只看原文，排序为行、显式规则次序、匹配位置，同键保持 pattern 顺序；不能用字符串排序把 `-10` 排到 `-2` 前，也不能 set 去重。词表「每行一处」和句式 / 用词「逐处报」分别保留；术语锚点和 allowlist 在整行找证据，违规本身才受保护范围约束 |
| 配置与忽略 | [config.py](../../src/limae/config.py) 从 cwd 向上，`.git` 文件与目录都终止；两种载体就近整份取胜。CLI enable / disable 连空字符串参数也须按「flag 出现」处理，跳过配置读取；坏 pyproject 不能越过。未知的未实现键当前不读取，不能加 `deny_unknown_fields` 误拒 polish / quote_style。忽略发现独立，路径 resolve 后相对 ignore 根匹配，根外文件保留，输入顺序不变 |
| CLI / 文件选择 | [main / tracked_markdown](../../src/limae/zh_format.py) 的裸命令、flag、0 / 1 / 2、warning、修后重新检查、无输入与全部被忽略的区别都要测。`--all` 当前调用 `git ls-files '*.md'`，不是递归找所有 Markdown；保留 cwd 范围及显式文件顺序。含中文 / 空格 / 换行的 Git 路径需要单独对照，不能把改成 `-z` 顺手记成已兼容 |
| 文件写回 | `_fix_in_place` 先完整读 / 算，仅文本变动才 `write_text`，之后重读检查；无改动保留 mtime / 字节，保留末尾有没有换行。Linux 文本读取归一 CRLF / CR，实际改写会输出 LF；写入跟随 symlink，保留目标 inode / mode，hardlink 别名共享改动。批次不是事务，后文件错误不撤销前文件。A 默认保持这些成功语义，写入 / flush 失败具名报错；不宣称原地截断能抗崩溃，也不在迁移中偷偷换 rename 破坏链接语义。原子写回的链接策略应另案同时更新两实现与文件合同 |

合成输入本机复核 (2026-09-06，调用该 SHA 的 `check_text` / `fix_text` / `main`)：

| 输入 / 场景 | Python 观察 | 对照要分辨什么 |
| --- | --- | --- |
| `用A表示`；前置 emoji、附加组合音符的变体 | 前者两处 `zh-typography-4`；变体安全切片 | 消费匹配漏第二个边界、UTF-8 切片越界 |
| `pİvotal`，开启实验规则 | 命中 `en-tell-1` | Unicode 大小写差异 |
| `中A\u2028文B` (实际 U+2028) | check 两个行号；fix 保留分隔符 | 两种分行 API 的差异 |
| ` ``` ` 开围栏、`~~~` 收尾后的违规行 | 后一行报违规 | 更换 Markdown parser 带来的语义漂移 |
| 干净 CRLF / 有违规 CRLF / 无末尾 LF | 干净文件字节与 mtime 不变；有改动写 LF；不补末尾 LF | 一律重写、统一补换行 |
| 第二文件含未知指令；另测 symlink 输入 | 返回 2 时第一文件已改；symlink 保留、目标 inode 不变 | 假设全批原子、rename 替换 symlink |

这些读数是迁移线索，不在本 ADR 新立一份规则规范。常见路径默认按 Python 兼容；发现 Python 与 spec 不一致时，先提交独立的规范 / reference 修正并重定基线。不得通过忽略差异列表把结论改成「全部兼容」。

### 五、测试迁移映射与差分验收

Rust 使用 Cargo 内建测试框架：模块内 unit tests 覆盖局部业务边界，`rust/tests/integration.rs` 接入共享 golden 等集成模块，library 文档示例用 doctest。A1 即给首个可用 API 一条可执行文档示例，并从 `Doc-tests` 结果确认执行。[Cargo test 文档](https://doc.rust-lang.org/cargo/commands/cargo-test.html#target-selection) 的默认集合含 unit / integration / doctest，`--all-targets` 的显式集合不含 doctest，所以全量门使用默认 `cargo test --locked`。缓存可用于减少重复构建，不引入 nextest 或额外测试框架。

| Python 来源 | 已有业务边界 → Rust 验收归属 |
| --- | --- |
| [test_fixtures.py](../../tests/test_fixtures.py) | 49 组共享数据：fix 等于期望、fix 幂等、原文 findings 精确有序；`rust/tests/golden.rs` 薄 runner，仍只做这三项，不由实现生成 `.fixed` / `.findings` |
| [test_zh_format.py](../../tests/test_zh_format.py)：CLI / grading | 报告后修复、disable / enable、warning 不使运行失败但仍修复、experimental 总开关与逐条 enable 报错；现有 pytest 手写期望保留一份，参数化执行两个 CLI，补非 fixable 升为 error 后 `--fix` 仍退出 1 |
| 同文件：配置 / 指令 / 忽略 | 两载体、CLI 整份覆盖、非法 TOML / 键值 / id、冲突、单位、未知指令、显式文件忽略、上层发现和否定模式；现有测试实际进程内调用 `main()`，A8b 改为统一 subprocess run fixture，Python 臂也执行 `.venv/bin/limae`，两臂显式设置等价 cwd / env 并捕获输出，复用同一份手写期望。补目录 / `.git` / ignore 发现、`--all`、全部忽略、读写失败与上表字节 / 链接行为；Rust 原生测试只补库业务与语言边界 |
| 同文件：packaged wordlists | 现有测试只从当前安装读资源；B 增加「仅 binary、全新 cwd」的真实运行，包含 experimental 命中与术语 fix。包内缺资源臂应构建失败，完整包臂应安装成功且真命中；不把 checkout 内的 symlink 测试当成发行证明 |
| [test_polish.py](../../tests/test_polish.py) → C | 预设 / custom argv 与 stdin、随机边界串、宿主排序、只有 binary 缺席硬排除、探活 marker、成功 / 失败 TTL 与真实失败更新、逐引擎脱敏诊断、cwd / env 隔离、prompt 分层、CLI / env / 配置优先级、空输入 / 空答复 / 错误退出；新增真实受控进程的取消、超时、双 pipe、输出上限与子孙回收 |
| [test_hook.py](../../tests/test_hook.py) → D | 中间批放行 / 末批拼接、迟到 / 缺批、阈值排除代码、失败恢复、递归禁用、Codex systemMessage；A/B 抽样、编号耗尽 / 唯一、只回注一次、改动计数与展示、确定性修复失败、诊断、记录覆盖与权限、会话及孤儿批清理。`difflib.SequenceMatcher` 的计数 / 摘录须单独差分，换任意 diff 库不能算等价 |
| [conftest.py](../../tests/conftest.py) / [test_environment.py](../../tests/test_environment.py) | 清继承的 `LIMAE_*` 并主动注入变量作回归；Rust 测试对每个 child 配独立 env / cwd / 临时目录，避免并行测试修改进程级环境；保留「注入后运行真实用例」的有效对照 |

差分 (differential testing) 分两层：文本层由 `tests/test_differential.py` 驱动 Python API 与 Rust 的薄测试适配器，比较完整有序 Finding、fixed、再次 fix；name / snippet 相等是本次为 E 入口替换选择的行为一致性 (parity) 目标，不升级为共享规则事实。进程层用上述 pytest fixture 在成对的合成临时 Git 仓执行两个 CLI，比较退出码、stdout / stderr、实际文件字节和是否写回。本仓 Markdown 也做只读差分，不读消费仓私有语料。只归一临时根路径与程序名，用法帮助布局、OS / TOML parser 的具体措辞比较错误类别与来源，不删除 warning、finding 或错误类别。

适配器是 `rust/examples/diff_probe.rs`，以显式 `[[example]]` target `diff-probe` 构建；[Cargo example target](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#examples) 可用 dev-dependencies，不随默认 Cargo 安装或发布 binary 打包。它有测试专用 JSON 输入输出：输入文本 / 规则设置，输出 Finding 列表与两次 fix 结果；传输字段的唯一说明由该源文件模块注释持有，A8b 集成任务同时维护 Python 消费端。它确实是一个测试协议，生产 CLI 不暴露该协议，也不另造 case DSL / 通用 runner 框架。

除了全部 fixture，再用有限、固定种子的业务组合：CJK / ASCII / emoji / 组合字符、规则边界两侧、不同保护容器、相互作用的修复阶段和配置开关。每次只改变一个因素；20 / 40 字符窗口测阈值两侧，取代规则顺序、漏掉边界或清环境 fixture 缺席时，受影响的用例必须变红。不扩展为任意非法输入 fuzz 平台，不为 Python 私有 helper 逐个造镜像测试。新增规则事实落 `spec/`；实际差异涉及合同缺口时，只对该项另案补最小合同、两实现共同采用，无关模块继续迁移，不把全部 Python 边角清理设成前置阶段。本文只保留映射。

### 六、Rust 审查规则从设计阶段应用

设计、每份实现 brief、自审与正式审查均加载用户级 `rust-review` 与 `pr-review`，再叠加本仓适用规则；这里记录具体设计取舍，不复制或安装另一份 skill。

- **RS5–RS9、RS29**：库用 `Config` / `Directive` / `Resource` / `Io` 等领域错误，保留路径、行号与 source；CLI 将用法 / 配置 / 指令错误映射为 2，I/O / Git / 资源执行失败为 1，报告未完成，不能打印 clean。Python 的未捕获异常 traceback 不属于要复制的界面。`None` 只表示没配置 / 无事件等正常缺席；不用 `.ok()` / 默认集吞错误。规则、严重度、操作模式用 enum / 具名 options，不给库接口加难懂的位置布尔值。
- **RS5、RS24、RS26**：单 package 在根 `[lints.rust]` 开 `unsafe_code = "forbid"`、`unused_must_use = "deny"`，`[lints.clippy]` deny `unwrap_used` / `expect_used` / `dbg_macro` / `print_stdout` / `print_stderr`；测试是否允许 unwrap 在独立质量门约定中显式写清，默认用 `Result`。stdout 只由 CLI / hook 协议边界输出，窄范围 lint 例外附理由。纯库暂不打日志，故不加 tracing；确需事件时遵循 RS24 / RS25，只记录审计过的字段。
- **RS10–RS23、RS27–RS28**：A 的纯文本 lint / fix 没有 async、spawn、signal、channel；`--all` 的 Git 是独立外部调用，要有超时、受控输出与 wait。C 的引擎 runner 单独落地，以同步管道消费 / OS 安全封装为默认；Unix 用安全的 [CommandExt::process_group(0)](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#method.process_group)，检查 PGID 大于 1，区分已退出与真实系统错误。取消与超时是不同结果，都向组 TERM、有限宽限、KILL、wait，并关闭 / 回收 stdout、stderr、stdin 的 reader / writer；孙进程仍持 pipe 时也必须到期退出。两路输出及 answer 文件设字节上限，A/B 最多两候选，失败清理全部存活调用，不靠 Drop 收尾。只有受测需求证明同步结构不够时才另提 Tokio。
- **RS20、RS22–RS25**：预设 cwd / 环境白名单、custom 的既有继承边界与 hook 递归标记分别测试；不读取真实凭证或会话作测试数据。日志、错误、Debug 不带正文、完整 prompt、argv / 环境值或原始引擎输出。正文只走产品既定的输入 / 显示 / 会话态记录通路；D 记录按 [ADR-0012](0012-single-run-polish-records.md) 保持权限与期限，批次、缓存、诊断文件另设容量上限，不能只靠过期时间控制增长。
- **RS27、RS29–RS37**：纯 lint 不依赖 Unix；首个发行验证目标为 Linux GNU / musl，macOS / Windows 的实际运行支持各自拿到 CI 证据再声明。进程 adapter 必须有非 Unix 的可编译、可诊断「不支持」路径，不能假装成功；声明 Windows polish 支持前实现等价进程树清理。进程清理测试用隔离且能 reap 的环境，包含正常成功臂和故意留下孙进程的失败臂；异步、输出限制与安全边界按实际新增路径复核。

本机只读复核于 2026-09-06：`rustc --version`、`cargo --version` 均为 1.98.0，`rustup component list --installed` 含 rustfmt / clippy，`rustup target list --installed` 含 GNU 与 musl x86_64。它是已有环境读数，不是本任务升级或安装的授权，也不据此宣布最低支持 Rust 版本 (MSRV)。

A1 新增根 `rust-toolchain.toml`，按上述环境选择具体 `channel = "1.98.0"`、`profile = "minimal"`、`components = ["rustfmt", "clippy"]`。这是 CI / 本地一致性的钉子，不是 MSRV 声明。[rustup toolchain 文件](https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file) 是版本 / components 的唯一配置来源：CI 在 checkout 根使用 runner 自带的 rustup 代理运行 Cargo，由 rustup 按文件安装所需工具链，不另引 action，不回退浮动 stable；`rustup show` 核对有效工具链与来源，覆盖它的 `+toolchain` / 环境或目录 override 必须显式处理。升级走独立 chore PR，toolchain 文件与锁文件同样只由单一任务持有。

### 七、按 PR 推进的任务表

表内文件均为拟议文件。每个代码 PR 同时交付其业务测试；约 300 行是审阅规模信号，不是硬上限，按实际代码连贯性拆分或合并下列工作包，不能靠把断言留到末尾压行数，也不拆纯占位碎片。依赖已经落地的真实模块，缺功能就不接入口，不能用 TODO / stub / 静默空结果占位。阶段 A 结束前不分发 Rust binary。

| 任务 | 目标与文件集 | 依赖 / 可并行性 | 本 PR 验收 |
| --- | --- | --- | --- |
| G | 独立的仓级 Rust 布局 / 质量门约定：仅 `AGENTS.md`，必要时项目 skill | 最先；守则 / skills 独占串行 | 明确 A1 出现 Cargo 后即启用门与 lint 例外；继续引用用户级两 skill，不抄另一仓路径 |
| A1 | 根 `Cargo.toml` / `Cargo.lock` / `rust-toolchain.toml`、`.gitignore` 加 `target/`、`rust/lib.rs` / `text.rs`、测试 target 与 `.github/workflows/ci.yml`；实现字符位置和单条宽度转换原语 | G；共享文件独占 | 可执行库测试与 doctest，emoji / 组合字符范围安全；CI / 本地按文件 pin 工具链，required check 从本 PR 起执行 fmt / clippy / test / doc，没有空 binary 或假全量通过 |
| A2 | `rust/markdown.rs` 与保护范围测试 | A1；可与 A5 并行 | 四类保护、跨行与块边界、零长 delimiter span；用真实受保护 / 正文两臂 |
| A3 | `rules/typography.rs`：全部排版规则、固定阶段与不动点；按宽度 / 间距的实际审阅规模分 PR | A2；同文件及 `lib.rs` 接线串行 | 每批对照对应 fixture，最后混合规则、重复修复、禁用组合全部通过；原文 finding 与修后结果分开验 |
| A4 | `rules/tells.rs` / `words.rs` / `resources.rs` 与 `spec/README.md`：词表嵌入、中文 / 英文实验规则 | A2 / A5；复用 A5 的 toml，可与 A3 规则文件并行；库接线串行 | 共享词表、整行锚点 / allowlist、逐处 / 每行报告、Unicode 窗口；同步构建时取资源的说明 |
| A5 | `rust/config.rs` 与配置测试，首次引入 toml 的 manifest / lock 变更 | A1；正文可与 A2 / A3 并行，依赖变更独占 | 两载体、发现停止、覆盖 / 类型 / 严重度边界；暂不接 CLI |
| A6 | `rust/directives.rs`，接入库 pipeline | A2 / A5；库接线排在 A3 后 | 状态机、配置上界、未知 id 的文件 / 行定位，真实检查及修复均生效 |
| A7 | `rust/files.rs`：Git 文件选择、ignore、文件读写与测试 | A5；可与规则文件并行，共享依赖串行 | 成对临时仓的文件集合、否定与根外路径、链接 / 换行 / 部分失败；Git 子进程有期限并回收 |
| A8a / A8b | `rust/main.rs` / `cli.rs` 与完整 golden runner；随后 pytest CLI tests / `conftest.py` / `test_differential.py`、`rust/examples/diff_probe.rs` 及 example manifest / CI 接线 | A3–A7；集成串行 | 同一 run fixture 用两个子进程执行手写期望，再做独立差分；先构建两 Rust 产物再运行 pytest，阶段 A 全量判据 |
| B1 | Cargo 包含清单、`.github/workflows/ci.yml` 的 package / target 验收、`docs/knowledge/` 分发说明 | A8b；manifest / lock / CI 串行 | 源码包解包安装、独立 binary 真命中；缺词表臂构建失败；GNU / musl 真执行；本任务只扩大 A1 已接入的 Rust CI |
| B2 | 独立 opt-in hook id、`.pre-commit-hooks.yaml` 与对应说明 | B1；入口文件串行 | 临时消费仓安装 Rust 入口与原 Python 入口各跑一次；现有默认不换 |
| C1 | `rust/polish/process.rs` 与进程边界测试 | B；独立模块，OS 依赖串行 | 真外部受控命令的成功、超时、取消、超量输出、pipe / 孙进程回收 |
| C2a / C2b | `polish/engines.rs` / prompt 资源与 `polish/config.rs`：命令展开，随后 auto / TTL | C1；接线串行 | Python tests 中的引擎 / 配置边界，调用合成 custom 路径与预设探活路径 |
| C3 | `polish/mod.rs`、CLI 接入与分发资源 / CI | C2；共享文件串行 | `polish -` 真运行；离线受控 tests 进门，真实模型 smoke 留独立证据、不作 required check |
| D1a / D1b | `rust/hook/state.rs` / `render.rs`：记录、诊断、prune，随后计数与展示 | C；模块可并行，状态协议统一 | 权限、容量、记录覆盖、计数差分；均用合成文本 |
| D2a / D2b | `hook/claude.rs` / `ab.rs`、随后 `hook/codex.rs` 与主入口 | D1；宿主 / A/B 共用状态接线串行 | Claude 迟到 / 缺批、A/B / 一次性回注；Codex 单路、递归禁用、失败后下一轮恢复；分别真实宿主对照 |
| E | Rust bin 命名、`pyproject.toml` reference 入口、README 弃用说明、仓内 hook / dogfood / CI；本地受管安装由 orchestra 在 machine-setup 另走 PR | D 全过；互审后自动执行，外部消费仓仅调查 / 准备 | 正式 CLI 为 Rust `limae`，Python deprecated 且 reference / 测试保留，安装 / 回退可复演；汇报发布缺口并停在实际 tag / release / publish 之前 |

根 `Cargo.toml`、`Cargo.lock`、`rust-toolchain.toml`、CI、`rust/lib.rs` 与共享测试入口由当时的集成任务独占；「可并行」只指文件集不相交的正文，接线 / 加依赖在合入时串行。`uv.lock` 也遵守原有单持有者规则。本方案不改 `AGENTS.md`、skills 或 tracker，G 由后续独立任务完成。

G 建议约定：既有两个 Python 裸质量门继续保留；从 A1 首个 Rust 代码 PR 起，按仓根 toolchain pin 新增 `cargo fmt --check`、`cargo clippy --all-targets --locked -- -D warnings`、`RUSTDOCFLAGS=-Dwarnings cargo test --locked`、`RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked`，本地与现有 required check 同步执行。[RUSTDOCFLAGS](https://doc.rust-lang.org/cargo/reference/environment-variables.html) 将文档警告作为错误，文档构建与实际执行 doctest 两项分别验收。

A8b 后，裸 `uv run pytest -q` 仍只跑 Python 臂；push 前 / CI 则先 `cargo build --locked --bin limae-rs --example diff-probe`，再 `uv run pytest -q --rust-bin target/debug/limae-rs`。给参数就必须运行双臂，CLI 或其旁的 `examples/diff-probe` 缺席均失败，不能 skip；E 更名时同步该命令与 Python 臂入口。B 再加入 package / 安装 / musl 验收。Rust 的重检查放 push 前与 CI，commit 钩子不塞完整 Cargo 构建。每次 push 前全部已启用的质量门全绿。

## 后果

优先拿到完整的确定性工具，再扩大分发和宿主影响面；代价是迁移期维护两套实现，Rust 词表更新需要重建 binary。换来的是单文件部署与更少的路径 / 安装状态。未测性能前不承诺加速倍数。规则事实始终留在 `spec/`，两实现的差异必须被测试暴露并逐项裁定。agent 自动推进与互审覆盖当前 Python 已实现功能，不把后续产品功能或其他消费仓的整体迁移加进完成条件。

「Rust 迁移完成、待发版」须同时满足 A–D 的真实路径、所有现有 Python 业务边界的迁移对账、无未裁决的差分、checkout 外发行验收、两宿主试用与 E 的入口迁移 / 回退验收。此时正式 CLI 是 Rust `limae`，Python deprecated 且保留 reference / 测试，全部已启用测试通过；只有 49 组 fixture 通过时只能称规则黄金集兼容。

本方案的工程默认均已给出，后续由 orchestra 持续派活、agent 互审推进，不再等待人工切换许可。停止点是实际创建发布 tag、GitHub release 或执行 `cargo publish` 之前；届时汇报 crates.io 发布尚缺的许可证、发布权限 / 认证配置、元数据、名称及流程验收等，认证只报告状态与引用，不读写凭证值，不先发布占位包。发布流程调查由 orchestra 单独负责，本文不另建一套流程。

补记 (2026-09-08)：用户 2026-09-08 裁决 **Python 参考实现从此冻结、不再发布，版本号与 Rust 走同一条序列**。原话是「Python 从此不再更新、版本与 Rust 走同一条序列」。这条落在上面「Python deprecated 且保留 reference / 测试」之上，把「deprecated」收紧成一个可判定的边界：`limae-python` 入口与全部 Python 测试继续保留、继续跑 (它是差分臂的对照一侧，撤掉就没有对照了)，但不再打 wheel、不再上传任何包索引，`pyproject.toml` 的 `version` 只是跟随 `Cargo.toml`、不表示存在对应的 Python 发布物。同日的另一条裁决解除了上一段那个停止点：0.13.0 已获授权发到 crates.io，`Cargo.toml` 的 `publish = false` 因此删除，发布步骤与判据的正本是 [发布手册](../knowledge/release.md)。本节其余各段不变。
