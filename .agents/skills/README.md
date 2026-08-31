# 项目级 skill

本目录是本仓项目级 skill 的正本：每个 skill 一个子目录 `.agents/skills/<name>/SKILL.md`，`.claude/skills/<name>` 是指向它的逐 skill 目录级软链 (不链整个目录、不链 `SKILL.md` 文件)；不装到用户级目录。目前为空，审查标准用用户级 `pr-review` skill (`~/.agents/skills/pr-review`，machine-setup 分发)；本仓自有的审查 skill 起名 `mdlint-pr-review`。
