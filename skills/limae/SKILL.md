---
name: limae
description: Write Chinese technical prose the way a native speaker writes it, not the way a machine translation or an LLM draft reads. Use when drafting or revising anything a person will read (replies, Markdown, code comments, commit messages) in a repository that uses limae, or whenever the user asks for writing without AI tells. Other languages will follow.
---

<!-- Generated from spec/skill/SKILL.md by `cargo run --example render-skill`; edit the source, not this file. -->

# limae

Before you write a sentence, ask whether a native speaker of this language would put it that way. Do not ask whether a reader could work it out: plenty of odd sentences are easy to understand, and those are the ones that get through.

This covers everything a person will read: replies, Markdown, code comments, commit messages.

Writing Chinese: read `references/zh/guide.md` and `references/zh/lexicon.md` first, then write.

Typography (spacing, full-width or half-width punctuation) is not your job; `limae --fix` repairs it. Never rewrite a sentence for typography's sake.
