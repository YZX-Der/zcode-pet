//! 宠物路径解析与状态文件读写。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 状态文件反序列化结构 —— 对应 docs/03-state-protocol.md
#[derive(Clone, serde::Deserialize)]
pub struct StateFile {
    pub session_id: String,
    pub state: String,
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub project_dir: Option<String>,
    pub ts: i64,
}

pub fn home() -> PathBuf {
    dirs::home_dir().expect("cannot find home directory")
}

pub fn state_dir() -> PathBuf {
    home().join(".zcode-pet").join("state")
}

pub fn user_pets_dir() -> PathBuf {
    home().join(".zcode-pet").join("pets")
}

/// 内置宠物目录（开发期从项目 assets 加载，否则回退到用户目录）。
pub fn builtin_pets_dir() -> PathBuf {
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets")
        .join("pets");
    if dev.exists() {
        dev
    } else {
        user_pets_dir()
    }
}

/// Codex 宠物目录（兼容 Codex 社区宠物资源）。
pub fn codex_pets_dir() -> PathBuf {
    home().join(".codex").join("pets")
}

/// 解析指定宠物的目录（优先 zcode-pet 用户目录，其次 Codex 目录，最后内置）。
pub fn pet_dir(pet_name: &str) -> PathBuf {
    let user = user_pets_dir().join(pet_name);
    if user.exists() {
        return user;
    }
    let codex = codex_pets_dir().join(pet_name);
    if codex.exists() {
        return codex;
    }
    builtin_pets_dir().join(pet_name)
}

/// 返回宠物精灵表绝对路径（webp 或 png）。
pub fn sheet_path(pet_name: &str) -> Option<PathBuf> {
    let dir = pet_dir(pet_name);
    ["webp", "png"]
        .iter()
        .map(|ext| dir.join(format!("spritesheet.{ext}")))
        .find(|p| p.exists())
}

/// 列出所有可用宠物名（去重，扫描优先级：zcode-pet 用户目录 → Codex 目录 → 内置）。
pub fn list_pets() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for dir in [user_pets_dir(), codex_pets_dir(), builtin_pets_dir()] {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if entry.path().is_dir() && seen.insert(name.to_string()) {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }
    names
}

/// 从状态文件路径提取 session_id（文件名去掉 .json）。
pub fn session_id_from_path(path: &Path) -> Option<String> {
    path.file_stem()?.to_str().map(|s| s.to_string())
}

/// 根据 session_id 生成窗口 label（取前 12 字符防止过长）。
pub fn session_label(session_id: &str) -> String {
    let prefix = &session_id[..session_id.len().min(12)];
    format!("pet-{prefix}")
}

/// 识别 ZCode 当前活跃会话：rollout 目录下最近修改的 sess_* 文件对应的 session_id。
///
/// rollout 文件（~/.zcode/cli/rollout/model-io-sess_<id>.jsonl）每次模型 IO 都会更新，
/// 比状态文件 ts 和 bot-state.activeTaskId 更实时。subagent 文件不算用户会话，排除。
/// 目录不可用时返回 None。
pub fn current_session_id() -> Option<String> {
    let dir = home().join(".zcode").join("cli").join("rollout");
    let mut latest: Option<(SystemTime, String)> = None;
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        // model-io-sess_<id>.jsonl -> sess_<id>
        let session_id = match name
            .strip_prefix("model-io-")
            .and_then(|s| s.strip_suffix(".jsonl"))
        {
            Some(s) if s.starts_with("sess_") => s.to_string(),
            _ => continue,
        };
        if session_id.contains("subagent") {
            continue;
        }
        let mtime = match entry.metadata().and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if latest.as_ref().map_or(true, |(t, _)| mtime > *t) {
            latest = Some((mtime, session_id));
        }
    }
    latest.map(|(_, id)| id)
}
