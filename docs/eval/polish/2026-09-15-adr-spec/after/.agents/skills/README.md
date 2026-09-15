# 项目级 skill

本目录存放本仓项目级 skill：每个 skill 一个子目录 `.agents/skills/<name>/SKILL.md`，`.claude/skills/<name>` 是指向它的逐 skill 目录级软链接 (不链接整个目录，也不链接 `SKILL.md` 文件)；不安装到用户级目录。目前只有 `limae-pr-review/`：本仓的审查标准由用户级 `pr-review` skill (`~/.agents/skills/pr-review`，由 machine-setup 分发) 和它共同构成 —— 用户级定义通用轨道、优先级与输出格式，它只补充本仓的规则并加严。名称带 `limae-` 前缀，避免遮蔽用户级 skill。
