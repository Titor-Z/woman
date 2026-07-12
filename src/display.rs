// display.rs — 终端直接输出渲染（非 TUI 场景）

/// 渲染纯文本
pub fn render_plain(text: &str) {
    println!("{}", text);
}

/// 渲染提示信息
pub fn render_hint(text: &str) {
    println!("💡 {}", text);
}

/// 渲染错误信息
pub fn render_error(text: &str) {
    eprintln!("错误：{}", text);
}

// ============================================================
// Markdown → ANSI 转换（轻量，不依赖第三方库）
// ============================================================

const BOLD: &str = "\x1b[1m";
const ITALIC: &str = "\x1b[3m";
const STRIKE: &str = "\x1b[9m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";

const RESET: &str = "\x1b[0m";

/// 将 Markdown 文本转换为带 ANSI 转义序列的终端友好格式
pub fn ansi_format(text: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    let lines: Vec<&str> = text.lines().collect();

    for li in 0..lines.len() {
        let raw = lines[li];
        let trimmed = raw.trim();

        // 代码块 fence
        if trimmed.starts_with("```") {
            in_code = !in_code;
            if !in_code {
                out.push('\n');
            }
            continue;
        }

        if in_code {
            out.push_str(&format!("{}{}{}\n", DIM, raw, RESET));
            continue;
        }

        // 空行
        if raw.trim().is_empty() {
            out.push('\n');
            continue;
        }

        // 块级处理
        let formatted = if trimmed.starts_with("###") {
            // 三级标题 — 加粗 + 斜体
            let content = &trimmed[3..].trim();
            format!("{}{}{}", BOLD, format_inline(content), RESET)
        } else if trimmed.starts_with("##") {
            let content = &trimmed[2..].trim();
            format!("{}{}{}", BOLD, format_inline(content), RESET)
        } else if trimmed.starts_with('#') {
            let content = &trimmed[1..].trim();
            format!("{}{}{}", BOLD, format_inline(content), RESET)
        } else if trimmed.starts_with("> ") || trimmed == ">" {
            let content = if trimmed.len() > 2 { &trimmed[2..] } else { "" };
            format!("{0}│{2}{1}", DIM, format_inline(content), RESET)
        } else if is_hrule(trimmed) && raw.len() >= 3 {
            format!("{}{}{}", DIM, "─".repeat(raw.len()), RESET)
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            // 无序列表
            format!("• {}", format_inline(&trimmed[2..]))
        } else if trimmed.starts_with("1.") || trimmed.starts_with("2.")
            || trimmed.starts_with("3.") || trimmed.starts_with("4.")
            || trimmed.starts_with("5.") || trimmed.starts_with("6.")
            || trimmed.starts_with("7.") || trimmed.starts_with("8.")
            || trimmed.starts_with("9.") || trimmed.starts_with("0.")
        {
            // 有序列表
            let dot = trimmed.find('.').unwrap_or(1);
            format!("{} {}", &trimmed[..=dot], format_inline(&trimmed[dot+1..].trim()))
        } else {
            format_inline(raw)
        };

        out.push_str(&formatted);
        out.push('\n');
    }

    out
}

/// 判断是否为水平线
fn is_hrule(s: &str) -> bool {
    let no_spaces: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if no_spaces.len() < 3 {
        return false;
    }
    let first = no_spaces.chars().next().unwrap();
    (first == '-' || first == '*' || first == '_')
        && no_spaces.chars().all(|c| c == first)
}

/// 行内格式化：`code` **加粗** *斜体* ~~删除线~~ [文字](链接)
fn format_inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < chars.len() {
        // 转义：\*
        if chars[i] == '\\' && i + 1 < chars.len() && "`*~[".contains(chars[i + 1]) {
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }

        // `code`
        if chars[i] == '`' {
            if let Some(end) = find_closing(&chars, i + 1, "`") {
                let inner: String = chars[i + 1..end].iter().collect();
                out.push_str(&format!("{}{}{}", RED, inner, RESET));
                i = end + 1;
                continue;
            }
        }

        // ~~删除线~~
        if chars[i] == '~' && i + 1 < chars.len() && chars[i + 1] == '~' {
            if let Some(end) = find_closing(&chars, i + 2, "~~") {
                let inner: String = chars[i + 2..end].iter().collect();
                out.push_str(&format!("{}{}{}", STRIKE, format_inline(&inner), RESET));
                i = end + 2;
                continue;
            }
        }

        // **加粗**
        if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            if let Some(end) = find_closing(&chars, i + 2, "**") {
                let inner: String = chars[i + 2..end].iter().collect();
                out.push_str(&format!("{}{}{}", BOLD, format_inline(&inner), RESET));
                i = end + 2;
                continue;
            }
        }

        // *斜体*（只在前后字符不是 * 时匹配）
        if chars[i] == '*' && (i == 0 || chars[i - 1] != '*')
            && (i + 1 >= chars.len() || chars[i + 1] != '*')
        {
            if let Some(end) = find_closing(&chars, i + 1, "*") {
                // 确保闭合的 * 后面没有 *
                if end + 1 >= chars.len() || chars[end + 1] != '*' {
                    let inner: String = chars[i + 1..end].iter().collect();
                    out.push_str(&format!("{}{}{}", ITALIC, format_inline(&inner), RESET));
                    i = end + 1;
                    continue;
                }
            }
        }

        // [文字](链接)
        if chars[i] == '[' {
            let be = find_char(&chars, i + 1, ']');
            let ps = be.and_then(|b| {
                if b + 1 < chars.len() && chars[b + 1] == '(' {
                    Some(b + 1)
                } else {
                    None
                }
            });
            let pe = ps.and_then(|p| find_char(&chars, p + 1, ')'));

            if let (Some(be), Some(_), Some(pe)) = (be, ps, pe) {
                let text: String = chars[i + 1..be].iter().collect();
                out.push_str(&format_inline(&text));
                i = pe + 1;
                continue;
            }
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

/// 在字符数组中查找模式，返回匹配位置
fn find_closing(chars: &[char], start: usize, pattern: &str) -> Option<usize> {
    let p: Vec<char> = pattern.chars().collect();
    let mut i = start;
    while i + p.len() <= chars.len() {
        if chars[i..i + p.len()] == p[..] {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// 查找单个字符
fn find_char(chars: &[char], start: usize, target: char) -> Option<usize> {
    chars[start..].iter().position(|&c| c == target).map(|p| start + p)
}

/// 先 ansi_format 再 println（快捷函数）
pub fn render_markdown(text: &str) {
    println!("{}", ansi_format(text));
}
