<!-- 任务 slug：polish-debate；关联 PR：文件模式实现 PR (编号见 docs/tracker.md 同日条目)；消费者：limae-orchestra、用户；作者：limae-polishdebate-astra (gpt-6-astra) -->

> 归档说明：本文件由 `limae-polish-impl` 于 2026-09-13 从 `~/scratch/limae/debate-polish-filemode-astra-2026-09-12.md` 原样归档入库到 `docs/research/`，正文一字未改，唯一的编辑是把「证据边界」引用的旁支文件 (第一轮实验记录) 内联为文末附录 (内容未改，只把附录内的标题降一级)，并把那个相对路径换成指向附录锚点的仓内链接。裁决方读的主检出是正文自己声明的 `187a7e0`。**两轮的最终设计在 [ADR-0017](../adr/0017-polish-file-mode.md)**：第一轮 Q1 推荐每次 flag、Q2 推荐外部强制隔离的完整副本，第二轮 Q1 改判为用户侧授权记录、Q2 改判为 limae 唯一写回；用户最终裁的是「每次 flag + 导出视图 + limae 唯一写回」，读者不要把任一轮单独当成终态。归档与本次编辑均由实现方完成，未改动裁决方本人的任何判断或措辞。

# polish 文件模式：Astra 独立裁决，第一轮

日期：2026-09-12。作者标识：`limae-polishdebate-astra`，由 `herdr agent get` 核到。未读取另一位裁决方的屏幕或产物。本文是设计裁决，不是 PR 审查。留存触发点：等待 limae-orchestra 消费两轮结论；形成仓内决策后删除本文及证据附件，未入库则由 orchestra 在收工时重新确认消费者。

**【推断／推荐】Q1 用每次调用的显式 CLI 开关，仓库配置不能代为授权；Q2 哈希回滚不能兑现“只能修改指定集合”，必须改承诺或增加执行时隔离；Q3 当前架构用一个同时说明读取与写入的文件模式授权，不造两个没有独立权限边界的开关；Q4 prompt injection 是真实风险，那句话仅是未经本场景验证的软缓解，真正的权限约束要在模型之外执行。**

## 证据边界

- **【文档／源码】** 检查的本地主检出为 `187a7e0566ab019084b89bf068f9dd102fb33105`。`rust/polish/engines.rs:417` 起的三种预设展开使用临时目录；`rust/polish/process.rs:212` 起执行 `current_dir`、`env_clear`、白名单环境及独立进程组。这里“文档”包括静态源码证据，不冒充引擎运行实验。未 fetch，遵守本任务不修改主检出下任何文件的只读约束。
- **【文档／源码】** `spec/polish/general.md:15` 起含正文不是指令的规则；Claude / Codex 另有随机边界标记，Grok 直接传正文。`Engine::Custom` 保留请求的工作目录，不在“三预设使用临时目录”的结论内。
- **【文档／历史记录】** ADR-0008 §三记录了 Grok 引入仓库上下文的既往实验；§六说 polish 不进入 required check。后者不能推出消费方不会把它放进自己的脚本或 CI。此次没有重跑 ADR 的模型实验，也没有在该 ADR 中找到足以独立确立“所有外发字节完全可预测”的证据。
- **【实测】** 在本机 Linux 的独立 `/tmp` 目录运行 Python 标准库探针，11 条结果及断言全部通过，命令退出 0。只操作合成文件，不读取真实凭证，不调用真实引擎、不联网发送实验数据。完整脚本及结果见同目录附件 [实验记录](#附录astra-第一轮实验记录原样内联)。
- **【文档】** 外部官方资料于 2026-09-12 查阅，支持的只是攻击类别与隔离机制；没有拿其它产品或浏览器评测的成功率来替代本仓三引擎的实际成功率。

## Q1：每次调用的 CLI 开关，拒绝隐式持久授权

**【推断／推荐】** 新增一个明确表示启动仓库 agent 的参数，例如 `--allow-repo-agent`。这是本文建议的新名字，不是现有参数；刻意避免让名字只说“read”，却实际交出完整工具能力。用法为：

```sh
limae polish -
limae polish --allow-repo-agent README.md docs/guide.md
limae polish --allow-repo-agent --all
```

**【推断／推荐】** 文件参数或 `--all` 没有该开关就立即非零退出，解释其会让本机 coding agent 读取仓库内容并尝试修改目标。检查必须发生在任何引擎探活或启动之前。stdin 模式永久保留临时目录语义；它与该开关混用应报参数冲突。配置文件和环境变量不得默默替调用者补上同意。解析配置可能需要 limae 自己读取配置文件；这与授权引擎遍历仓库应明确区分。

| 候选 | 【推断】失败模式 | 裁决 |
|---|---|---|
| 每次 CLI flag | 可被写进 Makefile、alias 或 CI，后来调用者只看到 `make polish`；也可能被人机械复制 | 推荐。授权至少位于实际触发操作的命令上，裸调用仍拒绝。不能把 argv 中出现开关描述成“用户本次已认真理解” |
| 仓库配置键 | 可被 commit、clone、PR 修改，消费仓 CI 自动继承；把被处理仓库的内容当成授权来源 | 拒绝作为授权。即使主仓禁止 polish 进 required check，也约束不了消费仓 |
| 用户全局配置键 | 不随仓库 commit，但一次同意自动覆盖后来的不可信仓库；重建 HOME 或不同机器又行为不同 | 拒绝作为常开授权 |
| flag 与配置都必须有 | 两者可一起被提交进仓库；若配置移到全局，则变成旧同意加新命令，并未增加执行时能力限制 | 不要求这两把同意开关。独立的管理员“禁止运行”策略可以有，但它是限制，不能授予权限 |
| 首次交互确认 | headless 无法作答；缓存的“首次同意”不随仓库内容、身份或机器变化失效；无法回答时默认通过最危险 | 拒绝作为主路径。无交互时应拒绝，不能隐式接受 |
| 单凭 `<files>` 或 `--all` | 用户可能按普通 formatter 的心理模型理解为“只读写这些文件”，没意识到 agent 会读其它文件 | 不足以表达仓库读取 opt-in |

**【推断／边界】** CLI 开关不能防住已经获准执行的恶意 Makefile；一旦用户执行任意仓库代码，那段代码原本就可以直接调用 agent。为了拦这个失败模式再堆一层同意配置，不能形成新的安全边界。也不要用启动后的警告充当授权；它至多告知，来不及阻止已发生的读取。

**【推断／拟验收，尚未执行】** 用只写合成启动标记的假引擎：无开关，即使仓库配置声称允许，也应报错且标记不存在；同一命令加开关，假引擎应被实际启动并生成标记。对照的观测是启动副作用，不只是不同错误文案。另设 stdin 臂，假引擎报告 cwd，应处于临时目录；文件臂应处于约定的仓库执行视图。

**【推断／改变主意的条件】** 如果出现独立于被处理仓库、绑定仓库身份且随权限范围变化失效的组织授权机制，并以以上两臂证实仓库内容不能自授权限，我会接受它替代每次 CLI flag。仅有“大家觉得麻烦”或两个开关都常绿，不改变结论。

## Q2：否决“哈希回滚等于只许修改指定文件”

**【推断／推荐】** 前后哈希是事后检测，不能提供执行时的写权限约束。即使每次都报错，越界写入也已经发生；恢复字节不能撤销被其它程序观察、被网络传出或触发执行的效果。不要把这种机制写成事务，也不要默认用全仓恢复来修复不明归属的差异。

**【推断】** 先定义三个集合：写入目标 W、提供给引擎的可读集合 R、运行后核对的集合 M。检查越界要求 M 覆盖 W 以外的有关对象；若“作用域内所有文件”只指 W，根本看不到范围外修改。ignore 只决定 `--all` 的目标集合，不自动决定 R，更不是文件系统权限。若检测也尊重 ignore，ignored 文件就成为检测盲区。

| 攻击或失败 | 证据等级与本次结果 | 哈希方案的边界 |
|---|---|---|
| 持续修改非目标文件 | **【实测】** `other.txt` 字节改变被发现；只改 `allowed.md` 的正向业务臂也通过 | 表明探针确实能识别它擅长的情况，不是一个恒绿程序 |
| 改权限、不改字节 | **【实测】** chmod 后内容哈希完全相同、mode 不同 | 内容哈希遗漏元数据；扩展属性、ACL 等属于同族遗漏，但此次没测它们 |
| 写后恢复原内容 | **【实测】** 一个合成观察者在中间复制了改后字节，恢复后全仓内容哈希相同，观察者仍持有改后副本 | 与从未写过给出相同结果；更强哈希无用。观察者是脚本安排的读者，不声称测过真实 watcher |
| 允许路径是硬链接 | **【实测】** 通过 `allowed.md` 写入同时改变仓库外同 inode 文件，非目标哈希保持原值；普通目标文件是对照 | 路径名集合不等于对象集合；只做字符串路径检查不够 |
| 符号链接换目标 | **【实测】** 链接由一个文件转向另一个同内容文件，按解引用内容计算的哈希不变 | 必须区分文件类型、链接目标与内容；路径祖先被替换的竞态是额外的未测风险 |
| 新增文件 | **【实测】** 只重算初始路径列表完全漏报；重新枚举路径集合可发现 | 有可行补救：名字集合也要比较。删除、重命名同样需要集合语义 |
| 自动回滚覆盖并发编辑 | **【实测】** 模拟用户在期间修改非目标文件，备份恢复后哈希回到原值，但用户编辑丢失 | “agent 越界”与“用户自己编辑”给出同样差异，哈希无法归因；禁止无归属的全仓强制回滚 |
| 检查以后才写 | **【实测】** 同组子进程被组信号终止后不再写；另起 session 的子进程在检查时哈希相同、触发后继续写 | 原进程组清理不等于所有后代失去写能力。本次测的是进程机制，不是 limae 集成回归 |
| 被允许修改的 README 混入机密 | **【推断】** 如果 agent 已读到机密，只写允许目标仍能完成泄漏 | 完美的写集合约束也发现不了语义泄漏；不能拿“只改了目标”验收机密性 |
| 仓库外写入、Git 状态、网络副作用 | **【推断】** 题目已指出；Git 元数据若真在 M 内且字节持续改变，完整扫描能发现那部分，不能笼统说一切 Git 操作都看不见 | 外置 gitdir、其它 worktree、远端 push 或外部服务状态仍非工作区恢复能处理；此次没有操作真实 Git 状态或网络 |
| 清理失败、快照不可信 | **【推断】** 只有哈希没有原字节就无法恢复；备份若 agent 可改也不可信；崩溃、磁盘满、取消可发生于逐文件恢复中 | 备份不是原子事务。以 Git HEAD 恢复尤其会抹掉启动前未提交内容；此次没有故障注入 |

**【文档】** Linux `setsid(2)` 说明成功调用会创建新 session 与进程组。本仓源码 `process.rs:418` 的清理向原组发信号。上述实验支持“进程组不是恶意子进程的约束边界”，不否认它对普通超时清理的价值。[setsid(2)](https://man7.org/linux/man-pages/man2/setsid.2.html)

**【推断／准确 README 措辞】** 对“全仓核对、保存备份、事后恢复”版本，最多可以这样写；这是拟实现后的契约，不能当作今天已实现：

> 文件模式会让本机 coding agent 在仓库中运行，并要求它仅修改指定目标。limae 在运行前后比较纳入核对范围的文件内容；发现目标之外的内容变化会报错，并尝试从运行前备份恢复。恢复可能失败，也可能覆盖运行期间的其它编辑。这是事后检查，不是写入隔离，不能保证其它文件从未被修改，也不能阻止仓库外操作、Git 状态变化或内容外传。`--all` 的 ignore 规则只筛选改写目标，不限制引擎读取范围。

**【推断／推荐】** 我不建议把上面的危险恢复行为作为最终产品：应保留差异和恢复材料、报冲突，让非独占环境中的恢复成为明确动作。README 届时把“尝试恢复”改为“保留差异与恢复材料，不自动覆盖检测到的其它编辑”。文件模式方向不变，但若产品必须承诺“只能修改这几个文件”，应把原仓库的写权限从 agent 手里拿走：在外部强制隔离的仓库副本中运行，由可信的 limae 进程只导入 W 的结果；导入前核对目标是否仍是运行前版本，冲突则拒绝覆盖。单独建 tempdir 或 Git worktree 不构成这种隔离。

**【推断／拟验收，尚未执行】** 对上述隔离方案，用确定性恶意假引擎直接尝试写非目标、仓库外和 `.git`，包括子进程、链接及 rename 路径；不依赖模型是否听话。启用隔离应让原仓库这些对象不发生写入，解除隔离的相同探针应实际写成功；同时正常编辑 W 必须能导入。并发臂在导入前由外部编辑目标，导入应拒绝且保留外部版本；无并发臂应成功。只检查最后哈希不变仍不够，拒绝写入要看操作返回值及外部观察者在运行期间是否读到过改变。

**【推断／改变主意的条件】** 外部强制的文件系统／进程边界在以上对照及目标平台上成立，我会允许对受保护的原仓库使用“仅导入指定文件的修改”这一强承诺。增加哈希位数、把 `.git` 也扫一遍、一次真实模型听话，都不够。

## Q3：当前文件模式只用一个复合 opt-in

**【推断／推荐】** 不增设第二个 `--allow-write`。Q1 的 `--allow-repo-agent` 同时告知并授权“启动具备工具能力的仓库 agent”；文件参数指定其被要求修改的目标。帮助文字必须分别陈述读取范围与写入保障的实际强度，不能只写“允许读取仓库”。

**【推断／理由】** 机密性 (confidentiality) 与完整性 (integrity) 必须分别建模，但两个开关不自动产生两个能力边界。今天若给了 `--allow-read` 就启动带写工具、没有强制限制的 agent，“读同意、写不同意”仍可能写；那不是更安全的状态，而是虚构的只读状态。若所有有效调用反正都强制携带两个开关，它们等价于一个更长的复合确认，也没有提供真正可选的低权限用法。

**【推断】** 因而这里单开关的决定以“不声称它是安全隔离”为前提，不能反过来拿复合同意豁免“只改集合”的产品要求。授权回答“是否愿意启动”，执行边界回答“实际能做什么”，分别验收。

**【推断／改变主意的条件】** 如果新增真正只读的仓库分析／预览阶段，能在模型之外阻止全部写入，并且用户能选择读完后不应用结果，我会改为两阶段授权：读取授权与应用修改授权。对照是只读状态下恶意写探针失败、正常引用读取成功；允许应用后，仅 W 的变化能进入原仓库。只有 `--dry-run` 名字或 prompt 里的“不要写”不足以改变判断。

## Q4：真风险；prompt 规则是软缓解，不是权限边界

**【推断／裁决】** 保留这条规则，并将其归类为“未经本场景验证的概率性缓解”。不把它算进“只能改指定文件”或“机密不会外传”的保证。说它在任何时候都毫无作用同样缺证据；说存在这句话就已缓解到可放心运行，也缺证据。

**【文档】** Anthropic 的官方资料描述了低信任内容指挥 agent 使用既有工具外传信息的攻击类别，并明确称问题尚未解决。这支持风险存在，不代表本文测到了 limae 三引擎的成功率。[Prompt injection defenses](https://www.anthropic.com/news/prompt-injection-defenses)

**【推断】** 题目里的攻击链是：参考 Markdown 中的攻击者文字 → 被当成指令 → 读取 `.env` → 将内容写进允许的 README。只要读权限覆盖 `.env` 且 README 属于 W，最后两步甚至不违反理想的文件权限表；权限正确仍可发生信息流违规。写完后等人工看 diff 也不能撤回此前已进入模型服务的内容。明确同意读仓库不等于同意把仓库中任意内容复制到公开文档。

**【推断】** 边界标记仅帮助模型辨认输入分段；它不约束之后工具读到的 Markdown，也不阻止模型选择服从它。仓库指令文件与项目配置也是低信任入口；不能让待处理仓库自己调整外层权限。常规预览若仍由完整权限的 agent 在原仓库执行，也不自动降低这一风险。

**【文档】** Anthropic 的 sandbox 设计将文件系统与网络隔离作为模型之外的能力边界，并覆盖子进程。本文引用这两个机制，不照搬其营销文字中的绝对安全承诺，也不假定三家当前命令已启用等价限制。[Claude Code sandboxing](https://www.anthropic.com/engineering/claude-code-sandboxing)

**【推断／推荐】** 真正能减少后果的措施是：在外层强制限制可见数据与可写对象，让 agent 无权访问不应流出的内容；限制外连与宿主 IPC，且约束子进程；用可信应用步骤把结果限制到 W。可以保留仓库上下文，但应定义提供给 agent 的具体视图。不能一边允许它读全部机密，一边宣称网络白名单能阻止信息泄漏：模型服务本身就是允许的外发通道，允许的 README 也是输出通道。本机已登录的 CLI 还需要认证资源，必须单独解决认证与宿主资源隔离；“整个 HOME 挂进去”不能宣称宿主机密已被隔离。本轮未实现这些边界，也未验证跨平台／三引擎兼容性。

**【推断／拟验收，尚未执行】** 对“机密不进入模型”的边界，外层用只含合成标记的私密文件与可读参考文件两臂：前者由确定性读取探针访问应失败，后者应成功。故意关闭限制后同一私密读取应成功，以排除探针本来不会读。网络限制同理用本地允许／禁止接收端，比较实际接收记录；能连模型服务不等于所有出口被正确限制。上述测试是权限验证，不是 prompt injection 成功率评估。

**【推断／改变主意的条件】** 如果要把原 prompt 规则从“未经验证”提升为“本产品已实证有效的软缓解”，需要固定引擎、版本与设置，对同一批合成仓库攻击反复运行有／无该规则两臂；用真实工具读写和合成内容泄漏作为结果，另有正常润色成功臂，不能只统计模型说“我拒绝了”。一次拒绝或一次 PONG 不成立。即使攻击率下降，也只改变概率性缓解的证据等级，不把它升级成权限保证。

## 对旧模式前提的一处必要校正

**【实测】** 探针在空目录启动、只保留合成 PATH 环境，仍能读取目录外已知绝对路径的合成文件；访问不存在文件的对照失败。

**【推断】** 因此临时目录是减少自动仓库发现的有效设计意图，但不是“工具无处可施”的安全证明。现有实现能支持的措辞是“limae 显式提交的正文与 spec 可确定，预设不在调用者仓库启动”；无法仅凭 cwd 与 env_clear 推出“引擎对外发送的所有字节就是 pipe 的那一段”。模型服务交互、本机配置与工具行为是更大的系统。本结论不主张删掉临时目录，也没有重新争论文件模式是否该做；它只是避免把旧模式夸大的安全前提带进新模式裁决。

## 第二轮：回应对方

**【文档／交叉阅读】** 已完整阅读 Fable 第一轮报告，另读其 `run.sh`、Claude / Grok 只读工具臂的 stdout 与退出码、Codex 默认臂及关闭项目文档臂的 stderr。下文“Fable 实测”指对方的实验记录，未冒充本人的独立复验。orchestra 第二轮 brief 给出的空 cwd 外读、Claude 项目 hook 四臂，作为**【实测／orchestra 复验转述】**直接接受，没有重测。本节只处理 Q1 与 Q2。

### 一、Q1 改判：flag 不能承担授权证明，但用户信任记录也要堵住自授权入口

**【推断／明确改判】** 我撤回第一轮“单独的每次 CLI flag 是推荐授权层”的结论。Fable 指出的构建文件固化场景下，**我的方案不能安全失败**：仓库自带 `make polish` 的命令行已经含 flag，limae 会启动引擎，即使运行者只以为在调用普通 formatter。flag 没有办法从 argv 判断它由用户亲自输入还是仓库脚本填入。第一轮以“任意脚本本来就能执行代码”解释该局限，在本题要求防止未充分理解的普通调用这一层过早放弃了防护。

**【推断／新推荐】** 授权决定移到使用者一侧，文件模式只消费已有授权；仓库配置、环境变量与文件模式 flag 都不能授予权限。**默认只同意这一次；只有用户明确选择持久信任时，才保存按仓库登记的记录。** 一次性同意应在可信的确认入口后直接执行已展示的那次任务，不先留下一个任何竞争调用都可以抢先消费的宽泛“下一次允许”。headless 调用没有既有授权就拒绝，不能尝试代用户完成首次登记。具体 UI 与存储协议属于实现设计，本轮没有验证，也不把一个拟议的子命令当成已经存在的授权机制。

**【推断】** 我接受 Fable 候选 C 对“仓库 flag AND 仓库配置”的反驳：两个攻击者控制的比特没有新增边界。用户授权记录与请求 flag 并非都由仓库文件直接提供，逻辑上不属于这个反例；但在当前文件模式中，再强制 `--allow-repo-agent` 最多提醒直接敲命令的人，不拦固化了 flag 的 wrapper。**因此我不推荐“用户信任记录 + 每次 flag”双重必选**：文件参数已经表达本次请求，一次性或持久授权表达同意的寿命，额外 flag 不解决这里剩下的问题。

**【推断／对 Fable 的必要修正】** Fable 的 E 若允许 CI 或 Makefile 无交互执行 `limae polish --trust`，然后再运行文件模式，原来的攻击能原样搬过去：

```text
仓库脚本：先调用无交互登记命令，再调用 polish --all
```

**【推断】** “显式子命令”不等于“用户亲自同意”；它与“显式 flag”有同一个来源不可判别问题。故我接受的是**使用者侧授权**，不接受“任何调用者都能无交互登记”的 E 原样方案。CI 授权须由用户／管理员控制的外层任务配置提供，不能把同一份不可信仓库 workflow 里的自登记步骤算成独立同意。没有这样的来源边界，就只能将记录称为防误用门槛，不能称为防恶意构建脚本的安全边界。

**【推断／不可夸大的边界】** 家目录文件归同一 UID 所有时，恶意 Makefile 通常可以绕开登记命令直接改记录，或者直接调引擎。`0600` 保护不了同 UID 的另一个进程；路径规范化也解决不了这个问题。要真正对抗已执行的恶意脚本，必须限制它的宿主权限，或让授权由独立权限主体持有。limae 的小型信任记录不应悄悄扩张成能解决这一问题的承诺。它仍比单 flag 更适合**未登记仓库里的正常 wrapper 被无意调用**：该 wrapper 不会仅因自带 flag 而启动引擎。这里把实际收益和无法覆盖的攻击分别写清。

**【推断／授权寿命】** 持久信任必须明确表示“以后也允许”；按路径记录不会因 `git pull`、目录内容替换而自动失效。一次性默认消除了“我只想用一次却留下永久授权”的失败模式。暂不建议拍一个任意 TTL 来假装解决内容变化：有效期内一样可以换内容；用户明确选择持久时，应能撤销，并知道它信任的是后续该仓库任务，而非此刻某个 commit。路径被替换时是否重新确认应由仓库身份策略处理，不能用路径字符串相同当内容没变的证据。

**【推断／拟验收，未执行】** 一组对照应区分三件事：① 未登记时，仓库 wrapper 内带 flag 仍不得启动假引擎；② 使用者侧批准一次后，该次正常读取与改写可以完成，再次调用重新拒绝；③ 试图从 headless wrapper 调无交互自登记，不能取得权限。若宣称能挡同 UID 恶意脚本，则还必须验证直接写用户记录的攻击臂；该臂能通过就收窄声明，不能只展示前三臂。所有“拒绝”的对照均有获准后真正启动并完成合法任务的成功臂。

**【推断／再改判条件】** 若本地信任只能靠“脚本不会调用登记命令／不会改文件”成立，它就不能获得强于 flag 的恶意脚本防御评级，我会明确降为防误用机制；若一个来源独立、可一次性授权的机制实际通过上述对照，我会接受它作为本题授权层。不会因文件放在 HOME 就宣称这项已经验证。

### 二、Q2 部分改判：采用 limae 唯一写回，但不以工具名单代替完整执行边界

**【推断／明确改判】** 接受 Fable 的简化：引擎只返回改写文本，由 limae 写目标文件。**撤回第一轮把“可写的完整仓库副本”作为默认实现形状的推荐。** 它为不需要的 agent 写能力付出了复制、收集 diff 与还原路径的成本。仍坚持第一轮的核心要求：如果 README 要保证其它原仓库文件不被修改，执行时必须实际拿走引擎及其启动阶段、后代进程对原仓库的写能力，不能只要求模型选择不写。

**【推断／推荐形状】** 外层提供原仓库的受保护只读视图；引擎使用最小只读工具集，关闭不需要的项目配置／hook／扩展加载；输出留在私有临时区域，limae 从答案通道取得正文，只写 W。这样可以保留真实仓库作为参考来源，**不必默认复制整个仓库**。这是设计建议，未在本机配置隔离或验证三家兼容性。认证与引擎自己的临时状态需要专门的允许位置；“只有 limae 写原仓库”不应表述为“整个系统只有 limae 会写任何文件”。

**【文档】** Linux `mount(8)` 描述了只读 bind mount：可以令一个挂载入口只读，同时原入口仍可写。这证明只读视图不必复制文件数据；也直接提醒其限制：**若进程仍能走原来的可写入口，单独加一个只读入口不构成隔离**。因此必须结合外层访问限制，覆盖别名、子挂载、继承的可写句柄与宿主 IPC。本文未执行 mount，不把这一 Linux 文档机制冒充跨平台成品。[mount(8)，2026-09-12 查阅](https://man7.org/linux/man-pages/man8/mount.8.html)

#### 对 Fable 工具层方案的具体失败模式

- **【实测／orchestra 复验转述】** 项目 hook 可以在模型之前写文件，`--setting-sources user` 对所测 Claude 项目 hook 有效。**【推断】** 这支持关闭那个入口，不支持“只读工具已经约束整个引擎进程”。用户层 hook／插件、其它自动启动入口仍需各自覆盖；本轮没测它们，不声称发生过绕过。
- **【文档／对方日志】** Claude / Grok 允许名单臂输出“没有写工具”，合法读取合成文件成功；Grok 默认臂存在实际写入。**【推断】** 这是有价值的工具限制证据，但不穷尽进程在模型工具之外的写入路径。Fable F7 自己也记录了引擎会话落盘。README 的“写文件的只有 limae 自己”至少必须限定为受保护的原仓库。
- **【文档／对方日志】** `codex-raw.stderr` 与 `codex-nodoc.stderr` 均有 `sandbox: read-only`，同时工具执行失败于 `bwrap: loopback: Failed RTM_NEWADDR`；二者都有 `hook: PreToolUse` / `Completed`。**【推断】** 后者不能证明是哪份 hook，更不能据此称恶意项目 hook 已运行；但它明确不支持“关掉项目文档就没有 hook 执行”。前者也还缺同设置下合法参考读取成功的业务臂；沙箱初始化失败与正确限制写入都能让任务没写成，不能混为同一种验证。
- **【推断】** Codex 本身的 OS 沙箱若覆盖所有有关入口并经验证，可以成为执行边界的一部分，不必因为由引擎 flag 启动就另套一份重复沙箱。问题在覆盖范围和保证，不能简单按“来自引擎”或“外部工具”划定可信与否。默认值则不够：必须显式选择限制、验证运行事实；未知或失效时拒绝文件模式，不能自动回退到更宽权限。
- **【推断】** limae 自己的写回也要处理符号链接／硬链接别名和并发编辑。仅仅“只调用一次指定路径的 write”还不够，第一轮硬链接实验已经说明路径名与底层对象不是一回事。目标更新要保存启动前内容的前提，冲突时保留外部编辑，不能拿 HEAD 覆盖。这个要求对两种方案完全相同。

#### 1. Grok 没有关闭 GROK.md 的 flag，是否直接不支持

**【文档／对方已报告的能力缺口】** Fable 未找到关闭 `GROK.md` 自动加载的 flag；其只读允许名单臂仍输出了项目标记。此次没有重新搜索 flag，也没有重跑 Grok。

**【推断／裁决】** **当前证据不足以让 Grok 文件模式承担强写入承诺，首版应暂不支持这个组合，不能退回全工具 + 哈希。** 但理由不能写成“能读 GROK.md，所以必然能写”：项目指令进入模型是控制内容，写权限是执行能力；一个真正只读的进程读到恶意指令也不因此获得写权限。自动加载关不掉说明三家无法承诺相同的项目文档加载行为；尚未核清的启动入口、Grok 允许名单只覆盖模型工具的证据，则说明还不能承诺相同的完整执行限制。

**【推断／恢复支持条件】** 如果外层隔离已保护原仓库及有关宿主对象，Grok 的合法参考读取和受控写入攻击对照均通过，那么即使仍加载 `GROK.md`，也可以支持文件模式并明确这条上下文差异。反之，即使未来新增关闭 `GROK.md` 的 flag，也不能单凭它恢复强承诺。Claude／Codex 同样按已验证的“引擎 + 版本 + 平台 + 设置”组合准入，不能因 Grok 有显眼缺口就把另两家默认视为全通过。

#### 2. 承认完整副本的实际成本与信息失真

**【推断】** 普通完整复制需要随文件数量与总字节量增长的枚举、读取、写入和额外空间；大体积 ignored 目录也在“完整”的成本里。只复制 tracked HEAD 能便宜一些，却丢掉用户正想参考的未提交、untracked 或 ignored 内容。使用硬链接省空间会共享底层对象，违背隔离目的；写时复制 (copy-on-write) 可以降低数据复制成本，但依赖文件系统支持、仍有元数据与后续写入成本，本轮未测，不能作为通用默认前提。

**【推断】** 完整副本也不天然保真：没有 `.git` 就没有真实 Git 状态；另建仓库得到的是另一份历史与 index；复制 `.git` 仍可能遇到指向外部的 worktree／对象存储路径；副本绝对路径改变会使文档中的路径与工具判断不同。复制期间用户编辑还可能形成并非某一时点的混合内容。不能向 agent 虚构“这是实时原仓库”，也不能把一次目标哈希检查说成全仓一致快照。

**【推断／据此简化】** 默认采用受强制限制的实时只读视图，避免上述全量复制成本与大部分路径失真；相应承认它不是快照，运行期间参考内容可能变化。Git 元数据若提供，必须沿实际关联路径保持只读；若为安全隐藏，便明确不可用，不给它伪造的 Git 状态。本任务是文本润色，无须强行提供完整 Git 命令体验。只有业务明确需要一致时间点的仓库参考，才再为快照付成本。

#### 3. 能叠加，而且这里有不同作用；不叠两套相同保证

**【推断／明确推荐】** 叠加“最小只读工具 + 关闭不必要配置加载 + 覆盖整个引擎执行的只读边界 + limae 定点写回”。前两项缩小入口与能力，第三项使某个工具开关失效或启动入口遗漏时仍不能写原仓库，第四项约束落地结果与并发冲突。**不叠加完整副本、全仓哈希回滚与第二套同功能沙箱作为默认流程**；哈希若保留只是有限检测，不负责兜底权限。

**【推断／拟验收，未执行】** 用拆分臂验证叠加的实际收益：① 工具限制和强制边界都在，合法引用与目标写回应成功；② 只拿掉工具限制，以确定性探针尝试写原仓库，仍应被执行边界拒绝；③ 只拿掉执行边界，受控启动代码尝试写原仓库，若工具名单管不到启动代码就会写成；④ 两者都拿掉，同一写探针必须成功，证明探针与目标本来可写。③ 可利用 orchestra 已复验的 hook 机制设计，但本轮没有重测。此处验证的是不同边界的职责，不是让模型随机决定要不要攻击。

**【推断／再改判条件】** 若某家引擎能给出并实证：限制涵盖启动代码、所有工具、子进程和宿主访问，且配置被仓库影响时也无法放宽，那么可以直接使用这份完整边界，外部再包装同等隔离只剩摩擦。当前允许名单的实验还没有达到这个覆盖程度；“没有某工具”的模型自述与一次无写结果不能代替它。

## 附录：Astra 第一轮实验记录 (原样内联)

【实测】2026-09-12，本机 Linux；仅合成文件，未调用真实引擎。下面脚本于写入本附件时再次执行，退出码 0。该脚本保存到任意自有临时目录后用 `python3 probe.py` 执行即可复跑；数据子目录生成在脚本同目录。

留存触发点：与裁决正文一起等待 limae-orchestra 消费；结论入库后删除，否则收工时复核。

### 原样结果

```json
[
  {
    "case": "allowed_control",
    "changed_only_allowed": true
  },
  {
    "case": "persistent_control",
    "detected": true
  },
  {
    "case": "chmod",
    "hashes_equal": true,
    "mode_changed": true
  },
  {
    "case": "transient_write",
    "hashes_equal": true,
    "observer_retains_changed_bytes": true
  },
  {
    "case": "hardlink",
    "outside_changed": true,
    "non_target_hashes_equal": true
  },
  {
    "case": "symlink_retarget",
    "hashes_equal": true,
    "link_target_changed": true
  },
  {
    "case": "new_path",
    "original_paths_only_misses": true,
    "rescan_names_detects": true
  },
  {
    "case": "rollback_concurrent_edit",
    "hashes_restored": true,
    "user_edit_lost": true
  },
  {
    "case": "empty_cwd_env_clear",
    "outside_read_succeeds": true,
    "missing_control_fails": true
  },
  {
    "case": "late_child_same_group_control",
    "hash_equal_at_check": true,
    "changed_after_check": false
  },
  {
    "case": "late_child_detached",
    "hash_equal_at_check": true,
    "changed_after_check": true
  }
]
```

### 完整探针

```python
from pathlib import Path
import hashlib, json, os, signal, subprocess, sys, tempfile, time

BASE = Path(__file__).parent
results = []
def digest(root):
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in root.rglob('*') if p.is_file()}
def record(name, **values):
    results.append(dict(case=name, **values))
def fresh():
    p = Path(tempfile.mkdtemp(dir=BASE)); r = p / 'repo'; r.mkdir()
    (r/'allowed.md').write_text('original'); (r/'other.txt').write_text('original')
    return p,r

p,r=fresh(); before=digest(r)
(r/'allowed.md').write_text('edit')
assert {k for k in before if before[k]!=digest(r)[k]}=={'allowed.md'}
record('allowed_control', changed_only_allowed=True)
(r/'other.txt').write_text('persistent')
assert before['other.txt']!=digest(r)['other.txt']
record('persistent_control', detected=True)

p,r=fresh(); before=digest(r); oldmode=(r/'other.txt').stat().st_mode
(r/'other.txt').chmod(0o755)
assert before==digest(r) and oldmode!=(r/'other.txt').stat().st_mode
record('chmod', hashes_equal=True, mode_changed=True)

p,r=fresh(); before=digest(r)
(r/'other.txt').write_text('temporary modification')
(p/'observer-copy').write_bytes((r/'other.txt').read_bytes())
(r/'other.txt').write_text('original')
assert before==digest(r) and (p/'observer-copy').read_text()!='original'
record('transient_write', hashes_equal=True, observer_retains_changed_bytes=True)

p,r=fresh(); outside=p/'outside.txt'; outside.write_text('original')
(r/'allowed.md').unlink(); os.link(outside,r/'allowed.md'); before=digest(r)
(r/'allowed.md').write_text('changed via allowed path')
assert before['other.txt']==digest(r)['other.txt'] and outside.read_text()!='original'
record('hardlink', outside_changed=True, non_target_hashes_equal=True)

p,r=fresh(); (r/'alias').symlink_to('other.txt'); before=digest(r)
(r/'alias').unlink(); (r/'alias').symlink_to('allowed.md')
assert before==digest(r) and os.readlink(r/'alias')=='allowed.md'
record('symlink_retarget', hashes_equal=True, link_target_changed=True)

p,r=fresh(); before=digest(r)
(r/'new.txt').write_text('new')
assert all(hashlib.sha256((r/k).read_bytes()).hexdigest()==v for k,v in before.items())
assert before!=digest(r)
record('new_path', original_paths_only_misses=True, rescan_names_detects=True)

p,r=fresh(); before=digest(r); backup=(r/'other.txt').read_bytes()
(r/'other.txt').write_text('concurrent user edit')
assert before!=digest(r)
(r/'other.txt').write_bytes(backup)
assert before==digest(r) and (r/'other.txt').read_text()!='concurrent user edit'
record('rollback_concurrent_edit', hashes_restored=True, user_edit_lost=True)

p,r=fresh(); outside=p/'reference.txt'; outside.write_text('synthetic reference')
empty=p/'empty'; empty.mkdir()
code='from pathlib import Path; import sys; sys.exit(0 if Path(sys.argv[1]).read_text()=="synthetic reference" else 2)'
a=subprocess.run([sys.executable,'-c',code,str(outside)],cwd=empty,env={'PATH':os.defpath},capture_output=True)
b=subprocess.run([sys.executable,'-c',code,str(p/'absent')],cwd=empty,env={'PATH':os.defpath},capture_output=True)
assert a.returncode==0 and b.returncode!=0
record('empty_cwd_env_clear', outside_read_succeeds=True, missing_control_fails=True)

# Keep this experiment local, bounded, and independent of any real engine.
child_code='''from pathlib import Path
import sys,time
ready,go,out=map(Path,sys.argv[1:])
ready.write_text('ready')
end=time.monotonic()+4
while not go.exists() and time.monotonic()<end: time.sleep(.01)
if go.exists(): out.write_text('late write')
'''
leader_code='''import subprocess,sys,time
from pathlib import Path
child=subprocess.Popen([sys.executable,'-c',sys.argv[1],*sys.argv[3:]],start_new_session=sys.argv[2]=='yes',stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
time.sleep(5)
'''
for detach in (False,True):
    p,r=fresh(); before=digest(r)
    ready,go,out=p/'ready',p/'go',r/'other.txt'
    leader=subprocess.Popen([sys.executable,'-c',leader_code,child_code,'yes' if detach else 'no',str(ready),str(go),str(out)],start_new_session=True,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    deadline=time.monotonic()+3
    while not ready.exists() and time.monotonic()<deadline: time.sleep(.01)
    assert ready.exists()
    os.killpg(leader.pid,signal.SIGKILL); leader.wait(timeout=3)
    assert before==digest(r)
    go.write_text('go'); time.sleep(.2)
    changed=before!=digest(r)
    assert changed==detach
    record('late_child_detached' if detach else 'late_child_same_group_control', hash_equal_at_check=True, changed_after_check=changed)

print(json.dumps(results,ensure_ascii=False,indent=2))
```
