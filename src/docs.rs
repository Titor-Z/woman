// docs.rs — 管理 ~/.woman/docs/ 和 ~/.woman/cache/ 目录
// 负责文档的读取、元信息解析、来源标注、缓存读写

use std::path::{Path, PathBuf};
use std::fs;
use std::time::SystemTime;

// ============================================================
// 文档元信息
// ============================================================

/// 文档的元信息，从 YAML frontmatter 解析
pub struct DocMeta {
    /// 文档标题
    pub title: Option<String>,
    /// 来源："manual"（人工）| "ai-generated"（AI 生成）| "cache"（缓存）| "help"（--help）
    pub source: Option<String>,
    /// 生成/抓取日期
    pub fetched: Option<String>,
    /// 工具版本
    pub tool_version: Option<String>,
}

impl DocMeta {
    /// 创建一个带有来源标记的默认元信息
    pub fn with_source(source: &str) -> DocMeta {
        DocMeta {
            title: None,
            source: Some(source.to_string()),
            fetched: None,
            tool_version: None,
        }
    }
}

// ============================================================
// 文档对象
// ============================================================

/// 完整的文档，包含元信息和正文
pub struct Doc {
    pub meta: DocMeta,
    pub body: String,
    pub path: Option<PathBuf>,
}

impl Doc {
    /// 从文件读取文档（自动解析 frontmatter）
    pub fn from_file(path: &Path) -> Option<Doc> {
        let content = fs::read_to_string(path).ok()?;
        let (meta, body) = Self::parse_frontmatter(&content);
        Some(Doc {
            meta,
            body,
            path: Some(path.to_path_buf()),
        })
    }

    /// 从 --help 原始输出创建文档对象
    pub fn from_help(name: &str, help_text: &str) -> Doc {
        let body = format!("`{} --help` 输出：\n\n```\n{}\n```", name, help_text.trim());
        Doc {
            meta: DocMeta::with_source("help"),
            body,
            path: None,
        }
    }

    /// 从缓存内容创建文档对象
    pub fn from_cache(content: &str, meta: DocMeta) -> Doc {
        Doc {
            meta,
            body: content.to_string(),
            path: None,
        }
    }

    /// 解析 YAML frontmatter
    /// 格式：---\nkey: value\nkey: value\n---
    fn parse_frontmatter(content: &str) -> (DocMeta, String) {
        let content = content.trim_start();

        // 检查是否以 --- 开头
        if !content.starts_with("---") {
            return (DocMeta::with_source("manual"), content.to_string());
        }

        // 找到第二个 ---
        let rest = &content[3..];
        let end = rest.find("\n---").unwrap_or(rest.len());
        let front_raw = &rest[..end];
        let body_start = if end + 4 < rest.len() { end + 4 } else { rest.len() };
        let body = rest[body_start..].trim().to_string();

        // 解析键值对
        let mut meta = DocMeta::with_source("manual");
        for line in front_raw.lines() {
            let line = line.trim();
            if let Some((key, val)) = line.split_once(':') {
                let val = val.trim().to_string();
                match key.trim() {
                    "title" => meta.title = Some(val),
                    "source" => meta.source = Some(val),
                    "fetched" | "generated" => meta.fetched = Some(val),
                    "tool_version" => meta.tool_version = Some(val),
                    _ => {}
                }
            }
        }

        (meta, body)
    }

    /// 生成来源徽标（显示在文档顶部）
    pub fn source_badge(&self) -> String {
        let source = self.meta.source.as_deref().unwrap_or("unknown");
        match source {
            "manual" => {
                let date = self.meta.fetched.as_deref().unwrap_or("");
                if date.is_empty() {
                    "📝 人工编写".to_string()
                } else {
                    format!("📝 人工编写 · {}", date)
                }
            }
            "ai-generated" => {
                let date = self.meta.fetched.as_deref().unwrap_or("");
                if date.is_empty() {
                    "🤖 AI 生成".to_string()
                } else {
                    format!("🤖 AI 生成 · {}", date)
                }
            }
            "cache" => {
                let source_url = self.meta.title.as_deref().unwrap_or("在线源");
                let date = self.meta.fetched.as_deref().unwrap_or("");
                if date.is_empty() {
                    format!("📦 缓存 · {}", source_url)
                } else {
                    format!("📦 缓存 · {} · {}", source_url, date)
                }
            }
            "help" => "⚡ --help 原始输出".to_string(),
            _ => format!("🔗 {}", source),
        }
    }
}

// ============================================================
// 文档目录管理
// ============================================================

/// 获取 woman 家目录路径
fn home_dir() -> PathBuf {
    let home = dirs::home_dir().expect("无法获取用户主目录");
    home.join(".woman")
}

/// 在 docs/ 目录下查找文档
pub fn find_in_docs(name: &str) -> Option<Doc> {
    let path = home_dir().join("docs").join(format!("{}.md", name));
    Doc::from_file(&path)
}

/// 在 cache/ 目录下查找文档
pub fn find_in_cache(name: &str) -> Option<Doc> {
    let path = home_dir().join("cache").join(format!("{}.md", name));
    Doc::from_file(&path)
}

/// 保存内容到缓存目录
pub fn save_to_cache(name: &str, content: &str, source_url: &str) -> Result<(), String> {
    let path = home_dir().join("cache").join(format!("{}.md", name));
    let header = format!(
        "---\ntitle: {}\nsource: cache\nfetched: {}\n---\n\n",
        source_url,
        current_date()
    );
    let full = format!("{}{}", header, content);
    fs::write(&path, &full).map_err(|e| format!("写入缓存失败：{}", e))
}

/// 获取当前日期字符串 YYYY-MM-DD
pub(crate) fn current_date() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();

    // Unix time 转公历日期（忽略闰秒，误差可接受）
    let days = secs / 86400;
    let mut y = 1970i64;
    let mut remaining = days as i64;

    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let leap = is_leap(y);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30,
                      31, 31, 30, 31, 30, 31];
    let mut m = 0;
    for &md in &month_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        m += 1;
    }

    format!("{:04}-{:02}-{:02}", y, m + 1, remaining as u32 + 1)
}

/// 判断闰年
fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
