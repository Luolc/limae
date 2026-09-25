# 运行记录：spec/rules.md 与三份文档，2026-09-24

| 项 | 值 |
| --- | --- |
| 基线 commit | `ce08b10` |
| 命令 | 逐个 `limae polish --share-repo-with-engine <file>`，全部跑完再一次 `limae --fix <四个文件>` |
| 引擎 | `auto` |
| prompt / 词典最后改动的 commit | `ce08b10` (`spec/polish/`) / `3bb36db` (`spec/lexicon/zh.toml`) |
| 目标 | 4 个文件，全部进仓 (`spec/rules.md` 第一次被结构核对拒绝、重跑一次后通过) |
| 改动量 | **两个口径都是** `before/` 到 `after/` (模型纯产物) 与 `before/` 到进仓文件 **19 增 19 删** —— 这一批没有手工修正，两个口径因此相等 |
| `--fix` 是否动过 | 否 —— `limae --fix` 对 polish 产物报 `OK: 4 file(s) clean`，无额外改动 |

## `spec/rules.md` 第一次被拒，重跑一次通过

第一次 `limae polish --share-repo-with-engine spec/rules.md` 输出：

```
not written: spec/rules.md: the rewrite changed the count of code fence lines 62 → 60; the file is kept, polish it again
```

按 brief 要求没有手工拼接，原样记在这里；退出码 0，文件保持基线不变。重跑一次，第二次输出 `polished: spec/rules.md`，结构核对通过 (标题 45 → 45、围栏 58 → 58、列表项 161 → 161，均相等)。这是 #200 那条写回前结构核对在真实运行里第一次拦下一次坏写回；产物为空、没有产生可比的废弃版本，因此没有 `discarded-runs` 条目。

## 结构核对 (标题 / 围栏 / 列表项 / 总行数)，全部四份

| 文件 | 标题 | 围栏 | 列表项 | 行数 |
| --- | --- | --- | --- | --- |
| `spec/rules.md` | 45 → 45 | 58 → 58 | 161 → 161 | 688 → 688 |
| `docs/research/claudish-and-ai-slop-survey.md` | 33 → 33 | 0 → 0 | 105 → 105 | 487 → 487 |
| `docs/research/zh-typography-guidelines-survey.md` | 35 → 35 | 0 → 0 | 43 → 43 | 460 → 460 |
| `docs/knowledge/release.md` | 25 → 25 | 2 → 2 | 22 → 22 | 376 → 376 |

`docs/knowledge/release.md` 是上一批 (2026-09-16) 被砍掉一半而撤回的那份，这次四项计数全部相等，没有重演那次故障。

## 反例原样保留：这批的第一次实战

四份文件里唯一属于反例类型的文字是 `docs/research/claudish-and-ai-slop-survey.md` 第 222 行「掘金直译术语」清单里引用的坏词 `` `gate` → 「门控」``，以及第 235 行摘要行的「门控 / 横切 / 一等公民 (掘金)」；`spec/rules.md` 第 662 行「词表只收……」那句里同样引用了「门控」作为**不收**的反例。**这三处模型一处都没动**，与上一批「门控」两处未动的读数一致。

## 手工修正

**这批零手工修正。** 逐份读过 diff，没有发现语义被改、反例被改、或「门」的比喻残留 (逐份 diff 见 PR)。

其中 `spec/rules.md` 与 `claudish-and-ai-slop-survey.md` 里出现的 `正本` → `源文件` / `以……为准`、`合同` → `约定`、`长出` → `归纳出` / `积累而成` 这些替换，都命中 `spec/lexicon/zh.toml` 里已收录的词条 (`正本`、`长出`)，且改法与词典 `examples` 里给的 before/after 完全一致 —— 这是词典生效，不是需要恢复的语义漂移。

`docs/knowledge/release.md` 与 `spec/rules.md`、`claudish-and-ai-slop-survey.md` 里的「门」全部按语境换成了「质量检查」「一项检查」「一条判据」「打包检查」，没有残留；「门槛」「专门」未出现在这四份文件里，无需处理。

## 裁决

留白。裁决人是用户；判据不在这里定义，用户怎么说就是什么。记裁决时写在下面，注明日期，照抄原话不要改写。
