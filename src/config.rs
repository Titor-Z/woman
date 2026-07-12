// config.rs — 管理 ~/.woman/config.json
// 支持多个 AI 提供者配置

use std::path::PathBuf;
use std::fs;
use serde::{Deserialize, Serialize};

// ============================================================
// AI 提供者配置
// ============================================================

/// 单个 AI 提供者的配置信息
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AiProvider {
    /// 显示名称，用于区分多个 AI（如 "doubao"、"deepseek"）
    pub name: String,
    /// API 入口地址
    pub api_base: String,
    /// API 密钥
    pub api_key: String,
    /// 模型名称
    pub model: String,
    /// 是否默认选中
    #[serde(default)]
    pub default: bool,
}

// ============================================================
// 顶层配置
// ============================================================

/// config.json 的顶层结构
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    /// AI 提供者列表（支持多个）
    #[serde(default)]
    pub ai: Vec<AiProvider>,
}

impl Config {
    /// 获取 woman 家目录：~/.woman/
    pub fn home_dir() -> PathBuf {
        let home = dirs::home_dir().expect("无法获取用户主目录");
        home.join(".woman")
    }

    /// 确保所需的目录结构存在
    pub fn ensure_dirs() {
        let home = Self::home_dir();
        fs::create_dir_all(home.join("docs")).ok();
        fs::create_dir_all(home.join("cache")).ok();
    }

    /// docs/ 目录
    pub fn docs_dir() -> PathBuf {
        Self::home_dir().join("docs")
    }

    /// cache/ 目录
    pub fn cache_dir() -> PathBuf {
        Self::home_dir().join("cache")
    }

    /// 配置文件路径
    pub fn config_path() -> PathBuf {
        Self::home_dir().join("config.json")
    }

    /// 加载配置，不存在则创建默认空配置
    pub fn load() -> Config {
        let path = Self::config_path();
        if !path.exists() {
            let cfg = Config { ai: Vec::new() };
            cfg.save();
            return cfg;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Config { ai: Vec::new() },
        };
        let mut cfg: Config = serde_json::from_str(&content).unwrap_or_else(|e| {
            eprintln!("警告：config.json 解析失败（{}），使用默认配置", e);
            Config { ai: Vec::new() }
        });
        if !cfg.ai.is_empty() && cfg.ai.iter().all(|p| !p.default) {
            cfg.ai[0].default = true;
            cfg.save();
        }
        cfg
    }

    /// 按名称获取 AI 提供者，不传 name 则返回 default 标记的，无标记则取第一个
    pub fn get_provider(&self, name: Option<&str>) -> Option<&AiProvider> {
        match name {
            Some(n) => self.ai.iter().find(|p| p.name == n),
            None => self.ai.iter().find(|p| p.default).or(self.ai.first()),
        }
    }

    /// 将指定提供者设为 default 并保存
    pub fn set_default(&mut self, name: &str) {
        for p in &mut self.ai {
            p.default = p.name == name;
        }
        self.save();
    }

    /// 保存配置到文件
    pub fn save(&self) {
        let content = serde_json::to_string_pretty(self).unwrap_or_default();
        fs::write(Self::config_path(), content).ok();
    }
}
