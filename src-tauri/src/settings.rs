//! 用户设置：持久化到 ~/.zcode-pet/config.json，支持运行时动态修改。

use crate::pet;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// 用户可配置项（通过设置窗口修改）
#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    /// 宠物显示缩放（0.3 ~ 1.5）
    #[serde(default = "default_scale")]
    pub scale: f64,
    /// 宠物不透明度（0.5 ~ 1.0）
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    /// 当前选中的宠物名
    #[serde(default = "default_pet")]
    pub pet: String,
    /// 是否始终置顶
    #[serde(default = "default_true")]
    pub always_on_top: bool,
    /// 点击激活的目标应用优先级列表
    #[serde(default = "default_activate_targets")]
    pub activate_targets: Vec<String>,
    /// 桌宠全局开关（false=显示，true=隐藏）
    #[serde(default)]
    pub pet_hidden: bool,
}

fn default_scale() -> f64 {
    1.0
}
fn default_opacity() -> f64 {
    1.0
}
fn default_pet() -> String {
    "zbuddy".to_string()
}
fn default_true() -> bool {
    true
}
fn default_activate_targets() -> Vec<String> {
    vec![
        "ZCode".into(),
        "iTerm2".into(),
        "Terminal".into(),
    ]
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            scale: default_scale(),
            opacity: default_opacity(),
            pet: default_pet(),
            always_on_top: default_true(),
            activate_targets: default_activate_targets(),
            pet_hidden: false,
        }
    }
}

/// 配置文件路径：~/.zcode-pet/config.json
pub fn config_path() -> PathBuf {
    pet::home().join(".zcode-pet").join("config.json")
}

/// 读取配置（文件不存在则返回默认值）
pub fn load() -> Settings {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// 原子写入配置（tmp + rename）
pub fn save(settings: &Settings) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Tauri 命令：读取设置
#[tauri::command]
pub fn get_settings() -> Settings {
    load()
}

/// Tauri 命令：保存设置并应用
#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<Settings, String> {
    // 逐字段合并到现有配置：pet_hidden 由后端管理（桌宠开关），
    // 前端整体保存设置时不得覆盖它
    let mut merged = load();
    merged.scale = settings.scale;
    merged.opacity = settings.opacity;
    merged.pet = settings.pet.clone();
    merged.always_on_top = settings.always_on_top;
    merged.activate_targets = settings.activate_targets;
    save(&merged)?;
    // 更新全局 pet_name
    let state = app.state::<crate::AppState>();
    *state.pet_name.lock().unwrap() = settings.pet.clone();
    // 重建桌宠窗口以应用新 scale/opacity/置顶等参数
    crate::window::recreate_all(&app);
    Ok(merged)
}

/// Tauri 命令：列出可用宠物名（供设置窗口下拉选择）
#[tauri::command]
pub fn list_pets() -> Vec<String> {
    pet::list_pets()
}
