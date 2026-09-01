// ai.rs — AI 对话客户端 + 工具调用 + REPL 循环
// 通过 curl.exe POST 调用 OpenAI 兼容 API
// AI 只有一个 bash 工具，所有操作通过 PowerShell 命令完成

use crate::config::{AiProvider, Config};
use crate::docs::current_date;
use crate::platform;

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{read, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

// ============================================================
// 系统提示词
// ============================================================

/// 构建按平台定制的系统提示词：Windows 讲 PowerShell + coreutils `.exe`；
/// Unix 讲 `sh` + 系统命令 + man。两者环境说明与本地手册路径不同。
fn system_prompt() -> String {
    #[cfg(target_os = "windows")]
    {
        r#"你是一个 Windows 命令行助手 woman AI，默认运行在 PowerShell 环境中。
你只有一个工具 **bash**，所有操作都通过它完成。

## 环境说明
- 底层 shell：PowerShell（`pwsh -NoProfile -Command`）
- 本机已安装 GNU coreutils（`C:\Program Files\coreutils\bin\`），以下命令**必须加 `.exe` 后缀**（PowerShell alias 会拦截 `ls` `cat` 等）：
  `ls.exe` `cat.exe` `cp.exe` `mv.exe` `rm.exe` `mkdir.exe` `echo.exe`
- 运行 `coreutils.exe --list-raw` 查看所有支持的命令列表
- 其他 coreutils 命令可直接使用（如 `grep` `sed` `find` `head` `tail` `wc` 等）
- 读文件用 `cat.exe`
- HTTP 请求用 `curl.exe`
- 查找命令路径用 `where.exe` <name>

## 本地手册资料
本机已有 woman 手册体系，回答命令类问题时**优先查阅本地资料**，再决定是否联网：
- 正式手册：`cat.exe $env:USERPROFILE\.woman\docs\<name>.md`（AI 整理后的中文手册）
- 原始缓存：`cat.exe $env:USERPROFILE\.woman\cache\<name>.txt`
- PowerShell 指令：`Get-Help <name> -Full`
- 列出已有手册：`ls.exe $env:USERPROFILE\.woman\docs\`
- 整理新手册时，遵守 `$env:USERPROFILE\.woman\skills\manual.md` 的章节结构（先 `cat.exe` 读它，再按其模板整理并写入 `docs\` 目录，frontmatter 用 `source: ai-enhanced`）

## 优先级（按回答偏好从高到低）
1. **GNU coreutils** — 本机已安装。回答时优先介绍 coreutils 版本。
2. **自定义命令** — `was`、`unwas`、`woman`（本工具）等，这些是本机特有命令。
3. **标准 Windows 命令** — `dir` `find` `icacls` 等 cmd.exe 原生命令。
4. **PowerShell cmdlet** — `Get-ChildItem` `Select-String` 等，优先级最低，仅在用户明确询问或前两者无法覆盖时才回答。

## 规则
1. 始终用中文回答
2. 当用户询问某个命令时，先通过 `bash` 获取原始信息（如 `command --help`、`Get-Help`、`cat.exe` 读本地手册、`curl.exe` 抓取在线手册），再给出结构化的中文解释
3. 解释应包含：用途、基本语法、常用选项、典型示例
4. 如果用户要求生成或保存手册，用 bash 的 echo/重定向写入文件到 `$env:USERPROFILE\.woman\docs\` 目录，内容须包含 YAML frontmatter：
   ---
   title: <命令名>
   source: ai-enhanced
   generated: YYYY-MM-DD
   type: <coreutils|powershell|windows>
   ---
5. **终端友好排版**：由于输出在终端渲染，请**避免使用表格和 Markdown 代码块（```）**。推荐用**列表（- 或 1.）、缩进、加粗**来组织内容"#
            .to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        r#"你是一个 Unix 命令行助手 woman AI，默认运行在 POSIX shell（sh）环境中，作为 man 的全平台替代品。
你只有一个工具 **bash**，所有操作都通过它完成。

## 环境说明
- 底层 shell：POSIX `sh`（`sh -c <命令>`）
- 本机命令即系统二进制，无需任何后缀（如 `ls` `cat` `grep` `find` `man` 均直接可用）
- 查看命令手册：`man <name>` 或 `whatis <name>`
- 读文件用 `cat`
- HTTP 请求用 `curl`
- 查找命令路径用 `which` <name>
- 环境变量路径用 `$HOME`（如 `$HOME/.woman/...`），不要用 Windows 的 `$env:USERPROFILE`

## 本地手册资料
本机已有 woman 手册体系，回答命令类问题时**优先查阅本地资料**，再决定是否联网：
- 正式手册：`cat $HOME/.woman/docs/<name>.md`（AI 整理后的中文手册）
- 原始缓存：`cat $HOME/.woman/cache/<name>.txt`
- 列出已有手册：`ls $HOME/.woman/docs/`
- 整理新手册时，遵守 `$HOME/.woman/skills/manual.md` 的章节结构（先 `cat` 读它，再按其模板整理并写入 `docs/` 目录，frontmatter 用 `source: ai-enhanced`）

## 优先级（按回答偏好从高到低）
1. **系统标准命令**（POSIX/GNU 工具）— 本机自带，回答时优先介绍系统版本。
2. **自定义命令** — `woman`（本工具）等，本机特有命令。

## 规则
1. 始终用中文回答
2. 当用户询问某个命令时，先通过 `bash` 获取原始信息（如 `command --help`、`man <name>`、`cat` 读本地手册、`curl` 抓取在线手册），再给出结构化的中文解释
3. 解释应包含：用途、基本语法、常用选项、典型示例
4. 如果用户要求生成或保存手册，用 bash 的 echo/重定向写入文件到 `$HOME/.woman/docs/` 目录，内容须包含 YAML frontmatter：
   ---
   title: <命令名>
   source: ai-enhanced
   generated: YYYY-MM-DD
   type: <coreutils|unknown>
   ---
5. **终端友好排版**：由于输出在终端渲染，请**避免使用表格和 Markdown 代码块（```）**。推荐用**列表（- 或 1.）、缩进、加粗**来组织内容"#
            .to_string()
    }
}

// ============================================================
// 工具定义（OpenAI tools 格式，shell 描述按平台定制）
// ============================================================

fn tools_json() -> String {
    #[cfg(target_os = "windows")]
    {
        r#"[
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
]"#
        .to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        r#"[
  {
    "type": "function",
    "function": {
      "name": "bash",
      "description": "Run a shell command on Unix (sh). Execute any command, script, or program. Returns stdout + stderr.",
      "parameters": {
        "type": "object",
        "properties": {
          "command": {
            "type": "string",
            "description": "The sh command to execute"
          }
        },
        "required": ["command"]
      }
    }
  }
]"#
        .to_string()
    }
}

// ============================================================
// API 消息类型
// ============================================================

/// 发送给 API 的消息（兼容新旧两种 role 格式）
#[derive(Debug, Clone, PartialEq, Serialize)]
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
    ToolCall {
        fc: FunctionCall,
        tool_call_id: Option<String>,
    },
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
            while i < bytes.len() && bytes[i] != b'm' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
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

/// 平台相关危险命令黑名单判定：命中即拦截执行。
/// Windows 拦 PowerShell/cmd 破坏性惯用法；Unix 拦 rm -rf 根目录等。
fn is_dangerous(command: &str) -> bool {
    let lower = command.to_lowercase();
    #[cfg(target_os = "windows")]
    {
        const DANGEROUS: &[&str] = &[
            "rm -rf /",
            "rd /s /q",
            "format ",
            "shutdown",
            "reboot",
            "> NUL",
            "> \\\\.\\",
            "del /f /s",
            "erase /f",
            "remove-item -recurse",
        ];
        DANGEROUS.iter().any(|d| lower.contains(d))
    }
    #[cfg(not(target_os = "windows"))]
    {
        const DANGEROUS: &[&str] = &[
            "rm -rf /",
            "rm -fr /",
            "> /dev/sda",
            "mkfs",
            "dd if=",
            ":(){:|:&};:",
            "shutdown",
            "reboot",
            "halt",
            "poweroff",
        ];
        DANGEROUS.iter().any(|d| lower.contains(d))
    }
}

/// 安全的执行 shell 命令（按平台：Windows pwsh / Unix sh），含危险命令过滤和输出截断
fn run_bash(command: &str) -> String {
    if is_dangerous(command) {
        return "错误：该命令已被安全策略拦截".to_string();
    }

    let (shell, prefix) = platform::shell_runner();
    let mut cmd = Command::new(shell);
    cmd.args(prefix).arg(command);

    match cmd.output().ok() {
        Some(output) => {
            let mut result = String::new();
            if !output.stdout.is_empty() {
                result.push_str(&String::from_utf8_lossy(&output.stdout));
            }
            if !output.stderr.is_empty() {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(&String::from_utf8_lossy(&output.stderr));
            }
            let trimmed = result.trim().to_string();
            if trimmed.is_empty() {
                return "(无输出)".to_string();
            }
            // 截断过大的输出
            // 注意：这个返回值会被塞进上下文的 tool 消息，并在后续每轮请求里反复发送，
            // 因此必须保持精简以节省 token。AI 只需提炼要点，默认保留前面 MAX_OUTPUT 字符足矣。
            const MAX_OUTPUT: usize = 12000;
            if trimmed.len() > MAX_OUTPUT {
                let mut truncated = trimmed[..MAX_OUTPUT].to_string();
                truncated.push_str(&format!("\n...（输出被截断，共 {} 字符）", trimmed.len()));
                return truncated;
            }
            trimmed
        }
        None => "执行失败：无法启动命令".to_string(),
    }
}

// ============================================================
// 流式 API 调用（SSE via curl -N）
// ============================================================

// ============================================================
// curl 请求体写入文件（避免超长命令行 os error 206）
// ============================================================

/// 把 JSON 请求体写入临时文件，返回 `-d @<path>` 参数值。
///
/// Windows `CreateProcess` 命令行长度上限为 32767 字符（os error 206：
/// 文件名或扩展名太长）。当对话历史累积过长时，直接把 body 塞进
/// `-d <body>` 会超出该限制导致 curl.exe 无法启动，因此改用
/// `-d @file` 让 curl 从文件读取请求体。
fn write_body_file(body_str: &str) -> Result<String, String> {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("woman_body_{}.json", std::process::id()));
    std::fs::write(&path, body_str).map_err(|e| format!("写入请求体临时文件失败：{e}"))?;
    Ok(format!("@{}", path.display()))
}

/// 清理临时请求体文件。curl 用 `-d @file` 读取请求体，
/// 必须在 curl 进程结束后再删除，避免读取时文件被移除（竞态）。
fn remove_body_file(arg: &str) {
    if let Some(p) = arg.strip_prefix('@') {
        let _ = std::fs::remove_file(PathBuf::from(p));
    }
}

fn chat_completion_stream(
    provider: &AiProvider,
    messages: &[RequestMessage],
) -> Result<StreamOutcome, String> {
    let url = provider.api_base.trim_end_matches('/').to_string();

    let body = serde_json::json!({
        "model": provider.model,
        "messages": messages,
        "tools": serde_json::from_str::<serde_json::Value>(&tools_json()).unwrap(),
        "tool_choice": "auto",
        "stream": true,
    });

    let body_str = body.to_string();
    let data_arg = write_body_file(&body_str)?;
    let mut child = Command::new(platform::curl_bin())
        .args([
            "-sS",
            "-N",
            "-X",
            "POST",
            &url,
            "-m",
            "120",
            "-H",
            "Content-Type: application/json",
            "-H",
            &format!("Authorization: Bearer {}", provider.api_key),
            "-d",
            &data_arg,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            remove_body_file(&data_arg);
            format!("无法启动 {}：{e}", platform::curl_bin())
        })?;

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
                return Err(format!(
                    "解析流事件失败：{e}\n原始数据：{}",
                    &data[..data.len().min(200)]
                ));
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
                typewrite(
                    &crate::display::ansi_format(&complete),
                    Duration::from_millis(6),
                );
            }
        }
    }

    let _ = child.wait();
    remove_body_file(&data_arg);

    if finish_reason.as_deref() == Some("tool_calls") {
        if let Some(name) = tool_name {
            return Ok(StreamOutcome::ToolCall {
                fc: FunctionCall {
                    name,
                    arguments: tool_args,
                },
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
    let data_arg = write_body_file(&body_str)?;
    let output = Command::new(platform::curl_bin())
        .args([
            "-sS",
            "-X",
            "POST",
            &url,
            "-m",
            "120",
            "-H",
            "Content-Type: application/json",
            "-H",
            &format!("Authorization: Bearer {}", provider.api_key),
            "-d",
            &data_arg,
        ])
        .output()
        .map_err(|e| format!("无法启动 {}：{e}", platform::curl_bin()))?;
    remove_body_file(&data_arg);

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("API 请求失败：{}", err.trim()));
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let resp: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("解析 API 响应失败：{e}"))?;

    if let Some(msg) = resp["error"]["message"].as_str() {
        return Err(format!("API 错误：{msg}"));
    }

    resp["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "API 响应缺少 content".to_string())
}

// ============================================================
// AI 增强整理 + 一次性问答
// ============================================================

/// 把命令原始资料整理成符合 skill 模板的结构化中文手册。
///
/// - `skill`：`~/.woman/skills/manual.md` 的内容（含章节结构模板 + 整理指令）
/// - `raw`：命令原始资料（缓存/源抓取内容）
/// - 返回整理后的 Markdown 文档字符串（含 YAML frontmatter）
pub fn enhance(
    name: &str,
    local_help: Option<&str>,
    online: Option<&str>,
    skill: &str,
    cmd_type: &str,
) -> Result<String, String> {
    let config = Config::load();
    let provider = config.get_provider(None).ok_or_else(|| {
        "未配置 AI 提供者。请编辑 ~/.woman/config.json 添加 ai 配置。".to_string()
    })?;

    let key = provider.api_key.trim();
    if key.is_empty() || key.contains("your-api-key") {
        return Err("API 密钥未配置或为占位符".to_string());
    }

    let today = current_date();

    // 把本地 --help 与在线全文分别标注后一起喂给 AI，
    // 供其按 skill 指令做「本地选项校对」（在线有而本地没有 → 标注不支持）。
    let mut user_msg = format!("命令名：{name}\n命令类型：{cmd_type}\n\n");
    match (local_help, online) {
        (Some(help), Some(onl)) => {
            user_msg.push_str(&format!(
                "【本地 --help / ? 输出（本机选项真值）】\n```\n{}\n```\n\n",
                help.trim()
            ));
            user_msg.push_str(&format!("【在线手册全文】\n{}\n", onl.trim()));
        }
        (Some(help), None) => {
            user_msg.push_str(&format!(
                "【本地 --help / ? 输出（本机选项真值）】\n```\n{}\n```\n",
                help.trim()
            ));
        }
        (None, Some(onl)) => {
            user_msg.push_str(&format!("【在线手册全文】\n{}\n", onl.trim()));
        }
        (None, None) => {
            user_msg.push_str("（未提供原始资料）\n");
        }
    }

    let messages = vec![
        RequestMessage {
            role: "system".into(),
            content: Some(format!(
                "你是一个中文手册整理助手。请严格按照给定的 skill 章节结构模板与整理指令，\
                 把用户提供的命令资料整理成一份**详细完整、可离线参考**的中文手册。\
                 当前日期为 {today}，frontmatter 中的 generated 填这个日期。\n\n{skill}"
            )),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        },
        RequestMessage {
            role: "user".into(),
            content: Some(user_msg),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        },
    ];

    chat_completion(&provider, &messages)
}

/// 一次性问答（`woman -q "<问题>"`）：agent 式自由回答。
///
/// 让 AI 用 bash 工具自行读取 `~/.woman/docs|cache` 等本地资料作为上下文，
/// 只回答一轮即结束。返回最终回答文本（流式打印由调用方处理）。
///
/// 无 AI key 时返回错误（-q 本质依赖 AI）。
pub fn ask_once(question: &str) -> Result<String, String> {
    let config = Config::load();
    let provider = config.get_provider(None).ok_or_else(|| {
        "未配置 AI 提供者。请编辑 ~/.woman/config.json 添加 ai 配置。".to_string()
    })?;

    let key = provider.api_key.trim();
    if key.is_empty() || key.contains("your-api-key") {
        return Err("API 密钥未配置或为占位符（woman -q 需要 AI 能力）".to_string());
    }

    let mut messages: Vec<RequestMessage> = Vec::new();
    messages.push(RequestMessage {
        role: "system".into(),
        content: Some(system_prompt()),
        tool_call_id: None,
        name: None,
        tool_calls: None,
    });
    messages.push(RequestMessage {
        role: "user".into(),
        content: Some(question.to_string()),
        tool_call_id: None,
        name: None,
        tool_calls: None,
    });

    // 工具调用循环（与 run_repl 内核一致，但只跑一轮）
    loop {
        match chat_completion_stream(&provider, &trim_context(&messages)) {
            Ok(StreamOutcome::ToolCall { fc, tool_call_id }) => {
                let cmd = serde_json::from_str::<serde_json::Value>(&fc.arguments)
                    .ok()
                    .and_then(|v| v["command"].as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| fc.arguments.trim_matches('"').to_string());

                println!("\x1b[2m\x1b[38;5;244m$ {}\x1b[0m", cmd);
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
                        "function": { "name": fc.name, "arguments": fc.arguments }
                    })]),
                });
                messages.push(RequestMessage {
                    role: "tool".into(),
                    content: Some(result),
                    tool_call_id: Some(tcid),
                    name: None,
                    tool_calls: None,
                });
            }
            Ok(StreamOutcome::Complete(content)) => {
                let clean = content
                    .replace("<|FunctionCallBegin|>", "")
                    .replace("<|FunctionCallEnd|>", "")
                    .trim()
                    .to_string();
                return Ok(clean);
            }
            Err(e) => return Err(e),
        }
    }
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
    println!("  [\x1b[34m/exit\x1b[0m]             退出对话");
    println!("  [\x1b[34m/help\x1b[0m]           显示此帮助");
    println!("  [\x1b[34m/clear\x1b[0m]          清屏");
    println!("  [\x1b[34m/truncate\x1b[0m]       清除历史，开始新话题");
    println!("  [\x1b[34m/model\x1b[0m]           列出可用模型");
    println!("  [\x1b[34m/model\x1b[0m] <name>    切换到指定模型");
    println!("  [\x1b[34mCtrl+D\x1b[0m]         空输入时退出对话");
    println!("╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌");
}

fn clear_screen() {
    let (kind, args) = platform::clear_screen();
    if kind == "__ansi__" {
        // Unix：直接写 ANSI 清屏码
        let _ = io::stdout().write_all(args[0].as_bytes());
        let _ = io::stdout().flush();
    } else {
        // Windows：cmd /c cls
        let mut cmd = Command::new(kind);
        cmd.args(&args);
        let _ = cmd.status();
    }
}

/// 交互式选择器：↑↓/jk 切换，Enter 确认，Esc 取消。
///
/// 重绘策略：
/// - 进入前记录列表首行的绝对行号，每次重绘用 `MoveTo` 直接定位（不再用
///   `MoveUp` 累积移动，避免任何滚动/光标偏差导致的错位残影）；
/// - 仅在选中项真正变化时才重绘列表（无关按键、鼠标事件不触发重绘）；
/// - 每行行尾写 `\x1b[K` 清到行尾，长名称行缩短后不留残影；
/// - 结束后光标移到列表下方另起一行，列表保留在屏幕上作为选择记录。
fn select_provider(all: &[AiProvider], current: &str) -> Option<usize> {
    if all.len() <= 1 {
        return None;
    }
    let mut sel = all.iter().position(|p| p.name == current).unwrap_or(0);

    // 记录列表起始行（进入 raw 模式前光标所在行）
    let start_row = crossterm::cursor::position()
        .ok()
        .map(|p| p.1)
        .unwrap_or(0);

    enable_raw_mode().ok()?;
    execute!(io::stdout(), Hide).ok()?;

    /// 绘制一帧列表：MoveTo 逐行定位 + 行尾清行，绝对定位不累积偏差
    fn draw_list(all: &[AiProvider], sel: usize, start_row: u16) {
        let mut out = io::stdout();
        for (i, p) in all.iter().enumerate() {
            let _ = execute!(out, MoveTo(0, start_row + i as u16));
            if i == sel {
                let _ = write!(
                    out,
                    "\x1b[48;5;208m\x1b[30m {} · {} \x1b[0m\x1b[K",
                    p.name, p.model
                );
            } else {
                let _ = write!(out, "  {} · {}\x1b[K", p.name, p.model);
            }
        }
        let _ = out.flush();
    }

    draw_list(all, sel, start_row);

    let result = loop {
        match read() {
            Ok(Event::Key(ke)) if ke.kind == KeyEventKind::Press => match ke.code {
                KeyCode::Up | KeyCode::Char('k') if sel > 0 => {
                    sel -= 1;
                    draw_list(all, sel, start_row);
                }
                KeyCode::Down | KeyCode::Char('j') if sel + 1 < all.len() => {
                    sel += 1;
                    draw_list(all, sel, start_row);
                }
                KeyCode::Enter => break Some(sel),
                KeyCode::Esc => break None,
                _ => {}
            },
            Ok(_) => {} // 鼠标 / Resize 等事件不触发重绘
            Err(_) => break None,
        }
    };

    // 结束：光标移到列表下方另起一行，后续输出从列表之下开始，不与列表重叠
    let mut out = io::stdout();
    let _ = execute!(out, MoveTo(0, start_row + all.len() as u16));
    let _ = writeln!(out);
    disable_raw_mode().ok()?;
    execute!(out, Show).ok();
    result
}

// ============================================================
// 上下文裁剪（节省 token，同时尽量保留准确性）
// ============================================================

/// 上下文总字符预算。超过后开始裁剪最旧的纯对话消息。
/// 估算方式：约 1 token ≈ 2~4 字符，故取保守值 4 字符/token。
/// 120k 字符 ≈ 最多约 30k token，落在常见模型上下文窗口内，
/// 又给工具结果和最近对话留足空间。
const MAX_CTX_CHARS: usize = 120_000;

/// 返回给定消息估算的字符数（tokens ≈ chars/4）。
fn msg_chars(m: &RequestMessage) -> usize {
    let mut n = m.content.as_deref().map_or(0, |s| s.len());
    if let Some(tcs) = &m.tool_calls {
        for tc in tcs {
            n += tc["function"]["arguments"].as_str().map_or(0, |s| s.len());
        }
    }
    n
}

/// 上下文裁剪：在发送请求前调用，在 token 预算内尽量保留准确性。
///
/// 策略（由保准到宽松）：
/// 1. system（index 0）永远保留。
/// 2. 从最新消息往回收集，优先保留最近的、工具相关的消息——
///    这些是 AI 回答当前问题的依据，准确性最好。
/// 3. 用字符预算兜底，保证裁剪后总大小绝不超过 MAX_CTX_CHARS。
/// 4. 若某条消息单条就超预算，丢弃它（但最新一条 user 提问除外，强制保留）。
fn trim_context(messages: &[RequestMessage]) -> Vec<RequestMessage> {
    if messages.len() <= 1 {
        return messages.to_vec();
    }

    let total: usize = messages.iter().map(msg_chars).sum();
    if total <= MAX_CTX_CHARS {
        return messages.to_vec();
    }

    // system 无条件保留
    let mut out: Vec<RequestMessage> = Vec::with_capacity(messages.len());
    out.push(messages[0].clone());
    let mut budget = MAX_CTX_CHARS.saturating_sub(msg_chars(&messages[0]));

    // 从最新往回收集，填满预算。优先保留最近的工具相关消息。
    let mut recent: Vec<RequestMessage> = Vec::new();
    for m in messages[1..].iter().rev() {
        let c = msg_chars(m);
        if c > budget {
            continue; // 单条超预算
        }
        budget -= c;
        recent.push(m.clone());
    }

    // 兜底：最新一条 user 提问必须保留（若被上面的预算判断丢弃）
    if let Some(last) = messages.last() {
        if last.role == "user" && !recent.iter().any(|m| m == last) {
            // 把预算里最小的几条顶掉，也要保住提问
            if recent.iter().any(|m| m.role != "user") {
                if let Some(pos) = recent.iter().position(|m| m.role != "user") {
                    recent.remove(pos);
                }
            }
            recent.push(last.clone());
        }
    }

    // recent 是倒序，反转成时间正序
    recent.reverse();
    out.extend(recent);
    out
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
        content: Some(system_prompt()),
        tool_call_id: None,
        name: None,
        tool_calls: None,
    });

    println!("\n🤖 WOMAN AI · \x1b[38;5;208m{}\x1b[0m", current.name);
    println!("💡 输入 [\x1b[34m/exit\x1b[0m] 退出（或空输入 Ctrl+D） · [\x1b[34m/help\x1b[0m] 查看帮助\n");

    loop {
        // 进入多行编辑器读取输入（raw 模式，支持斜杠候选抽屉、Ctrl+J 换行）
        let line = match crate::editor::read_input() {
            Some(l) => l,
            None => {
                println!("\n再见！");
                break;
            }
        };
        if line.is_empty() {
            continue;
        }

        // ---- REPL 命令 ----
        // 斜杠命令仅当「单行且以 / 开头」时生效；多行输入一律按普通消息发给 AI
        let is_slash_cmd = line.starts_with('/') && !line.contains('\n');
        if is_slash_cmd {
            let parts: Vec<&str> = line.split_whitespace().collect();
            match parts[0] {
                "/exit" => {
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
                        if *name != current.name && all_providers.iter().any(|p| p.name == *name) {
                            for ap in all_providers.iter_mut() {
                                ap.default = false;
                            }
                            if let Some(ap) = all_providers.iter_mut().find(|ap| ap.name == *name) {
                                ap.default = true;
                            }
                            crate::config::Config::load().set_default(name);
                            current = all_providers
                                .iter()
                                .find(|ap| ap.name == *name)
                                .unwrap()
                                .clone();
                            messages.truncate(1);
                            println!(
                                "\x1b[2m已切换到 \x1b[0m\x1b[38;5;208m{}\x1b[0m \x1b[2m({})\x1b[0m",
                                current.name, current.model
                            );
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

        // 用户问题打印之后空一行，让 AI 回应与问题上下分离
        println!();

        // 工具调用循环（流式 SSE）
        loop {
            // 每次请求前做上下文裁剪，控制 token 增长
            match chat_completion_stream(&current, &trim_context(&messages)) {
                Ok(StreamOutcome::ToolCall { fc, tool_call_id }) => {
                    // 从 arguments 中提取 command 参数
                    let cmd = serde_json::from_str::<serde_json::Value>(&fc.arguments)
                        .ok()
                        .and_then(|v| v["command"].as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| fc.arguments.trim_matches('"').to_string());

                    println!("\x1b[2m\x1b[38;5;244m$ {}\x1b[0m", cmd);
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
                    // AI 回答结束后空一行，与下一问题分隔
                    println!();
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

// ============================================================
// 单元测试：上下文裁剪
// ============================================================

#[cfg(test)]
mod context_tests {
    use super::*;

    fn user(c: &str) -> RequestMessage {
        RequestMessage {
            role: "user".into(),
            content: Some(c.into()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }
    }
    fn assistant(c: &str) -> RequestMessage {
        RequestMessage {
            role: "assistant".into(),
            content: Some(c.into()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }
    }
    fn tool(id: &str, c: &str) -> RequestMessage {
        RequestMessage {
            role: "tool".into(),
            content: Some(c.into()),
            tool_call_id: Some(id.into()),
            name: None,
            tool_calls: None,
        }
    }
    #[test]
    fn small_context_is_untouched() {
        // 小上下文不裁剪，原样返回
        let mut v = vec![RequestMessage {
            role: "system".into(),
            content: Some("sys".into()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        v.extend([user("hi"), assistant("hi")]);
        let out = trim_context(&v);
        assert_eq!(out, v);
    }

    #[test]
    fn drops_old_pure_chat_keeps_latest_user() {
        // 制造超预算：大量旧纯对话 + 最新的 user
        let old: Vec<RequestMessage> = (0..2000)
            .map(|i| {
                if i % 2 == 0 {
                    user("x")
                } else {
                    assistant("y")
                }
            })
            .collect();
        let mut v = vec![RequestMessage {
            role: "system".into(),
            content: Some("sys".into()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        v.extend(old);
        v.push(user("最新问题"));
        let out = trim_context(&v);
        // system 保留
        assert_eq!(out[0].role, "system");
        // 最新 user 保留
        assert!(out.iter().any(|m| m.content.as_deref() == Some("最新问题")));
        // 总长度低于预算
        let total: usize = out.iter().map(msg_chars).sum();
        assert!(total <= MAX_CTX_CHARS);
    }

    #[test]
    fn keeps_recent_tool_results() {
        let mut v = vec![RequestMessage {
            role: "system".into(),
            content: Some("sys".into()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        // 超预算的旧工具结果 + 最新一个工具结果
        for i in 0..5000 {
            let c = format!("result_{}", i);
            // 大内容模拟
            let big = format!("{:0<2000}", c);
            v.push(tool(&format!("t{}", i), &big));
        }
        // 最新的工具结果应被保留（在最近回合内）
        let last_tool = tool("t_last", "最新结果");
        let q = user("当前问题");
        v.push(q.clone());
        v.push(last_tool.clone());
        let out = trim_context(&v);
        assert_eq!(out[0].role, "system");
        assert!(out.iter().any(|m| m == &last_tool));
        let total: usize = out.iter().map(msg_chars).sum();
        assert!(total <= MAX_CTX_CHARS);
    }

    #[test]
    fn keeps_recent_tool_cycle() {
        // 造一条超预算的旧历史
        let old: Vec<RequestMessage> = (0..3000)
            .map(|i| {
                if i % 2 == 0 {
                    user(&format!("{:0<3000}", "旧问"))
                } else {
                    assistant(&format!("{:0<3000}", "旧答"))
                }
            })
            .collect();
        let mut v = vec![RequestMessage {
            role: "system".into(),
            content: Some("sys".into()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        v.extend(old);
        // 最近一个完整回合：提问 -> 工具调用 -> 工具结果 -> 最终回答
        let q = user("当前问题");
        let call = assistant_toolcall_help("ls.exe");
        let res = tool("call_1", "文件列表...");
        let ans = assistant("这是 ls 的用法说明...");
        v.push(q.clone());
        v.push(call.clone());
        v.push(res.clone());
        v.push(ans.clone());

        let out = trim_context(&v);
        assert_eq!(out[0].role, "system");
        // 当前问提、工具调用、工具结果、最终回答全部保留
        assert!(out.iter().any(|m| m == &q));
        assert!(out.iter().any(|m| m == &call));
        assert!(out.iter().any(|m| m == &res));
        assert!(out.iter().any(|m| m == &ans));
        let total: usize = out.iter().map(msg_chars).sum();
        assert!(total <= MAX_CTX_CHARS);
    }

    // 构造带 tool_calls 的助手消息
    fn assistant_toolcall_help(cmd: &str) -> RequestMessage {
        let calls = serde_json::json!({
            "id": "call_1",
            "type": "function",
            "function": { "name": "bash", "arguments": format!(r#"{{"command":"{}"}}"#, cmd) }
        });
        RequestMessage {
            role: "assistant".into(),
            content: None,
            tool_call_id: None,
            name: None,
            tool_calls: Some(vec![calls]),
        }
    }
}
