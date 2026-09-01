// catalog.rs — models.dev 目录缓存 / 拉取 / 精简解析
// models.dev（https://models.dev）是 opencode / pi / deepseek-harness 同源的开源
// LLM 厂商与模型目录，MIT 协议。通过 curl.exe 拉取 https://models.dev/api.json 并缓存到
// `~/.woman/models/catalog.json`，保证离线可用。

use std::collections::HashMap;
use std::process::Command;

use crate::config::Config;
use crate::platform;

// ============================================================
// 精简 serde 结构（只取 woman 需要的字段，其余未知字段由 serde 自动忽略）
// ============================================================

/// 单个提供者（对应 models.dev /api.json 顶层的一个条目的价值字段）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Provider {
    /// 提供者 ID（如 "deepseek"），作为 AiProvider.name 与缓存 key
    pub id: String,
    /// 显示名称（如 "DeepSeek"）
    pub name: String,
    /// AI SDK 适配包名；`@ai-sdk/openai-compatible` 表示 OpenAI 兼容
    pub npm: String,
    /// 官方文档链接
    #[serde(default)]
    pub doc: String,
    /// OpenAI 兼容的 API base URL（如 "https://api.deepseek.com"）；原生 SDK 提供者可能没有
    #[serde(default)]
    pub api: Option<String>,
    /// 该提供者下的模型表：key 为模型 ID
    #[serde(default)]
    pub models: HashMap<String, Model>,
}

/// 模型精简结构（用于向导展示：名称 / 上下文 / 价格 / 能力 / 状态）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Model {
    /// 模型 ID（同时也是 models 表的 key）
    pub id: String,
    /// 模型显示名称
    pub name: String,
    /// 是否支持推理（chain-of-thought）
    #[serde(default)]
    pub reasoning: bool,
    /// 是否支持工具调用
    #[serde(default)]
    pub tool_call: bool,
    /// 状态：alpha / beta / deprecated（缺失视为稳定）
    #[serde(default)]
    pub status: Option<String>,
    /// 上下文 / 输出 token 限制
    #[serde(default)]
    pub limit: Option<Limit>,
    /// 每百万 token 价格（USD）
    #[serde(default)]
    pub cost: Option<Cost>,
}

/// token 限制
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Limit {
    #[serde(default)]
    pub context: Option<u64>,
    #[serde(default)]
    pub output: Option<u64>,
}

/// 价格（USD / 1M tokens）
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Cost {
    #[serde(default)]
    pub input: Option<f64>,
    #[serde(default)]
    pub output: Option<f64>,
}

/// 整个目录：顶层为「提供者 ID → Provider」的映射
pub type Catalog = HashMap<String, Provider>;

/// 标识 OpenAI 兼容提供者的 npm 包名
pub const OPENAI_COMPAT_NPM: &str = "@ai-sdk/openai-compatible";

/// 判断该提供者是否可直接用于 woman 的 `AiProvider`（OpenAI 兼容 chat/completions）。
/// 要求：npm 为 openai-compatible 且带 api base URL。
pub fn is_openai_compatible(p: &Provider) -> bool {
    p.npm == OPENAI_COMPAT_NPM && p.api.is_some()
}

// ============================================================
// 拉取与缓存
// ============================================================

const CATALOG_URL: &str = "https://models.dev/api.json";

/// 用 curl 拉取目录原文（二进制按平台：curl.exe / curl）
fn fetch_catalog_raw() -> Result<String, String> {
    let output = Command::new(platform::curl_bin())
        .args(["-sS", "-L", CATALOG_URL])
        .output()
        .map_err(|e| format!("无法执行 {}：{e}", platform::curl_bin()))?;
    if !output.status.success() {
        return Err(format!(
            "拉取 models.dev 目录失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 解析目录 JSON
fn parse_catalog(raw: &str) -> Result<Catalog, String> {
    serde_json::from_str(raw).map_err(|e| format!("解析 models.dev 目录失败：{e}"))
}

/// 获取目录（缓存优先，离线可用）。
///
/// 三级策略：
/// 1. 有缓存且不强制刷新 → 直接用（离线可用）。
/// 2. 无缓存 → 拉取并写入缓存。
/// 3. 拉取失败且无缓存 → 报错，提示先联网跑一次。
pub fn ensure_catalog(refresh: bool) -> Result<Catalog, String> {
    let path = Config::catalog_path();

    // 1. 缓存优先（未强制刷新）
    if !refresh {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(cat) = parse_catalog(&content) {
                return Ok(cat);
            }
        }
    }

    // 2. 拉取（作为源头真值）
    let raw = fetch_catalog_raw()?;

    // 3. 解析（确保内容合法后再写缓存，避免缓存脏数据）
    let cat = parse_catalog(&raw)?;
    let _ = std::fs::create_dir_all(Config::models_dir());
    let _ = std::fs::write(&path, &raw);
    Ok(cat)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> &'static str {
        r#"{
  "deepseek": {
    "id": "deepseek",
    "name": "DeepSeek",
    "npm": "@ai-sdk/openai-compatible",
    "doc": "https://api-docs.deepseek.com",
    "api": "https://api.deepseek.com",
    "models": {
      "deepseek-v4-flash": {
        "id": "deepseek-v4-flash",
        "name": "DeepSeek V4 Flash",
        "reasoning": true,
        "tool_call": true,
        "limit": { "context": 1000000, "output": 384000 },
        "cost": { "input": 0.14, "output": 0.28 }
      }
    }
  },
  "anthropic": {
    "id": "anthropic",
    "name": "Anthropic",
    "npm": "@ai-sdk/anthropic",
    "doc": "https://docs.anthropic.com",
    "models": {
      "claude-opus": {
        "id": "claude-opus",
        "name": "Claude Opus",
        "reasoning": false,
        "tool_call": true,
        "limit": { "context": 200000, "output": 16000 }
      }
    }
  }
}"#
    }

    #[test]
    fn parse_catalog_ok() {
        let cat = parse_catalog(sample()).unwrap();
        assert_eq!(cat.len(), 2);
        let ds = &cat["deepseek"];
        assert_eq!(ds.name, "DeepSeek");
        // 原样保留未知字段（此处无未知字段，仅验证关键字段）
        let m = &ds.models["deepseek-v4-flash"];
        assert_eq!(m.name, "DeepSeek V4 Flash");
        assert_eq!(m.limit.as_ref().unwrap().context, Some(1_000_000));
        assert_eq!(m.cost.as_ref().unwrap().input, Some(0.14));
    }

    #[test]
    fn openai_compat_filter() {
        let cat = parse_catalog(sample()).unwrap();
        assert!(is_openai_compatible(&cat["deepseek"])); // openai-compatible + api
        assert!(!is_openai_compatible(&cat["anthropic"])); // 原生 SDK，无 api
    }
}
