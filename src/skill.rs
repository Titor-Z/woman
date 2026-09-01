// skill.rs — 手册整理 skill 的读取与默认模板生成
// skill 文件位于 ~/.woman/skills/manual.md，包含「手册章节结构模板 + 给 AI 的整理指令」，
// 与 AI 的 system prompt 解耦，用户可自定义，多风格可扩展。
// 仅在 AI 需要整理手册时读取并注入 prompt，平时不占上下文。

use crate::config::Config;

/// 默认手册 skill 模板。
/// 采用 w3cschool / man 相融合的结构，产出**详细完整手册**（离线的完整参考）：
/// 用途 / 语法 / 全部选项（逐个）/ 示例 / 注意事项。
/// 并用「本地 --help 校对」指令：以本机选项为真值，在线选项标注「本机此版本不支持」。
const DEFAULT_SKILL: &str = r#"# 手册整理 Skill

你负责把一段"命令原始资料"整理成一份**详细完整、可直接离线参考**的 Markdown 手册。
原始资料可能包含两部分：
- **本地 `--help` / `/?` 输出**：本机真正支持的选项（真值）。
- **在线手册全文**（man.archlinux.org / MS Learn / Get-Help）：详细的说明来源。

## 章节结构模板

按以下章节组织（**没有内容的章节可省略**，不要输出空的章节标题）：

```markdown
---
title: <命令名>
source: ai-enhanced
generated: <今天日期 YYYY-MM-DD>
type: <coreutils | powershell | windows>
---

# <命令名>

## 用途
一段说明，讲清这个命令是做什么的、典型用在什么场景。

## 语法
基本用法，用代码块展示；有子命令/多参数组合时分别说明。

## 选项 / 参数
逐条列出该命令**全部**的选项与参数（不要因为冷门就省略），每条说明其作用：
- `-x, --xxx` — 选项作用
- （所有选项都列，用无序列表）


## 示例
尽量完整，覆盖常用场景和关键组合，每个示例配一行中文说明（代码块）。

## 注意事项
- 常见坑、与同类命令的区别、在 Windows 上的特殊行为（如 .exe 后缀、大小写差异）。
```

## 整理指令

1. 根据原始资料提炼，不要编造原始资料里没有的内容。
2. **终端友好排版**：避免表格和占用过宽的 Markdown，推荐用无序列表、加粗、代码块。
3. 全部使用中文说明。
4. YAML frontmatter 必须保留，`title` 填命令名，`generated` 填今天日期，`type` 填命令类型。
5. **完整性优先，作为离线手册**：列出命令的**全部**选项/参数，而非只挑常用的。原始资料里有的就都收录，逐条说明。
6. **本地选项校对（重要）**：
   - 以「本地 `--help` / `/?` 输出」里出现的选项为**本机真值**；把在线全文当作这些选项的**详细说明**来源。
   - 对本地支持、在线也有的选项：正常收录并写说明。
   - 对在线手册列出、但**本地 `--help` 没有出现**的选项：**保留该选项**，并标注 **『本机此版本不支持』**，让用户知道它存在但本机用不了；不要静默丢弃，也不要谎称支持。
   - 只给「在线全文」而没有本地输出时：正常收录全部选项，无需标注。
"#;

/// 读取手册整理 skill 内容；文件不存在时创建默认模板并返回。
pub fn load_manual_skill() -> String {
    let path = Config::skills_dir().join("manual.md");
    if let Ok(content) = std::fs::read_to_string(&path) {
        if !content.trim().is_empty() {
            return content;
        }
    }
    let _ = std::fs::write(&path, DEFAULT_SKILL);
    DEFAULT_SKILL.to_string()
}

/// 返回默认 skill 模板的原文（供 `init --reset` 重写默认模板）
pub fn default_skill() -> Result<String, std::io::Error> {
    Ok(DEFAULT_SKILL.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_skill_has_required_sections() {
        let skill = DEFAULT_SKILL;
        assert!(skill.contains("## 用途"));
        assert!(skill.contains("## 语法"));
        assert!(skill.contains("## 选项 / 参数"));
        assert!(skill.contains("## 示例"));
        assert!(skill.contains("## 注意事项"));
        assert!(skill.contains("ai-enhanced"));
        assert!(skill.contains("generated"));
    }

    #[test]
    fn default_skill_covers_local_option_validation() {
        let skill = DEFAULT_SKILL;
        // 必须包含「本地 --help 校对」指令与「本机此版本不支持」标注
        assert!(skill.contains("本地选项校对"));
        assert!(skill.contains("本机此版本不支持"));
        // 完整性优先关键词
        assert!(skill.contains("全部"));
    }
}
