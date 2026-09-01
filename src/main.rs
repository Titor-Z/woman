// woman — Windows man：统一手册查看工具
// 主入口：解析命令行参数并分发到对应功能
// 核心：woman <name> 对齐 man —— docs 命中即展示，否则全自动取源 + AI 整理写 docs

use std::process;

mod ai;
mod catalog;
mod config;
mod display;
mod docs;
mod editor;
mod fetch;
mod init;
mod platform;
mod skill;
mod tui;

use ai::{ask_once, enhance, run_repl};
use config::Config;
use display::{render_error, render_hint};
use docs::{find_in_cache, find_in_docs, save_to_cache, Doc};
use fetch::{classify_command, fetch_source, CommandType};

// ============================================================
// 帮助和版本信息
// ============================================================

fn print_help() {
    println!("woman — Windows man：统一手册查看工具");
    println!("");
    println!("用法：");
    println!("  woman <name>            查看命令手册（对齐 man：全自动取源 + AI 整理）");
    println!("  woman -q \"<问题>\"        一次性问答（AI 结合本地手册回答）");
    println!("  woman ai                交互式 AI 自由对话（/model 切换提供者）");
    println!("  woman init              交互式初始化向导（选厂商/模型，配置 AI）");
    println!("  woman init --refresh    重新拉取 models.dev 目录更新缓存");
    println!("  woman init --reset      恢复默认 config + skill 模板");
    println!("  woman init --reset --wipe  恢复默认并清空 docs/ cache/");
    println!("  -?, --help              显示此帮助");
    println!("  -V, --version           显示版本号");
    println!("");
    println!("查找流程：");
    println!("  1. ~/.woman/docs/<name>.md（整理后手册，命中即展示）");
    println!("  2. 分类命令（coreutils / powershell / windows）→ 取源");
    println!("  3. 有 AI key → 自动整理写入 docs/；无 key → 直接展示原始内容");
}

fn print_version() {
    println!("woman v{}", env!("CARGO_PKG_VERSION"));
}

// ============================================================
// 判定 AI 是否可用（是否配置了有效 key）
// ============================================================

/// 返回是否配置了有效的默认 AI 提供者
fn ai_available() -> bool {
    let config = Config::load();
    match config.get_provider(None) {
        Some(p) => {
            let key = p.api_key.trim();
            !key.is_empty() && !key.contains("your-api-key")
        }
        None => false,
    }
}

// ============================================================
// 命令类型 → 展示用字符串
// ============================================================

fn type_label(t: CommandType) -> &'static str {
    match t {
        CommandType::Coreutils => "coreutils",
        CommandType::Powershell => "powershell",
        CommandType::Windows => "windows",
        CommandType::Unknown => "unknown",
    }
}

/// 从命中的多个类型中按优先级选取默认类型（coreutils > windows > powershell > unknown）
fn pick_default_type(types: &[CommandType]) -> CommandType {
    if types.contains(&CommandType::Coreutils) {
        CommandType::Coreutils
    } else if types.contains(&CommandType::Windows) {
        CommandType::Windows
    } else if types.contains(&CommandType::Powershell) {
        CommandType::Powershell
    } else {
        CommandType::Unknown
    }
}

/// 选择命令最终处理类型。多个具体类型命中时弹 mdr 卡片选择器；否则按优先级取默认。
fn choose_type(name: &str, types: &[CommandType]) -> CommandType {
    // 去重 + 过滤 Unknown，收集具体类型
    let mut concrete: Vec<CommandType> = Vec::new();
    for t in types {
        if *t != CommandType::Unknown && !concrete.contains(t) {
            concrete.push(*t);
        }
    }

    match concrete.len() {
        0 => CommandType::Unknown,
        1 => concrete[0],
        _ => {
            // 多种类型命中：让用户选择
            let entries: Vec<(String, String)> = concrete
                .iter()
                .map(|&t| {
                    let (label, desc) = match t {
                        CommandType::Coreutils => (
                            "coreutils".to_string(),
                            format!("{} 是 GNU coreutils 命令（本机 coreutils.exe）", name),
                        ),
                        CommandType::Powershell => (
                            "powershell".to_string(),
                            format!("{} 是 PowerShell cmdlet（Get-Help 文档）", name),
                        ),
                        CommandType::Windows => (
                            "windows".to_string(),
                            format!("{} 是 Windows 原生命令（MS Learn 文档）", name),
                        ),
                        CommandType::Unknown => ("unknown".to_string(), "未知类型".to_string()),
                    };
                    (label, desc)
                })
                .collect();

            let title = format!("{name} 匹配多个命令类型，请选择：");
            match tui::pick_entries(&entries, &title) {
                Some(idx) => concrete.get(idx).copied().unwrap_or(CommandType::Unknown),
                None => pick_default_type(&concrete),
            }
        }
    }
}

// ============================================================
// 全自动查找并显示（对齐 man）
// ============================================================

/// 完整查找流程：docs/ 命中 → 展示；否则分类取源 → cache → AI 整理写 docs → 展示
fn lookup_and_show(name: &str) {
    // 1. docs/ 目录下查找（命中即展示，不重复调 AI）
    if let Some(doc) = find_in_docs(name) {
        let badge = doc.source_badge();
        let _ = tui::show_document(&doc.body, &[badge.as_str()]);
        return;
    }

    // 2. 分类命令类型（多类型命中时弹选择器）
    let types = classify_command(name);
    let chosen = choose_type(name, &types);
    render_hint(&format!(
        "未找到 '{name}' 的 docs，按命令类型「{}」自动获取...",
        type_label(chosen)
    ));

    // 3. 取源（双资料：本地 --help 真值 + 在线全文详情）
    let fetched = match fetch_source(name, chosen) {
        Ok(f) => f,
        Err(e) => {
            // 源取不到：尝试用 cache 兜底（若是旧缓存里的内容）
            if let Some(doc) = find_in_cache(name) {
                render_error(&format!("{e}，使用本地缓存展示"));
                let badge = doc.source_badge();
                let hints = [badge.as_str()];
                let _ = tui::show_document(&doc.body, &hints);
                return;
            }
            render_error(&format!("无法获取 '{name}' 的文档：{e}"));
            render_hint("可运行 `woman ai` 让 AI 帮你查找，或检查网络连接。");
            return;
        }
    };

    // 4. 保存合并后的完整原始资料到 cache（作为 AI 输入 + 离线兜底）
    let _ = save_to_cache(name, &fetched.combined, &fetched.label, type_label(chosen));

    // 5. AI 整理（有 key 时）写入 docs；无 key 或失败则直接展示完整原始资料
    if ai_available() {
        render_hint("正在用 AI 整理成中文手册...");
        let skill_content = skill::load_manual_skill();
        match enhance(
            name,
            fetched.local_help.as_deref(),
            fetched.online.as_deref(),
            &skill_content,
            type_label(chosen),
        ) {
            Ok(md) => {
                let path = Config::docs_dir().join(format!("{}.md", name));
                if let Err(e) = std::fs::write(&path, &md) {
                    render_error(&format!("保存整理后的手册失败：{e}"));
                }
                if let Some(doc) = find_in_docs(name) {
                    let badge = doc.source_badge();
                    let _ = tui::show_document(&doc.body, &[badge.as_str()]);
                    return;
                }
            }
            Err(e) => {
                render_error(&format!("AI 整理失败：{e}，直接展示原始内容"));
            }
        }
    }

    // 6. 兜底：展示合并后的完整原始资料（cache 或源文本，含本地 + 在线全文）
    let doc = Doc::from_cache(&fetched.combined, crate::docs::DocMeta::with_source("cache"));
    let badge = doc.source_badge();
    let hints = [badge.as_str()];
    let _ = tui::show_document(&fetched.combined, &hints);
}

// ============================================================
// 一次性问答（woman -q）
// ============================================================

fn run_query(question: &str) {
    if !ai_available() {
        render_error("未配置有效的 AI 提供者，`woman -q` 需要 AI 能力。");
        render_hint("请编辑 ~/.woman/config.json 设置 api_key 后再试。");
        process::exit(1);
    }
    match ask_once(question) {
        Ok(_) => {}
        Err(e) => {
            render_error(&e);
            process::exit(1);
        }
    }
}

// ============================================================
// 主入口
// ============================================================

fn main() {
    // 确保目录结构存在
    Config::ensure_dirs();

    let raw: Vec<String> = std::env::args().collect();

    // 处理 --help / -? 单独使用
    if raw.len() == 2 && (raw[1] == "--help" || raw[1] == "-?") {
        print_help();
        return;
    }
    // 处理 --version / -V 单独使用
    if raw.len() == 2 && (raw[1] == "--version" || raw[1] == "-V") {
        print_version();
        return;
    }

    // 无参数
    if raw.len() < 2 {
        print_help();
        return;
    }

    let first = raw[1].as_str();

    // -q：一次性问答
    if first == "-q" || first == "--query" {
        let question = raw[2..].join(" ");
        if question.trim().is_empty() {
            render_error("用法：woman -q \"<问题>\"");
            process::exit(1);
        }
        run_query(&question);
        return;
    }

    // ai：自由对话
    if first == "ai" {
        let mut config = Config::load();
        match config.get_provider(None) {
            Some(p) => {
                if let Err(e) = run_repl(p.clone(), &mut config.ai) {
                    render_error(&e);
                    process::exit(1);
                }
            }
            None => {
                render_error("未配置 AI 提供者。请编辑 ~/.woman/config.json 添加 ai 配置。");
                process::exit(1);
            }
        }
        return;
    }

    // init：交互式初始化向导（--refresh / --reset / --wipe）
    if first == "init" {
        let mut opts = init::InitOptions::default();
        for a in &raw[2..] {
            match a.as_str() {
                "--refresh" => opts.refresh = true,
                "--reset" => opts.reset = true,
                "--wipe" => opts.wipe = true,
                _ => {
                    render_error(&format!("未知 init 参数：{}", a));
                    process::exit(1);
                }
            }
        }
        if let Err(e) = init::run_init(opts) {
            if !e.is_empty() {
                render_error(&e);
            }
            process::exit(1);
        }
        return;
    }

    // 处理未知选项（以 - 开头的第一个参数）
    if first.starts_with('-') {
        render_error(&format!("未知选项：{}", first));
        process::exit(1);
    }

    // 默认：woman <name> — 查找并显示
    lookup_and_show(first);
}
