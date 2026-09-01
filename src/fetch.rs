// fetch.rs — 在线抓取和系统命令检测
// HTTP 请求通过 curl 实现（避免 rustls 编译问题）；curl/shell/命令定位按平台抽象层取。

use std::process::Command;

// Config 仅用于 Windows coreutils 清单路径（coreutils_list），Unix 上无需引入
#[cfg(target_os = "windows")]
use crate::config::Config;
use crate::platform;

// ============================================================
// 命令类型
// ============================================================

/// 命令类型，决定取哪个源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandType {
    /// GNU coreutils 命令（本机 coreutils.exe 提供）
    Coreutils,
    /// PowerShell 指令（如 Get-ChildItem）
    Powershell,
    /// Windows 原生命令（cmd.exe / System32）
    Windows,
    /// 其他（无法分类）
    Unknown,
}

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
        if !in_tag
            && !in_comment
            && chars[i] == '<'
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
                if rest_lower.starts_with("/p")
                    || rest_lower.starts_with("/d")
                    || rest_lower.starts_with("/h")
                    || rest_lower.starts_with("/l")
                    || rest_lower.starts_with("/t")
                    || rest_lower.starts_with("/s")
                    || rest_lower.starts_with("br")
                    || rest_lower.starts_with("/a")
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
                if chars[i..]
                    .iter()
                    .take(entity.len())
                    .copied()
                    .eq(entity.chars())
                {
                    out.push(ch);
                    i += entity.len();
                    decoded = true;
                    break;
                }
            }
            if decoded {
                continue;
            }
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
    let cut = markers
        .iter()
        .filter_map(|m| text[first_content..].find(m).map(|p| first_content + p))
        .min()
        .unwrap_or(first_content);

    let cleaned = text[cut..].trim().to_string();
    if cleaned.len() > 50 {
        Some(cleaned)
    } else {
        None
    }
}

// ============================================================
// learn.microsoft.com 抓取
// ============================================================

/// 从 Microsoft Learn 获取 Windows 命令文档
pub fn fetch_from_mslearn(name: &str) -> Result<String, String> {
    let url = format!(
        "https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/{name}"
    );
    let html = fetch_url(&url)?;

    match extract_mslearn_body(&html) {
        Some(text) => Ok(text),
        None => Err(format!("在 Microsoft Learn 上未找到 '{name}' 的文档")),
    }
}

// ============================================================
// 命令检测
// ============================================================

/// 检测是否为 Windows 原生命令（仅 Windows）
/// System32 下的 exe（icacls, findstr 等）或 cmd 内置命令（dir, type 等）
#[cfg(target_os = "windows")]
pub fn is_windows_command(name: &str) -> bool {
    if let Ok(output) = Command::new("where.exe").arg(name).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout);
            return path.to_lowercase().contains("system32");
        }
    }
    // 不在 System32 中（或找不到），尝试 cmd 内置命令
    if let Ok(output) = Command::new("cmd").args(["/c", name, "/?"]).output() {
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
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
// coreutils 命令清单缓存 + 命令分类
// ============================================================

/// 获取 coreutils 命令清单（优先用缓存，缓存不存在则运行 coreutils.exe --list-raw 生成）
/// 清单缓存于 ~/.woman/coreutils-list.txt，避免每次分类都 spawn 子进程
/// Unix 上系统二进制本身就是 man 目标，无需 coreutils 清单（返回空，归类走系统命令路径）
#[cfg(target_os = "windows")]
fn coreutils_list() -> Vec<String> {
    let cache_path = Config::coreutils_list_path();
    if let Ok(content) = std::fs::read_to_string(&cache_path) {
        let list: Vec<String> = content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if !list.is_empty() {
            return list;
        }
    }

    // 生成清单
    let mut list: Vec<String> = Vec::new();
    if let Ok(output) = Command::new("coreutils.exe").arg("--list-raw").output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            list = text
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
        }
    }

    // 写入缓存（为空则跳过，避免覆盖为空）
    if !list.is_empty() {
        let _ = std::fs::write(
            &cache_path,
            list.join("\n") + "\n",
        );
    }
    list
}

/// 分类命令：coreutils / powershell / windows，返回所有命中的类型
///
/// - `.exe` 后缀先剥掉再判断（`ls.exe` / `ls` 均归同一命令）——仅 Windows
/// - 默认优先级顺序在调用方体现（coreutils > windows > powershell）
/// - Unix：系统二进制本身就是 man 目标，存在即归 coreutils（代表「系统命令」），
///   不存在则归 Unknown；无 PowerShell / Windows 原生分类
pub fn classify_command(name: &str) -> Vec<CommandType> {
    #[cfg(target_os = "windows")]
    {
        let mut types: Vec<CommandType> = Vec::new();

        // 去掉 .exe 后缀统一判断
        let base = name
            .strip_suffix(".exe")
            .or_else(|| name.strip_suffix(".EXE"))
            .unwrap_or(name);

        // PowerShell 指令：形如 Get-Xxx 或含有字母 got-Xxx (cmdlet 命名)
        if is_powershell_cmdlet(base) {
            types.push(CommandType::Powershell);
        }

        // coreutils：查清单
        if coreutils_list().iter().any(|c| c == base) {
            types.push(CommandType::Coreutils);
        }

        // coreutils 判断还可用本机命令是否存在兜底（清单可能不全）
        if !types.contains(&CommandType::Coreutils) {
            // 若能通过 where 找到且位于 coreutils bin 下，也归 coreutils
            if is_in_coreutils_dir(base) {
                types.push(CommandType::Coreutils);
            }
        }

        // Windows 原生
        if is_windows_command(base) && !types.contains(&CommandType::Coreutils) {
            types.push(CommandType::Windows);
        }

        if types.is_empty() {
            types.push(CommandType::Unknown);
        }

        types
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut types: Vec<CommandType> = Vec::new();
        // 系统命令存在（which 命中）→ 归 Coreutils（代表「系统命令 / man 目标」）
        if platform::command_exists(name) {
            types.push(CommandType::Coreutils);
        }
        if types.is_empty() {
            types.push(CommandType::Unknown);
        }
        types
    }
}

/// 判断是否 PowerShell cmdlet（命名规范：Verb-Noun，如 Get-ChildItem、Select-String，仅 Windows）
#[cfg(target_os = "windows")]
fn is_powershell_cmdlet(name: &str) -> bool {
    // PowerShell 指令形如 `Get-*`、`Set-*`、`Select-*` 等，含连字符且首词常见动词
    let lower = name.to_lowercase();
    let some_verb = [
        "get", "set", "select", "where", "foreach", "out", "write", "read", "format", "new",
        "remove", "add", "copy", "move", "test", "measure", "compare", "sort", "group", "invoke",
        "convert", "import", "export", "start", "stop", "restart", "resume", "wait", "clear",
        "enable", "disable", "rename", "show", "tab", "trace", "trap", "update", "use", "with",
    ];
    if let Some((verb, _)) = lower.split_once('-') {
        return some_verb.contains(&verb);
    }
    false
}

/// 判断命令是否位于 coreutils bin 目录（清单兜底用，仅 Windows）
#[cfg(target_os = "windows")]
fn is_in_coreutils_dir(name: &str) -> bool {
    if let Ok(output) = Command::new("where.exe").arg(name).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout);
            return path.to_lowercase().contains("coreutils");
        }
    }
    false
}

// ============================================================
// 源选择：根据命令类型返回「双资料」（本地 --help + 在线全文）
// ============================================================

/// 双资料取源结果：同时携带「本地 --help（选项真值）」与「在线全文（详细说明）」。
///
/// - `local_help`：本地命令 `--help` / `/?` 输出，是本机真正支持的选项集，作为 AI 校对基准。
/// - `online`：在线手册全文（archlinux / MS Learn / Get-Help），提供详细说明。
/// - `combined`：两者合并的完整原始资料，存 cache（离线可用）并在无 AI 时直接展示。
pub struct FetchedSource {
    /// 主来源标签，用于 cache frontmatter 与徽标
    pub label: String,
    /// 本地命令 --help //? 输出（选项真值来源）
    pub local_help: Option<String>,
    /// 在线手册全文（详细说明来源）
    pub online: Option<String>,
    /// 合并后的完整原始资料（本地 + 在线）
    pub combined: String,
}

/// 把本地 --help 与在线全文合并成一段带标题区分的完整原始资料。
fn merge_combined(local_help: &Option<String>, online: &Option<String>, online_label: &str) -> String {
    let mut out = String::new();
    if let Some(lh) = local_help {
        out.push_str("==== 本地 --help / /? 输出（本机真值）====\n```\n");
        out.push_str(lh.trim());
        out.push_str("\n```\n\n");
    }
    if let Some(on) = online {
        out.push_str(&format!("==== 在线手册全文（{online_label}）====\n"));
        out.push_str(on.trim());
        out.push('\n');
    }
    out
}

/// 选择一个命令的最佳取源方式，返回双资料结果
///
/// - coreutils：**同时**取本地 `--help`（真值）与 archlinux 全文（详情），保证离线有详细手册。
/// - windows：同时取本地 `/?` 与 MS Learn 全文。
/// - powershell / unknown：尽力多取，至少保证有可用内容。
pub fn fetch_source(name: &str, cmd_type: CommandType) -> Result<FetchedSource, String> {
    match cmd_type {
        CommandType::Coreutils => {
            // 本地 --help（真值）+ archlinux 全文（详情），两者都要，至少取到其一
            let local_help = run_help(name);
            let online = fetch_from_archlinux(name).ok();
            let label = if online.is_some() {
                format!("coreutils `{} --help` + man.archlinux.org", name)
            } else {
                format!("coreutils `{} --help`", name)
            };
            let combined = merge_combined(&local_help, &online, "man.archlinux.org");
            if combined.trim().is_empty() {
                // 本地与在线都没取到，回退平台兜底：Windows→Get-Help，Unix→man
                #[cfg(target_os = "windows")]
                {
                    let gh = fetch_from_gethelp(name)?;
                    return Ok(FetchedSource {
                        label: format!("Get-Help {}", name),
                        local_help: None,
                        online: None,
                        combined: gh,
                    });
                }
                #[cfg(not(target_os = "windows"))]
                {
                    return Err(format!("无法获取 '{name}' 的文档（--help / archlinux / man 均无）"));
                }
            }
            Ok(FetchedSource {
                label,
                local_help,
                online,
                combined,
            })
        }
        CommandType::Powershell => {
            // PowerShell：Get-Help（单一源，无双资料）
            let text = fetch_from_gethelp(name)?;
            Ok(FetchedSource {
                label: format!("Get-Help {}", name),
                local_help: None,
                online: None,
                combined: text,
            })
        }
        CommandType::Windows => {
            // 本地 /?（真值）+ MS Learn 全文（详情）
            let local_help = run_help(name);
            let online = fetch_from_mslearn(name).ok();
            let label = if online.is_some() {
                format!("cmd `{} /?` + learn.microsoft.com", name)
            } else {
                format!("cmd `{} /?`", name)
            };
            let combined = merge_combined(&local_help, &online, "learn.microsoft.com");
            if combined.trim().is_empty() {
                return Err(format!("无法获取 '{name}' 的文档"));
            }
            Ok(FetchedSource {
                label,
                local_help,
                online,
                combined,
            })
        }
        CommandType::Unknown => {
            // 未知类型：尽力多取（--help + 在线全文 + 平台兜底）
            let local_help = run_help(name);
            let mut online = fetch_from_archlinux(name).ok();
            let mut online_label = "man.archlinux.org";
            let mut fallback: Option<String> = None;
            #[cfg(target_os = "windows")]
            {
                // Windows 第二在线源 + 兜底：MS Learn / Get-Help
                if online.is_none() {
                    online = fetch_from_mslearn(name).ok();
                    online_label = "learn.microsoft.com";
                }
                if local_help.is_none() && online.is_none() {
                    fallback = fetch_from_gethelp(name).ok();
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                // Unix 第二在线源 + 兜底：系统 man
                if online.is_none() {
                    online = fetch_from_man(name);
                    online_label = "系统 man";
                }
                if local_help.is_none() && online.is_none() {
                    fallback = fetch_from_man(name);
                }
            }

            let mut combined = merge_combined(&local_help, &online, online_label);
            if let Some(gh) = fallback.as_ref() {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(&format!(
                    "==== 在线手册全文（{online_label}）====\n{gh}"
                ));
            }
            if combined.trim().is_empty() {
                return Err(format!("无法获取 '{name}' 的文档"));
            }
            let label = if online.is_some() {
                format!("`{} --help` + {}", name, online_label)
            } else {
                format!("`{} --help` / {}", name, online_label)
            };
            Ok(FetchedSource {
                label,
                local_help,
                online,
                combined,
            })
        }
    }
}

// ============================================================
// PowerShell Get-Help 抓取
// ============================================================

/// 通过 PowerShell `Get-Help <name> -Full` 获取命令文档（仅 Windows）
#[cfg(target_os = "windows")]
pub fn fetch_from_gethelp(name: &str) -> Result<String, String> {
    let script = format!(
        "Get-Help {} -Full 4>&1 | Out-String -Width 200",
        name
    );
    let output = Command::new("pwsh")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|e| format!("无法执行 pwsh：{}", e))?;

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let trimmed = text.trim().to_string();

    // 判断是否找到（未找到通常会输出 Find Help 相关提示或为空）
    if trimmed.is_empty()
        || trimmed.contains("No help found")
        || trimmed.contains("Get-Help : Cannot find")
        || trimmed.contains("not recognized")
    {
        return Err(format!("PowerShell 中未找到 '{}' 的帮助", name));
    }
    Ok(trimmed)
}

/// Unix 上没有 PowerShell，Get-Help 不可用（调用方在非 Windows 上不应走到这里）
#[cfg(not(target_os = "windows"))]
pub fn fetch_from_gethelp(name: &str) -> Result<String, String> {
    Err(format!("Get-Help 仅 Windows 可用（{name}）"))
}

// ============================================================
// 系统 man 手册抓取（Unix：man / whatis）
// ============================================================

/// 通过系统 `man <name>` 抓取本地手册（Unix 系统自带 man）。
/// 失败（未安装 man 页或命令不存在）时返回 None。
#[cfg(not(target_os = "windows"))]
pub fn fetch_from_man(name: &str) -> Option<String> {
    // 依次尝试 man 与 whatis（whatis 单行摘要更轻量）
    for bin in ["man", "whatis"] {
        if let Ok(output) = Command::new(bin).arg(name).output() {
            if output.status.success() {
                let text = capture_output(&output);
                if !text.trim().is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

/// Windows 上没有系统 man，返回 None（Windows 编译时无调用点，故允许死代码）
#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn fetch_from_man(_name: &str) -> Option<String> {
    None
}


// ============================================================
// --help 获取
// ============================================================

/// 执行 <name> --help 获取帮助文本
/// Windows：失败后尝试 cmd /c <name> /?（原生命令）
/// Unix：失败后尝试系统 man / whatis
pub fn run_help(name: &str) -> Option<String> {
    // 尝试 --help（部分命令输出到 stderr）
    let output = Command::new(name).arg("--help").output().ok()?;
    if output.status.success() {
        let text = capture_output(&output);
        if !text.is_empty() {
            return Some(text);
        }
    }

    // 平台相关回退
    #[cfg(target_os = "windows")]
    {
        // Windows 原生命令（如 dir, icacls）用 `/?`
        let output = Command::new("cmd").args(["/c", name, "/?"]).output().ok()?;
        if output.status.success() {
            let text = capture_output(&output);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Unix 系统命令用 man / whatis
        if let Some(t) = fetch_from_man(name) {
            if !t.trim().is_empty() {
                return Some(t);
            }
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
    let output = Command::new(platform::curl_bin())
        .args([
            "-sS", // 静默模式，显示错误
            "-L",  // 跟随重定向
            "-A",  // User-Agent
            "woman/0.1.0",
            url,
        ])
        .output()
        .map_err(|e| format!("无法执行 {}：{}", platform::curl_bin(), e))?;

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
