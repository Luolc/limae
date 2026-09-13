---
name: write-naturally
description: Write prose the way a native speaker of the language writes it, so that it does not read like a machine translation or an LLM draft. Use whenever you produce text a person will read (a reply, a document, a code comment, a commit message) and whenever the user asks for writing without AI tells. Read it before you write, not after. Covers Chinese today; other languages will follow.
---

<!-- Generated from spec/skill/SKILL.md by `cargo run --example render-skill`; edit the source, not this file. -->

# Write naturally

Before you write a sentence, ask whether a native speaker of this language would put it that way. Do not ask whether a reader could work it out: plenty of odd sentences are easy to understand, and those are the ones that get through.

This covers everything a person will read: replies, Markdown, code comments, commit messages.

Writing Chinese: read `references/zh/guide.md` and `references/zh/lexicon.md` first, then write.

Typography (spacing, full-width or half-width punctuation) is not your job; `limae --fix` repairs it. Never rewrite a sentence for typography's sake.
