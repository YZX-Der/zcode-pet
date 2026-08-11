//! ZCode hooks 一键安装：部署 hook 脚本 + 合并写入 config.json。
//!
//! hook 脚本通过 include_str! 嵌入 binary，不依赖外部文件。
//! config.json 合并逻辑与 scripts/install.sh 等价（幂等，自动备份）。

use crate::pet;
use serde::Serialize;
use std::path::PathBuf;

const HOOK_SCRIPT: &str = include_str!("../../scripts/zcode-hook");

/// 需要注册的 ZCode hook 事件。
const EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PostToolUseFailure",
    "Stop",
];

#[derive(Serialize)]
pub struct InstallStatus {
    pub installed: bool,
}

/// ZCode 配置文件路径。
fn zcode_config_path() -> PathBuf {
    pet::home().join(".zcode").join("cli").join("config.json")
}

/// hook 脚本部署路径。
fn hook_bin_path() -> PathBuf {
    pet::home().join(".zcode-pet").join("bin").join("zcode-hook")
}

/// 检查 hooks 是否已安装：hook 脚本存在 + config.json 有 hooks 配置。
#[tauri::command]
pub fn is_hooks_installed() -> bool {
    if !hook_bin_path().exists() {
        return false;
    }
    let config = zcode_config_path();
    let Ok(content) = std::fs::read_to_string(&config) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };
    json.get("hooks").is_some()
}

/// 一键安装 hooks：部署脚本 + 合并 config.json（幂等，自动备份）。
#[tauri::command]
pub fn install_hooks() -> Result<(), String> {
    let pet_home = pet::home().join(".zcode-pet");
    let bin_dir = pet_home.join("bin");
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(pet_home.join("state")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(pet_home.join("pets")).map_err(|e| e.to_string())?;

    // 部署 hook 脚本
    let hook_path = hook_bin_path();
    std::fs::write(&hook_path, HOOK_SCRIPT).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hook_path)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).map_err(|e| e.to_string())?;
    }

    log::info!("hook script deployed to {}", hook_path.display());

    // 合并 config.json（幂等 + 自动备份）
    let config_path = zcode_config_path();
    merge_hooks_config(&config_path, &hook_path)?;

    Ok(())
}

/// 合并 hooks 配置到 config.json（等价于 install.sh 的 Python 逻辑）。
fn merge_hooks_config(config_path: &PathBuf, hook_path: &PathBuf) -> Result<(), String> {
    // 读取现有配置（不存在则空对象）
    let mut config: serde_json::Value = if config_path.exists() {
        // 自动备份
        let backup = config_path.with_file_name(format!(
            "config.json.bak-{}",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        ));
        let _ = std::fs::copy(config_path, &backup);
        log::info!("backup: {}", backup.display());

        let content = std::fs::read_to_string(config_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())?
    } else {
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        serde_json::json!({})
    };

    // 写入 hooks 配置
    let hook_path_str = hook_path.display().to_string();
    let hook_obj = serde_json::json!({
        "type": "process",
        "command": hook_path_str,
        "args": [""],  // 占位，下面按事件填充
        "timeoutMs": 2000,
    });

    if config.get("hooks").is_none() {
        config["hooks"] = serde_json::json!({});
    }
    config["hooks"]["enabled"] = serde_json::json!(true);
    if config["hooks"].get("events").is_none() {
        config["hooks"]["events"] = serde_json::json!({});
    }

    for ev in EVENTS {
        let mut hook = hook_obj.clone();
        hook["args"] = serde_json::json!([ev]);
        config["hooks"]["events"][ev] = serde_json::json!([{ "hooks": [hook] }]);
    }

    // 原子写入
    let content = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    let tmp = config_path.with_extension("json.tmp");
    std::fs::write(&tmp, &content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, config_path).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(config_path, std::fs::Permissions::from_mode(0o600));
    }

    log::info!("wrote {} event hooks (enabled=true)", EVENTS.len());
    Ok(())
}
