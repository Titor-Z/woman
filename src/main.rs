// woman — Windows man：统一手册查看工具
// 主入口：解析命令行参数并分发到对应功能

use std::process;

mod ai;
mod config;
mod display;
mod docs;
mod fetch;
mod tui;

use ai::{run_repl, generate_docs};
use config::Config;
use display::{render_plain, render_hint, render_error};
use docs::{Doc, find_in_docs, find_in_cache, save_to_cache};
use fetch::{command_exists, is_windows_command, run_help, fetch_from_archlinux, fetch_from_mslearn};

// ============================================================
// 帮助和版本信息
// ============================================================

fn print_help() {
    println!("woman — Windows man：统一手册查看工具");
    println!("");
    println!("用法：");
    println!("  woman <name>            查看命令手册");
    println!("  woman search <name>     在线检索并缓存手册");
    println!("  woman ai                交互式 AI 对话（/model 切换提供者）");
    println!("  woman generate <name>   基于缓存生成最终版手册（AI 功能）");
    println!("  -?, --help              显示此帮助");
    println!("  -V, --version           显示版本号");
    println!("");
    println!("查找顺序：");
    println!("  1. ~/.woman/docs/<name>.md（最终版手册）");
    println!("  2. ~/.woman/cache/<name>.md（在线缓存）");
    println!("  3. <name> --help（原始输出）");
}

fn print_version() {
    println!("woman v{}", env!("CARGO_PKG_VERSION"));
}

// ============================================================
// 查找并显示文档
// ============================================================

/// 完整查找流程：docs/ → command exists → cache/ → --help → 未找到
fn lookup_and_show(name: &str) {
    // 1. docs/ 目录下查找
    if let Some(doc) = find_in_docs(name) {
        let _ = tui::show_document(&doc.body, &[]);
        return;
    }

    let exists = command_exists(name);

    // 2. cache/ 目录下查找
    if let Some(doc) = find_in_cache(name) {
        let badge = doc.source_badge();
        let gen_hint = format!("运行 `woman generate {name}` 生成最终版手册");
        let hints: Vec<&str> = if exists {
            vec![&badge, &gen_hint]
        } else {
            vec![&badge]
        };
        let _ = tui::show_document(&doc.body, &hints);
        return;
    }

    // 3. 运行 --help
    if exists {
        if let Some(help_text) = run_help(name) {
            let doc = Doc::from_help(name, &help_text);
            let badge = doc.source_badge();
            let hints = [
                badge.as_str(),
                &format!("运行 `woman search {name}` 在线检索手册"),
                &format!("运行 `woman generate {name}` 生成完整手册"),
            ];
            let _ = tui::show_document(&help_text, &hints);
            return;
        }
    }

    // 4. 全部未找到
    if exists {
        render_error(&format!("无法获取 '{name}' 的帮助信息（--help 无输出）"));
        render_hint(&format!("运行 `woman search {name}` 在线检索"));
    } else {
        render_error(&format!("程序 '{name}' 未安装或找不到文档"));
        render_hint(&format!("运行 `woman search {name}` 在线检索"));
    }
}

// ============================================================
// 在线检索并缓存
// ============================================================

/// 在线检索并缓存（Windows 原生命令优先 MS Learn，其他优先 archlinux）
fn search_and_cache(name: &str) {
    let win_cmd = is_windows_command(name);
    let (text, source) = if win_cmd {
        render_hint(&format!("正在从 Microsoft Learn 检索 '{}'...", name));
        match fetch_from_mslearn(name) {
            Ok(t) => (t, format!("learn.microsoft.com/{}", name)),
            Err(_) => {
                render_hint(&format!("正在从 man.archlinux.org 检索 '{}'...", name));
                match fetch_from_archlinux(name) {
                    Ok(t) => (t, format!("man.archlinux.org/{}", name)),
                    Err(e) => {
                        render_error(&e);
                        render_hint("也可通过 `woman ai` 或搜索引擎获取该命令信息");
                        return;
                    }
                }
            }
        }
    } else {
        render_hint(&format!("正在从 man.archlinux.org 检索 '{}'...", name));
        match fetch_from_archlinux(name) {
            Ok(t) => (t, format!("man.archlinux.org/{}", name)),
            Err(_) => {
                render_hint(&format!("正在从 Microsoft Learn 检索 '{}'...", name));
                match fetch_from_mslearn(name) {
                    Ok(t) => (t, format!("learn.microsoft.com/{}", name)),
                    Err(e) => {
                        render_error(&e);
                        render_hint("也可通过 `woman ai` 或搜索引擎获取该命令信息");
                        return;
                    }
                }
            }
        }
    };

    let _ = match save_to_cache(name, &text, &source) {
        Ok(_) => {
            render_hint("已保存到缓存，显示内容：");
            let meta = docs::DocMeta::with_source("cache");
            let doc = Doc::from_cache(&text, meta);
            let badge = doc.source_badge();
            let hints = [
                badge.as_str(),
                &format!("运行 `woman generate {name}` 生成完整手册"),
            ];
            tui::show_document(&text, &hints)
        }
        Err(e) => {
            render_error(&format!("保存缓存失败：{e}"));
            println!("");
            render_plain(&text);
            Ok(())
        }
    };
}

// ============================================================
// 主入口
// ============================================================

fn main() {
    // 确保目录结构存在
    Config::ensure_dirs();

    let raw: Vec<String> = std::env::args().collect();

    // 处理 --help / -? / --version / -V 作为唯一参数
    if raw.len() == 2 {
        match raw[1].as_str() {
            "--help" | "-?" => {
                print_help();
                return;
            }
            "--version" | "-V" => {
                print_version();
                return;
            }
            _ => {}
        }
    }

    // 无参数
    if raw.len() < 2 {
        print_help();
        return;
    }

    let subcmd = raw[1].as_str();

    // 处理未知选项（以 - 开头的第一个参数）
    if subcmd.starts_with('-') {
        render_error(&format!("未知选项：{}", subcmd));
        process::exit(1);
    }

    // 分发子命令
    match subcmd {
        "search" => {
            let name = raw.get(2).map(|s| s.as_str());
            match name {
                Some(n) => {
                    if n.starts_with('-') {
                        render_error(&format!("未知选项：{}", n));
                        process::exit(1);
                    }
                    search_and_cache(n);
                }
                None => {
                    render_error("用法：woman search <name>");
                    process::exit(1);
                }
            }
        }
        "ai" => {
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
        }
        "generate" => {
            let name = raw.get(2).map(|s| s.as_str());
            match name {
                Some(n) => {
                    if n.starts_with('-') {
                        render_error(&format!("未知选项：{n}"));
                        process::exit(1);
                    }
                    if let Err(e) = generate_docs(n) {
                        render_error(&e);
                        process::exit(1);
                    }
                }
                None => {
                    render_error("用法：woman generate <name>");
                    process::exit(1);
                }
            }
        }
        _ => {
            // 默认：woman <name> — 查找并显示
            lookup_and_show(subcmd);
        }
    }
}
