// init.rs — `woman init` 交互式初始化向导
// 从 models.dev 目录选择 OpenAI 兼容厂商与模型，输入 API Key，写入 config.json。
// 支持 --refresh（重拉目录）、--reset（恢复默认）、--wipe（另清 docs/cache）。

use std::io::{self, IsTerminal, Write};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::catalog::{self, Catalog, Model, Provider, is_openai_compatible};
use crate::config::{AiProvider, Config};
use crate::tui;

/// init 的选项
#[derive(Debug, Clone, Copy, Default)]
pub struct InitOptions {
    /// 强制重新拉取 models.dev 目录（--refresh）
    pub refresh: bool,
    /// 恢复默认 config + skill（--reset）
    pub reset: bool,
    /// 在 reset 基础上另清 docs/ cache/（--reset --wipe）
    pub wipe: bool,
}

// ============================================================
// 交互式输入
// ============================================================

/// 普通一行输入（回显）。`default` 非空时直接回车返回默认值。
fn prompt_line(prompt: &str, default: Option<&str>) -> String {
    match default {
        Some(d) => print!("{prompt} [回车用默认: {d}] "),
        None => print!("{prompt} "),
    }
    io::stdout().flush().ok();
    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return String::new();
    }
    let line = line.trim().to_string();
    if line.is_empty() {
        default.map(|d| d.to_string()).unwrap_or_default()
    } else {
        line
    }
}

/// 掩码输入（不回显明文，逐个显示 `*`）。Esc 取消返回 Err。
fn prompt_masked(prompt: &str) -> Result<String, String> {
    print!("{prompt}");
    io::stdout().flush().ok();
    enable_raw_mode().map_err(|e| format!("无法进入 raw 模式：{e}"))?;

    let mut buf = String::new();
    let result = loop {
        match event::read() {
            Ok(Event::Key(ke)) if ke.kind == KeyEventKind::Press => match ke.code {
                KeyCode::Char(c) if !c.is_control() => {
                    buf.push(c);
                    print!("*");
                    io::stdout().flush().ok();
                }
                KeyCode::Backspace => {
                    buf.pop();
                    print!("\x08 \x08");
                    io::stdout().flush().ok();
                }
                KeyCode::Enter => break Ok(buf),
                KeyCode::Esc => break Err("已取消".to_string()),
                _ => {}
            },
            _ => {}
        }
    };

    disable_raw_mode().ok();
    println!();
    result
}

// ============================================================
// 返回值的提示函数
// ============================================================

/// 渲染「提示」行（绿色）
fn ok(msg: &str) {
    println!("\x1b[32m✓\x1b[0m {msg}");
}

/// 渲染「警告」行（黄色）
fn warn(msg: &str) {
    println!("\x1b[33m⚡\x1b[0m {msg}");
}

// ============================================================
// 厂商/模型选择
// ============================================================

/// 用 mdr 卡片选择器让用户从列表里选一个，返回选中下标；取消返回 None。
fn pick(entries: &[(String, String)], title: &str) -> Option<usize> {
    // 非终端环境直接选第 0 个
    if !io::stdout().is_terminal() {
        return Some(0);
    }
    tui::pick_entries(entries, title)
}

/// 排序后的 OpenAI 兼容厂商列表
fn compatible_providers(cat: &Catalog) -> Vec<&Provider> {
    let mut list: Vec<&Provider> = cat.values().filter(|p| is_openai_compatible(p)).collect();
    // 有可用模型的最优先，再按名称排序
    list.sort_by(|a, b| {
        let ab = !a.models.is_empty();
        let bb = !b.models.is_empty();
        bb.cmp(&ab).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    list
}

/// 展示一个模型的摘要行
fn model_desc(m: &Model) -> String {
    let mut tag = String::from(m.name.clone());
    let mut flags = Vec::new();
    if let Some(ctx) = m.limit.as_ref().and_then(|l| l.context) {
        flags.push(format!("ctx {ctx}"));
    }
    if let Some(out) = m.limit.as_ref().and_then(|l| l.output) {
        flags.push(format!("out {out}"));
    }
    if m.reasoning {
        flags.push("推理".to_string());
    }
    if m.tool_call {
        flags.push("工具调用".to_string());
    }
    if let Some(c) = m.cost.as_ref() {
        match (c.input, c.output) {
            (Some(i), Some(o)) => flags.push(format!("${i}/M in / ${o}/M out")),
            (Some(i), None) => flags.push(format!("${i}/M in")),
            (None, Some(o)) => flags.push(format!("${o}/M out")),
            (None, None) => {}
        }
    }
    if !flags.is_empty() {
        tag += &format!(" · {}", flags.join(" · "));
    }
    tag
}

// ============================================================
// 重置
// ============================================================

/// 恢复默认：config.json 置空默认 + skills/manual.md 重写；wipe 时另清 docs/ cache/
fn do_reset(wipe: bool) -> Result<(), String> {
    // config.json：清空 ai 列表（默认配置）
    let cfg = Config { ai: Vec::new() };
    cfg.save();
    ok("config.json 已恢复为默认（空提供者列表）");

    // skills/manual.md：重写为默认模板
    // 直接调用 skill 的默认模板写入
    if let Ok(skill_default) = crate::skill::default_skill() {
        let path = Config::skills_dir().join("manual.md");
        if let Err(e) = std::fs::write(&path, skill_default) {
            warn(&format!("重写 skills/manual.md 失败：{e}"));
        } else {
            ok("skills/manual.md 已恢复为默认模板");
        }
    }

    if wipe {
        // 清空 docs/ 与 cache/
        for d in [Config::docs_dir(), Config::cache_dir()] {
            if let Ok(entries) = std::fs::read_dir(&d) {
                for e in entries.flatten() {
                    let _ = std::fs::remove_file(e.path());
                    if e.path().is_dir() {
                        let _ = std::fs::remove_dir_all(e.path());
                    }
                }
            }
        }
        ok("docs/ 与 cache/ 已清空");
    }
    Ok(())
}

// ============================================================
// 向导主体
// ============================================================

/// 执行 `woman init`
///
/// - reset → 恢复默认（+可选 wipe），不进入向导。
/// - 否则：拉取/加载目录 → 选厂商 → 选模型 → 输 Key → 确认 base → 写入 config。
pub fn run_init(opts: InitOptions) -> Result<(), String> {
    if opts.reset {
        return do_reset(opts.wipe);
    }

    println!("woman 初始化向导 — 选择 LLM 厂商与模型");
    println!("目录来源：models.dev（MIT 开源，opencode/pi/deepseek-harness 同源）\n");

    // 1. 加载目录（缓存优先，--refresh 强制重拉）
    let catalog = match catalog::ensure_catalog(opts.refresh) {
        Ok(c) => c,
        Err(e) => {
            warn(&format!("{e}"));
            warn("请先联网运行一次 `woman init`，或手动编辑 ~/.woman/config.json 配置 AI 提供者。");
            return Err(String::new());
        }
    };
    ok(&format!(
        "目录已就绪：共 {} 家厂商，其中 {} 家可直接用（OpenAI 兼容）",
        catalog.len(),
        compatible_providers(&catalog).len()
    ));

    // 2. 选择厂商
    let providers = compatible_providers(&catalog);
    if providers.is_empty() {
        return Err("目录中没有可用的 OpenAI 兼容厂商".to_string());
    }
    let entries: Vec<(String, String)> = providers
        .iter()
        .map(|p| {
            let desc = format!("{} 个模型 · {}", p.models.len(), p.doc);
            (p.name.clone(), desc)
        })
        .collect();
    let sel = match pick(&entries, "选择厂商（↑↓ 选择，Enter 确认，q 取消）：") {
        Some(i) => i,
        None => return Err(String::new()),
    };
    let provider = providers[sel];
    ok(&format!("已选择厂商：{}", provider.name));

    // 3. 选择模型（过滤 deprecated）
    let mut models: Vec<&Model> = provider
        .models
        .values()
        .filter(|m| m.status.as_deref() != Some("deprecated"))
        .collect();
    models.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    if models.is_empty() {
        return Err(format!("厂商 '{}' 没有可用模型", provider.name));
    }
    let m_entries: Vec<(String, String)> =
        models.iter().map(|m| (model_desc(m), String::new())).collect();
    let m_sel = match pick(&m_entries, "选择模型（↑↓ 选择，Enter 确认，q 取消）：") {
        Some(i) => i,
        None => return Err(String::new()),
    };
    let model = models[m_sel];
    ok(&format!("已选择模型：{}", model.name));

    // 4. API Key（掩码输入）
    let key = prompt_masked("请输入 API Key（输入不回显，Enter 确认，Esc 取消）：")?;
    if key.trim().is_empty() {
        return Err("API Key 不能为空".to_string());
    }

    // 5. API base（默认用目录里的 api 字段，可覆盖）
    let default_base = provider.api.clone().unwrap_or_default();
    let base = prompt_line("API Base URL", Some(&default_base));
    if base.is_empty() {
        return Err("API Base URL 不能为空".to_string());
    }

    // 6. 写入 config（追加/替换同名，设置为默认）
    let mut config = Config::load();
    let added = config.add_provider(AiProvider {
        name: provider.id.clone(),
        api_base: base,
        api_key: key,
        model: model.id.clone(),
        default: true,
    });
    if added {
        ok(&format!(
            "已添加提供者 '{}'（模型 {}）并设为默认",
            provider.id, model.id
        ));
    } else {
        ok(&format!(
            "已更新提供者 '{}'（模型 {}）并设为默认",
            provider.id, model.id
        ));
    }
    println!("\n配置完成！运行 `woman <name>` 即可查看命令手册，或 `woman ai` 开始对话。");
    Ok(())
}
