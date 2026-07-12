// fetch.rs — 在线抓取和系统命令检测
// HTTP 请求通过 curl.exe 实现（避免 rustls 编译问题）

use std::process::Command;

// ============================================================
// HTML 工具函数（用于 MS Learn 页面）
// ============================================================

/// HTML 实体解码映射
const ENTITIES: &[(&str, char)] = &[
    ("&amp;", '&'),
    ("&lt;", '<'),
    ("&gt;", '>'),
    ("&quot;", '"'),
    ("&#39;", '\''),
    ("&#x27;", '\''),
    ("&#x60;", '`'),
    ("&nbsp;", ' '),
];

/// 从 HTML 中提取纯文本内容
fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;
    let mut in_tag = false;
    let mut in_comment = false;
    let mut block_break = false;

    while i < chars.len() {
        // 注释
        if !in_tag && !in_comment && chars[i] == '<'
            && chars.get(i + 1) == Some(&'!')
            && chars.get(i + 2) == Some(&'-')
            && chars.get(i + 3) == Some(&'-')
        {
            in_comment = true;
            i += 4;
            continue;
        }

        if in_comment {
            if chars[i] == '-' && chars.get(i + 1) == Some(&'-') && chars.get(i + 2) == Some(&'>') {
                in_comment = false;
                i += 3;
                continue;
            }
            i += 1;
            continue;
        }

        // 标签内
        if in_tag {
            if chars[i] == '>' {
                in_tag = false;
                if block_break {
                    out.push('\n');
                    block_break = false;
                }
            }
            // 检测块级闭标签（跳过 < 后的 /）
            if i > 0 && chars[i - 1] == '<' && (chars[i] == '/' || chars[i] == 'b') {
                let rest: String = chars[i..].iter().take(6).collect();
                let rest_lower = rest.to_lowercase();
                if rest_lower.starts_with("/p") || rest_lower.starts_with("/d")
                    || rest_lower.starts_with("/h") || rest_lower.starts_with("/l")
                    || rest_lower.starts_with("/t") || rest_lower.starts_with("/s")
                    || rest_lower.starts_with("br") || rest_lower.starts_with("/a")
                {
                    block_break = true;
                }
            }
            i += 1;
            continue;
        }

        // 标签开始
        if chars[i] == '<' {
            in_tag = true;
            i += 1;
            continue;
        }

        // HTML 实体解码
        if chars[i] == '&' {
            let mut decoded = false;
            for &(entity, ch) in ENTITIES {
                if chars[i..].iter().take(entity.len()).copied().eq(entity.chars()) {
                    out.push(ch);
                    i += entity.len();
                    decoded = true;
                    break;
                }
            }
            if decoded { continue; }
            // 数值实体 &#NNN; / &#xHH;
            if chars.get(i + 1) == Some(&'#') {
                let end = chars[i..].iter().position(|&c| c == ';');
                if let Some(pos) = end {
                    i += pos + 1;
                    continue;
                }
            }
        }

        out.push(chars[i]);
        i += 1;
    }

    // 合并连续空行
    let mut result = String::new();
    let mut prev_blank = false;
    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_blank {
                result.push_str("\n\n");
                prev_blank = true;
            }
        } else {
            result.push_str(trimmed);
            result.push('\n');
            prev_blank = false;
        }
    }

    result.trim().to_string()
}

/// 在 HTML 中查找容器标签内容
fn extract_between(html: &str, open_tag: &str, close_tag: &str) -> Option<String> {
    let start = html.find(open_tag)?;
    let content_start = html[start..].find('>')? + 1;
    let body = &html[start + content_start..];
    let end = body.find(close_tag)?;
    Some(body[..end].to_string())
}

/// 从 MS Learn HTML 页面提取正文内容
fn extract_mslearn_body(html: &str) -> Option<String> {
    // MS Learn 正文在 <article ...>...</article> 中
    let article = extract_between(html, "<article", "</article>")
        .or_else(|| extract_between(html, "<main", "</main>"))
        .or_else(|| extract_between(html, "<div class=\"content\"", "</div>"))?;

    let text = html_to_text(&article);

    // 找到正文起点：跳过导航面包屑等无用内容
    let markers = ["## ", "### ", "适用于", "命令参考", "语法"];
    let first_content = text.find(|c: char| c != '\n').unwrap_or(0);
    let cut = markers.iter()
        .filter_map(|m| text[first_content..].find(m).map(|p| first_content + p))
        .min()
        .unwrap_or(first_content);

    let cleaned = text[cut..].trim().to_string();
    if cleaned.len() > 50 { Some(cleaned) } else { None }
}

// ============================================================
// learn.microsoft.com 抓取
// ============================================================

/// 从 Microsoft Learn 获取 Windows 命令文档
pub fn fetch_from_mslearn(name: &str) -> Result<String, String> {
    let url = format!("https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/{name}");
    let html = fetch_url(&url)?;

    match extract_mslearn_body(&html) {
        Some(text) => Ok(text),
        None => Err(format!("在 Microsoft Learn 上未找到 '{name}' 的文档")),
    }
}

// ============================================================
// 命令检测
// ============================================================

/// 检查命令是否存在于系统中
pub fn command_exists(name: &str) -> bool {
    Command::new("where.exe")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 检测是否为 Windows 原生命令
/// System32 下的 exe（icacls, findstr 等）或 cmd 内置命令（dir, type 等）
pub fn is_windows_command(name: &str) -> bool {
    if let Ok(output) = Command::new("where.exe").arg(name).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout);
            return path.to_lowercase().contains("system32");
        }
    }
    // 不在 System32 中（或找不到），尝试 cmd 内置命令
    if let Ok(output) = Command::new("cmd").args(["/c", name, "/?"]).output() {
        let combined = format!("{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr));
        let trimmed = combined.trim();
        if !trimmed.is_empty()
            && !trimmed.contains("is not recognized")
            && !trimmed.contains("not recognized as")
        {
            return true;
        }
    }
    false
}

// ============================================================
// --help 获取
// ============================================================

/// 执行 <name> --help 获取帮助文本
/// 优先尝试 --help，失败后尝试 cmd /c <name> /?（Windows 原生命令）
pub fn run_help(name: &str) -> Option<String> {
    // 尝试 --help（部分命令输出到 stderr）
    let output = Command::new(name)
        .arg("--help")
        .output()
        .ok()?;
    if output.status.success() {
        let text = capture_output(&output);
        if !text.is_empty() {
            return Some(text);
        }
    }

    // 尝试 /?（Windows 原生命令，如 dir, icacls）
    let output = Command::new("cmd")
        .args(["/c", name, "/?"])
        .output()
        .ok()?;
    if output.status.success() {
        let text = capture_output(&output);
        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}

/// 从命令输出中提取文本，先 stdout 后 stderr
fn capture_output(output: &std::process::Output) -> String {
    let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !out.is_empty() {
        return out;
    }
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

// ============================================================
// HTTP 请求（通过 curl.exe，支持 HTTPS）
// ============================================================

/// 发送 HTTP GET 请求并返回响应文本
fn fetch_url(url: &str) -> Result<String, String> {
    let output = Command::new("curl.exe")
        .args([
            "-sS",          // 静默模式，显示错误
            "-L",           // 跟随重定向
            "-A",           // User-Agent
            "woman/0.1.0",
            url,
        ])
        .output()
        .map_err(|e| format!("无法执行 curl.exe：{}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("请求失败：{}", err.trim()));
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if text.trim().is_empty() {
        return Err(format!("空响应：{}", url));
    }

    Ok(text)
}

// ============================================================
// man.archlinux.org 抓取
// ============================================================

/// 从 man.archlinux.org 获取指定 section 的手册纯文本
fn fetch_from_archlinux_section(name: &str, section: u32) -> Result<String, String> {
    let url = format!("https://man.archlinux.org/man/{}.{}.txt", name, section);
    let text = fetch_url(&url)?;
    // archlinux 对不存在的页面返回 404 HTML（HTTP 200 但内容是 HTML），需检查内容类型
    let trimmed = text.trim_start();
    if trimmed.starts_with("<!DOCTYPE")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<head")
        || text.contains("404 — Page not found")
    {
        return Err(format!("在 man.archlinux.org 上未找到 '{name}' 的手册"));
    }
    Ok(text)
}

/// 自动尝试多个 section，返回第一个成功的
pub fn fetch_from_archlinux(name: &str) -> Result<String, String> {
    // 常见 section：1=用户命令, 8=系统管理, 5=配置文件, 7=杂项, 3=库函数
    let sections = [1u32, 8, 5, 7, 3];
    for &sec in &sections {
        let result = fetch_from_archlinux_section(name, sec);
        if result.is_ok() {
            return result;
        }
    }
    Err(format!("在 man.archlinux.org 上未找到 '{}' 的手册", name))
}
