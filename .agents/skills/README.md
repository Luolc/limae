# 项目级 skill

本目录是本仓项目级 skill 的正本：每个 skill 一个子目录 `.agents/skills/<name>/SKILL.md`，`.claude/skills/<name>` 是指向它的逐 skill 目录级软链 (不链整个目录、不链 `SKILL.md` 文件)；不装到用户级目录。目前只有 `limae-pr-review/` 一个：本仓的审查标准是用户级 `pr-review` skill (`~/.agents/skills/pr-review`，machine-setup 分发) 叠加它 —— 用户级写通用轨道、优先级与输出格式，它只写本仓的增量与加严。名字带 `limae-` 前缀是为了不遮蔽用户级那份。
