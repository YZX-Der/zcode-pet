//! Dashboard 面板的 Tauri 命令。

use crate::pet;
use crate::window::compute_effective_state;
use serde::Serialize;

#[derive(Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub state: String,
    pub effective_state: String,
    pub project: Option<String>,
    pub ts: i64,
}

#[derive(Serialize)]
pub struct PetSheetInfo {
    pub sheet_path: String,
}

/// 列出所有活跃会话及其当前状态（供 Dashboard 会话页展示）。
#[tauri::command]
pub fn list_sessions() -> Vec<SessionInfo> {
    let state_dir = pet::state_dir();
    let now = chrono::Utc::now().timestamp();
    let mut sessions = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&state_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "json") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(sf) = serde_json::from_str::<pet::StateFile>(&content) {
                    sessions.push(SessionInfo {
                        effective_state: compute_effective_state(&sf.state, sf.ts, now),
                        session_id: sf.session_id,
                        state: sf.state,
                        project: sf.project,
                        ts: sf.ts,
                    });
                }
            }
        }
    }
    sessions
}

/// 返回指定宠物的精灵表绝对路径（供 Dashboard 预览使用）。
#[tauri::command]
pub fn get_pet_sheet(pet_name: String) -> Result<PetSheetInfo, String> {
    let path = pet::sheet_path(&pet_name)
        .ok_or_else(|| format!("pet '{pet_name}' sprite not found"))?;
    Ok(PetSheetInfo {
        sheet_path: path.display().to_string(),
    })
}
