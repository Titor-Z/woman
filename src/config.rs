// config.rs — 管理 ~/.woman/config.json
// 支持多个 AI 提供者配置

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
        fs::create_dir_all(home.join("skills")).ok();
        fs::create_dir_all(Self::models_dir()).ok();
    }

    /// skills/ 目录（存放手册整理 skill，如 manual.md）
    pub fn skills_dir() -> PathBuf {
        Self::home_dir().join("skills")
    }

    /// coreutils 命令清单缓存文件路径（首次生成，供命令分类，仅 Windows）
    #[cfg(target_os = "windows")]
    pub fn coreutils_list_path() -> PathBuf {
        Self::home_dir().join("coreutils-list.txt")
    }

    /// models/ 目录（存放 models.dev 目录缓存）
    pub fn models_dir() -> PathBuf {
        Self::home_dir().join("models")
    }

    /// models.dev 目录缓存文件路径
    pub fn catalog_path() -> PathBuf {
        Self::models_dir().join("catalog.json")
    }

    /// docs/ 目录
    pub fn docs_dir() -> PathBuf {
        Self::home_dir().join("docs")
    }

    /// cache/ 目录
    #[allow(dead_code)]
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

    /// 追加（或替换同名）一个 AI 提供者，并将其设为 default 后落盘。
    /// 返回是否实际追加了新的（同名视为替换，返回 false）。
    pub fn add_provider(&mut self, provider: AiProvider) -> bool {
        let name = provider.name.clone();
        let existed = self.ai.iter().any(|p| p.name == name);
        if existed {
            // 同名替换：保留原位置，更新字段
            if let Some(p) = self.ai.iter_mut().find(|p| p.name == name) {
                p.api_base = provider.api_base;
                p.api_key = provider.api_key;
                p.model = provider.model;
            }
        } else {
            // 新建提供者，追加到列表
            let mut p = provider;
            p.default = true;
            self.ai.push(p);
        }
        // 其它提供者均取消 default
        for p in &mut self.ai {
            p.default = p.name == *name;
        }
        self.save();
        !existed
    }

    /// 保存配置到文件
    pub fn save(&self) {
        let content = serde_json::to_string_pretty(self).unwrap_or_default();
        fs::write(Self::config_path(), content).ok();
    }
}
