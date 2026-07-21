// ai.rs — AI 对话客户端 + 工具调用 + REPL 循环
// 通过 curl.exe POST 调用 OpenAI 兼容 API
// AI 只有一个 bash 工具，所有操作通过 PowerShell 命令完成

use crate::config::{AiProvider, Config};
use crate::docs::{current_date, find_in_cache, find_in_docs};
use crate::fetch::{command_exists, run_help};

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use crossterm::cursor::{Hide, MoveUp, Show};
use crossterm::event::{read, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::execute;

// ============================================================
// 系统提示词
// ============================================================

const SYSTEM_PROMPT: &str = "\
你是一个 Windows 命令行助手 woman AI，默认运行在 PowerShell 环境中。
你只有一个工具 **bash**，所有操作都通过它完成。

## 环境说明
- 底层 shell：PowerShell（`pwsh -NoProfile -Command`）
- 本机已安装 GNU coreutils（`C:\\Program Files\\coreutils\\bin\\`），以下命令**必须加 `.exe` 后缀**（PowerShell alias 会拦截 `ls` `cat` 等）：
  `ls.exe` `cat.exe` `cp.exe` `mv.exe` `rm.exe` `mkdir.exe` `echo.exe`
- 运行 `coreutils.exe --list-raw` 查看所有支持的命令列表
- 其他 coreutils 命令可直接使用（如 `grep` `sed` `find` `head` `tail` `wc` 等）
- 读文件用 `cat.exe`
- HTTP 请求用 `curl.exe`
- 查找命令路径用 `where.exe` <name>

## 优先级（按回答偏好从高到低）
1. **GNU coreutils** — 本机已安装。回答时优先介绍 coreutils 版本。
2. **自定义命令** — `was`、`unwas`（PowerShell $PROFILE 别名管理）、`woman`（本工具）等，这些是本机特有命令。
3. **标准 Windows 命令** — `dir` `find` `icacls` 等 cmd.exe 原生命令。
4. **PowerShell cmdlet** — `Get-ChildItem` `Select-String` 等，优先级最低，仅在用户明确询问或前两者无法覆盖时才回答。

## 规则
1. 始终用中文回答
2. 当用户询问某个命令时，先通过 `bash` 获取原始信息（如 `command --help`、`curl.exe` 抓取在线手册），再给出结构化的中文解释
3. 解释应包含：用途、基本语法、常用选项、典型示例
4. 如果用户要求生成或保存手册，使用 bash 的 echo/重定向写入文件到 `$env:USERPROFILE\\.woman\\docs\\` 目录，内容须包含 YAML frontmatter：
   ---
   title: <命令名>
   source: ai-generated
   generated: YYYY-MM-DD
   ---
5. **终端友好排版**：由于输出在终端渲染，请**避免使用表格和 Markdown 代码块（```）**。推荐用**列表（- 或 1.）、缩进、加粗**来组织内容";

// ============================================================
// 工具定义（OpenAI tools 格式）
// ============================================================

const TOOLS_JSON: &str = r#"[
  {
    "type": "function",
    "function": {
      "name": "bash",
      "description": "Run a shell command on Windows (PowerShell). Execute any command, script, or program. Returns stdout + stderr.",
      "parameters": {
        "type": "object",
        "properties": {
          "command": {
            "type": "string",
            "description": "The PowerShell command to execute"
          }
        },
        "required": ["command"]
      }
    }
  }
]"#;

// ============================================================
// API 消息类型
// ============================================================

/// 发送给 API 的消息（兼容新旧两种 role 格式）
#[derive(Debug, Serialize)]
struct RequestMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<serde_json::Value>>,
}

// ============================================================
// SSE 流式响应类型
// ============================================================

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<StreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct StreamToolCall {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    index: Option<u32>,
    #[serde(default)]
    function: Option<StreamFunction>,
}

#[derive(Debug, Deserialize)]
struct StreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// 流式调用结果
enum StreamOutcome {
    Complete(String),
    ToolCall { fc: FunctionCall, tool_call_id: Option<String> },
}

/// 函数调用结构（兼容两种 arguments 格式）
#[derive(Debug, Clone, Deserialize)]
struct FunctionCall {
    name: String,
    #[serde(deserialize_with = "de_arguments")]
    arguments: String,
}

/// arguments 可能是 JSON 字符串或 JSON 对象，统一转为字符串
fn de_arguments<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::String(s) => Ok(s),
        other => Ok(other.to_string()),
    }
}

/// 打字机效果逐字输出（ANSI 转义序列整体打出）
fn typewrite(text: &str, delay: Duration) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            let start = i;
            while i < bytes.len() && bytes[i] != b'm' { i += 1; }
            if i < bytes.len() { i += 1; }
            print!("{}", &text[start..i]);
        } else {
            let c = text[i..].chars().next().unwrap();
            print!("{c}");
            i += c.len_utf8();
            io::stdout().flush().ok();
            thread::sleep(delay);
        }
    }
}

// ============================================================
// bash 工具执行
// ============================================================

const DANGEROUS_CMDS: &[&str] = &[
    "rm -rf /", "rd /s /q", "format ", "shutdown", "reboot",
    "> NUL", "> \\\\.\\", "del /f /s", "erase /f", "remove-item -recurse",
];

/// 安全的执行 shell 命令（PowerShell），含危险命令过滤和输出截断
fn run_bash(command: &str) -> String {
    let lower = command.to_lowercase();
    if DANGEROUS_CMDS.iter().any(|d| lower.contains(d)) {
        return "错误：该命令已被安全策略拦截".to_string();
    }

    match Command::new("pwsh")
        .args(["-NoProfile", "-Command", command])
        .output()
    {
        Ok(output) => {
            let mut result = String::new();
            if !output.stdout.is_empty() {
                result.push_str(&String::from_utf8_lossy(&output.stdout));
            }
            if !output.stderr.is_empty() {
                if !result.is_empty() { result.push('\n'); }
                result.push_str(&String::from_utf8_lossy(&output.stderr));
            }
            let trimmed = result.trim().to_string();
            if trimmed.is_empty() {
                return "(无输出)".to_string();
            }
            // 截断过大的输出
            const MAX_OUTPUT: usize = 50000;
            if trimmed.len() > MAX_OUTPUT {
                let mut truncated = trimmed[..MAX_OUTPUT].to_string();
                truncated.push_str(&format!("\n...（输出被截断，共 {} 字符）", trimmed.len()));
                return truncated;
            }
            trimmed
        }
        Err(e) => format!("执行失败：{e}"),
    }
}

// ============================================================
// 流式 API 调用（SSE via curl -N）
// ============================================================

fn chat_completion_stream(provider: &AiProvider, messages: &[RequestMessage]) -> Result<StreamOutcome, String> {
    let url = provider.api_base.trim_end_matches('/').to_string();

    let body = serde_json::json!({
        "model": provider.model,
        "messages": messages,
        "tools": serde_json::from_str::<serde_json::Value>(TOOLS_JSON).unwrap(),
        "tool_choice": "auto",
        "stream": true,
    });

    let body_str = body.to_string();
    let mut child = Command::new("curl.exe")
        .args([
            "-sS", "-N",
            "-X", "POST",
            &url,
            "-m", "120",
            "-H", "Content-Type: application/json",
            "-H", &format!("Authorization: Bearer {}", provider.api_key),
            "-d", &body_str,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("无法启动 curl.exe：{e}"))?;

    let stdout = child.stdout.take().unwrap();
    let reader = io::BufReader::new(stdout);

    let mut full_content = String::new();
    let mut line_buf = String::new();
    let mut tool_call_id: Option<String> = None;
    let mut tool_name: Option<String> = None;
    let mut tool_args = String::new();
    let mut finish_reason: Option<String> = None;

    for line_result in reader.lines() {
        let line = line_result.map_err(|e| format!("读取流响应失败：{e}"))?;

        if !line.starts_with("data: ") {
            continue;
        }
        let data = &line[6..];
        if data == "[DONE]" {
            break;
        }

        let chunk: StreamChunk = match serde_json::from_str(data) {
            Ok(c) => c,
            Err(e) => {
                if let Ok(err) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(msg) = err["error"]["message"].as_str() {
                        return Err(format!("API 错误：{msg}"));
                    }
                }
                return Err(format!("解析流事件失败：{e}\n原始数据：{}", &data[..data.len().min(200)]));
            }
        };

        let choice = match chunk.choices.first() {
            Some(c) => c,
            None => continue,
        };

        finish_reason = choice.finish_reason.clone();

        if let Some(tcs) = &choice.delta.tool_calls {
            for tc in tcs {
                if let Some(id) = &tc.id {
                    tool_call_id = Some(id.clone());
                }
                if let Some(func) = &tc.function {
                    if let Some(name) = &func.name {
                        tool_name = Some(name.clone());
                    }
                    if let Some(args) = &func.arguments {
                        tool_args.push_str(args);
                    }
                }
            }
        }

        if let Some(delta) = &choice.delta.content {
            full_content.push_str(delta);
            line_buf.push_str(delta);
            while let Some(pos) = line_buf.find('\n') {
                let complete = line_buf[..=pos].to_string();
                line_buf = line_buf[pos + 1..].to_string();
                typewrite(&crate::display::ansi_format(&complete), Duration::from_millis(6));
            }
        }
    }

    let _ = child.wait();

    if finish_reason.as_deref() == Some("tool_calls") {
        if let Some(name) = tool_name {
            return Ok(StreamOutcome::ToolCall {
                fc: FunctionCall { name, arguments: tool_args },
                tool_call_id,
            });
        }
    }

    if !line_buf.is_empty() {
        let formatted = crate::display::ansi_format(&line_buf);
        if !formatted.trim().is_empty() {
            typewrite(&formatted, Duration::from_millis(6));
            println!();
        }
    }

    Ok(StreamOutcome::Complete(full_content))
}

// ============================================================
// 非流式 API 调用（用于文档生成）
// ============================================================

/// 非流式 chat completion 调用
fn chat_completion(provider: &AiProvider, messages: &[RequestMessage]) -> Result<String, String> {
    let url = provider.api_base.trim_end_matches('/').to_string();

    let body = serde_json::json!({
        "model": provider.model,
        "messages": messages,
    });

    let body_str = body.to_string();
    let output = Command::new("curl.exe")
        .args([
            "-sS",
            "-X", "POST",
            &url,
            "-m", "120",
            "-H", "Content-Type: application/json",
            "-H", &format!("Authorization: Bearer {}", provider.api_key),
            "-d", &body_str,
        ])
        .output()
        .map_err(|e| format!("无法启动 curl.exe：{e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("API 请求失败：{}", err.trim()));
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let resp: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("解析 API 响应失败：{e}"))?;

    if let Some(msg) = resp["error"]["message"].as_str() {
        return Err(format!("API 错误：{msg}"));
    }

    resp["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "API 响应缺少 content".to_string())
}

// ============================================================
// 文档生成（woman generate）
// ============================================================

/// 为指定命令生成 AI 手册并保存到 docs/
pub fn generate_docs(name: &str) -> Result<(), String> {
    let config = Config::load();
    let provider = config.get_provider(None)
        .ok_or_else(|| "未配置 AI 提供者。请编辑 ~/.woman/config.json 添加 ai 配置。".to_string())?;

    let key = provider.api_key.trim();
    if key.is_empty() || key.contains("your-api-key") {
        return Err("API 密钥未配置或为占位符".to_string());
    }

    // 获取原始资料
    let (source_content, source_type) = if let Some(doc) = find_in_cache(name) {
        (doc.body, "缓存手册")
    } else if command_exists(name) {
        if let Some(help_text) = run_help(name) {
            (help_text, "--help 输出")
        } else {
            return Err(format!("'{name}' --help 无输出"));
        }
    } else {
        return Err(format!("找不到 '{name}' 的缓存或命令。请先运行 `woman search {name}`"));
    };

    let today = current_date();
    let sys_prompt = format!(
        "你是一个文档生成助手。请根据提供的原始资料，生成一份结构化的中文手册。

要求：
- 输出必须包含 YAML frontmatter，格式如下：
---
title: {name}
source: ai-generated
generated: {today}
---

- 内容应包含：用途说明、基本语法、常用选项、典型示例
- 终端友好排版：不要使用表格和 Markdown 代码块（```）
- 使用列表（- 或 1.）、加粗、缩进来组织内容
- 语言：中文",
        name = name,
        today = today,
    );

    let messages = vec![
        RequestMessage {
            role: "system".into(),
            content: Some(sys_prompt),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        },
        RequestMessage {
            role: "user".into(),
            content: Some(format!("以下是为 {name} 生成的原始资料（{source_type}）：\n\n{source_content}")),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        },
    ];

    println!("\n🤖 WOMAN AI · {} 正在生成 {} 文档...\n", provider.name, name);
    let content = chat_completion(&provider, &messages)?;

    // 保存到 docs/
    let path = Config::docs_dir().join(format!("{}.md", name));
    std::fs::write(&path, &content)
        .map_err(|e| format!("保存文档失败：{e}"))?;

    println!("文档已保存 {} 完毕。", path.display());

    // 显示生成的文档
    if let Some(doc) = find_in_docs(name) {
        let badge = doc.source_badge();
        let _ = crate::tui::show_document(&doc.body, &[badge.as_str()]);
    }

    Ok(())
}

// ============================================================
// 工具结果排版优化
// ============================================================

/// 截断工具结果，仅显示前几行预览
fn truncate_output(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() > 2 {
        let preview: String = lines.iter().take(2).cloned().collect::<Vec<_>>().join("\n");
        format!("{}\x1b[2m\n...（共 {} 行）\x1b[0m", preview, lines.len())
    } else {
        text.to_string()
    }
}

// ============================================================
// REPL 交互循环
// ============================================================

fn print_repl_help() {
    println!("╌╌╌ WOMAN AI 命令 ╌╌╌");
    println!("  [\x1b[34m/exit\x1b[0m] / [\x1b[34m/quit\x1b[0m]    退出对话");
    println!("  [\x1b[34m/help\x1b[0m]           显示此帮助");
    println!("  [\x1b[34m/clear\x1b[0m]          清屏");
    println!("  [\x1b[34m/truncate\x1b[0m]       清除历史，开始新话题");
    println!("  [\x1b[34m/model\x1b[0m]           列出可用模型");
    println!("  [\x1b[34m/model\x1b[0m] <name>    切换到指定模型");
    println!("╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌");
    println!("其余文字作为消息发送给 AI。");
    println!("AI 只有一个 bash 工具，可执行任何 PowerShell 命令。");
    println!("例如：`ls.exe` `cat.exe` `curl.exe` `where.exe` 等。");
}

fn clear_screen() {
    let _ = Command::new("cmd").args(["/c", "cls"]).status();
}

/// 交互式选择器：↑↓/jk 切换，Enter 确认，Esc 取消
fn select_provider(all: &[AiProvider], current: &str) -> Option<usize> {
    if all.len() <= 1 { return None; }
    let mut sel = all.iter().position(|p| p.name == current).unwrap_or(0);
    execute!(io::stdout(), Hide).ok()?;
    enable_raw_mode().ok()?;

    for (i, p) in all.iter().enumerate() {
        if i == sel {
            println!("\x1b[48;5;208m\x1b[30m {} · {} \x1b[0m", p.name, p.model);
        } else {
            println!("  {} · {}", p.name, p.model);
        }
    }

    let result = loop {
        match read() {
            Ok(Event::Key(ke)) if ke.kind == KeyEventKind::Press => match ke.code {
                KeyCode::Up | KeyCode::Char('k') if sel > 0 => sel -= 1,
                KeyCode::Down | KeyCode::Char('j') if sel + 1 < all.len() => sel += 1,
                KeyCode::Enter => break Some(sel),
                KeyCode::Esc => break None,
                _ => {}
            },
            _ => {}
        }
        for _ in 0..all.len() {
            execute!(io::stdout(), MoveUp(1)).ok();
        }
        for (i, p) in all.iter().enumerate() {
            if i == sel {
                println!("\x1b[48;5;208m\x1b[30m {} · {} \x1b[0m", p.name, p.model);
            } else {
                println!("  {} · {}", p.name, p.model);
            }
        }
    };

    disable_raw_mode().ok()?;
    execute!(io::stdout(), Show).ok();
    result
}

/// 启动 AI 交互式 REPL
pub fn run_repl(initial: AiProvider, all_providers: &mut Vec<AiProvider>) -> Result<(), String> {
    let mut current = initial;
    let key = current.api_key.trim();
    if key.is_empty() || key.contains("your-api-key") {
        eprintln!("⚠ API 密钥未配置或为占位符");
        eprintln!("  请编辑 ~/.woman/config.json 设置正确的 api_key");
        return Ok(());
    }

    let mut messages: Vec<RequestMessage> = Vec::new();
    messages.push(RequestMessage {
        role: "system".into(),
        content: Some(SYSTEM_PROMPT.into()),
        tool_call_id: None,
        name: None,
        tool_calls: None,
    });

    println!("\n🤖 WOMAN AI · \x1b[38;5;208m{}\x1b[0m", current.name);
    println!("💡 输入 [\x1b[34m/exit\x1b[0m] 退出 · [\x1b[34m/help\x1b[0m] 查看帮助\n");

    let mut input = String::new();
    loop {
        print!("\x1b[34m> \x1b[0m");
        io::stdout().flush().map_err(|e| format!("输出刷新失败：{e}"))?;

        input.clear();
        if io::stdin().read_line(&mut input).is_err() {
            println!("\n再见！");
            break;
        }
        let line = input.trim();
        if line.is_empty() {
            continue;
        }

        // ---- REPL 命令 ----
        if line.starts_with('/') {
            let parts: Vec<&str> = line.split_whitespace().collect();
            match parts[0] {
                "/exit" | "/quit" => {
                    println!("再见！");
                    break;
                }
                "/help" => print_repl_help(),
                "/clear" => clear_screen(),
                "/truncate" => {
                    messages.truncate(1);
                    println!("已清除历史，开始新话题。");
                }
                "/model" => {
                    let new_name = if parts.len() >= 2 {
                        Some(parts[1].to_string())
                    } else {
                        select_provider(all_providers, &current.name)
                            .map(|i| all_providers[i].name.clone())
                    };
                    if let Some(ref name) = new_name {
                        if *name != current.name
                            && all_providers.iter().any(|p| p.name == *name)
                        {
                            for ap in all_providers.iter_mut() { ap.default = false; }
                            if let Some(ap) = all_providers.iter_mut().find(|ap| ap.name == *name) {
                                ap.default = true;
                            }
                            crate::config::Config::load().set_default(name);
                            current = all_providers.iter().find(|ap| ap.name == *name).unwrap().clone();
                            messages.truncate(1);
                            println!("\x1b[2m已切换到 \x1b[0m\x1b[38;5;208m{}\x1b[0m \x1b[2m({})\x1b[0m", current.name, current.model);
                        }
                    }
                }
                _ => println!("未知命令：{line}。输入 /help 查看可用命令。"),
            }
            continue;
        }

        // ---- 发送给 AI ----
        messages.push(RequestMessage {
            role: "user".into(),
            content: Some(line.to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        });

        // 工具调用循环（流式 SSE）
        loop {
            match chat_completion_stream(&current, &messages) {
                Ok(StreamOutcome::ToolCall { fc, tool_call_id }) => {
                    // 从 arguments 中提取 command 参数
                    let cmd = serde_json::from_str::<serde_json::Value>(&fc.arguments)
                        .ok()
                        .and_then(|v| v["command"].as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| fc.arguments.trim_matches('"').to_string());

                    println!("\n\x1b[2m\x1b[38;5;244m$ {}\x1b[0m", cmd);
                    let result = run_bash(&cmd);
                    if !result.is_empty() {
                        println!("\x1b[2m\x1b[38;5;244m{}\x1b[0m", truncate_output(&result));
                    }

                    let tcid = tool_call_id.unwrap_or_else(|| "call_0".to_string());
                    messages.push(RequestMessage {
                        role: "assistant".into(),
                        content: None,
                        tool_call_id: None,
                        name: None,
                        tool_calls: Some(vec![serde_json::json!({
                            "id": tcid,
                            "type": "function",
                            "function": {
                                "name": fc.name,
                                "arguments": fc.arguments,
                            }
                        })]),
                    });
                    messages.push(RequestMessage {
                        role: "tool".into(),
                        content: Some(result),
                        tool_call_id: Some(tcid),
                        name: None,
                        tool_calls: None,
                    });

                    println!();
                }
                Ok(StreamOutcome::Complete(content)) => {
                    let clean = content
                        .replace("<|FunctionCallBegin|>", "")
                        .replace("<|FunctionCallEnd|>", "")
                        .trim()
                        .to_string();
                    messages.push(RequestMessage {
                        role: "assistant".into(),
                        content: Some(clean),
                        tool_call_id: None,
                        name: None,
                        tool_calls: None,
                    });
                    break;
                }
                Err(e) => {
                    eprintln!("\x1b[2m⚠ API 错误：{e}\x1b[0m");
                    messages.pop();
                    break;
                }
            }
        }
    }

    Ok(())
}
