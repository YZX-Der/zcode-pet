//! Dashboard 面板的 Tauri 命令。

use crate::pet;
use crate::settings;
use crate::window::{compute_effective_state, is_active_state};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::AppHandle;

#[derive(Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub state: String,
    pub effective_state: String,
    pub project: Option<String>,
    /// 任务名（ZCode tasks-index.sqlite 中的会话标题，fallback 项目名）
    pub title: String,
    /// 该会话的桌宠是否开启
    pub pet_enabled: bool,
    pub ts: i64,
}

#[derive(Serialize)]
pub struct PetSheetInfo {
    pub sheet_path: String,
}

/// ZCode 会话索引数据库路径（任务标题来源）。
fn tasks_index_db() -> PathBuf {
    pet::home().join(".zcode").join("v2").join("tasks-index.sqlite")
}

/// 批量查询任务标题：session_id → title。
/// 数据库不存在/打不开/查不到时返回空映射，调用方 fallback 到项目名。
fn fetch_task_titles(session_ids: &[String]) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    if session_ids.is_empty() {
        return titles;
    }
    let conn = match rusqlite::Connection::open_with_flags(
        tasks_index_db(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(c) => c,
        Err(_) => return titles,
    };
    let placeholders = vec!["?"; session_ids.len()].join(",");
    let sql = format!(
        "SELECT task_id, title FROM tasks WHERE task_id IN ({placeholders}) AND title != ''"
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return titles;
    };
    let params = rusqlite::params_from_iter(session_ids.iter());
    if let Ok(rows) = stmt.query_map(params, |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) {
        for row in rows.flatten() {
            titles.insert(row.0, row.1);
        }
    }
    titles
}

/// 列出所有执行中的会话及其状态（供 Dashboard 会话页展示）。
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
                    let effective = compute_effective_state(&sf.state, sf.ts, now);
                    // 仅展示执行中的会话（活跃 = 有任务在执行）
                    if !is_active_state(&effective) {
                        continue;
                    }
                    sessions.push(SessionInfo {
                        effective_state: effective,
                        session_id: sf.session_id.clone(),
                        state: sf.state,
                        project: sf.project,
                        title: String::new(),
                        pet_enabled: true,
                        ts: sf.ts,
                    });
                }
            }
        }
    }

    // 批量补充任务标题（查不到时 fallback 到项目名）
    if !sessions.is_empty() {
        let ids: Vec<String> = sessions.iter().map(|s| s.session_id.clone()).collect();
        let titles = fetch_task_titles(&ids);
        for s in &mut sessions {
            s.title = titles
                .get(&s.session_id)
                .cloned()
                .or_else(|| s.project.clone())
                .unwrap_or_default();
        }
    }

    // 桌宠开关状态（按会话持久化在 config.json）
    let disabled = settings::load().disabled_sessions;
    for s in &mut sessions {
        s.pet_enabled = !disabled.iter().any(|d| d == &s.session_id);
    }

    sessions
}

/// 打开/关闭某条会话的桌宠（按会话持久化）。
#[tauri::command]
pub fn set_pet_enabled(app: AppHandle, session_id: String, enabled: bool) -> Result<(), String> {
    let mut cfg = settings::load();
    if enabled {
        cfg.disabled_sessions.retain(|s| s != &session_id);
    } else if !cfg.disabled_sessions.iter().any(|s| s == &session_id) {
        cfg.disabled_sessions.push(session_id.clone());
    }
    settings::save(&cfg)?;

    if enabled {
        // 重新打开：若该会话状态文件仍在（执行中）则重建宠物窗口
        let state_file = pet::state_dir().join(format!("{session_id}.json"));
        if let Ok(content) = std::fs::read_to_string(&state_file) {
            if let Ok(sf) = serde_json::from_str::<pet::StateFile>(&content) {
                crate::window::ensure_window(&app, &sf.session_id, &sf.state);
            }
        }
    } else {
        crate::window::close_window(&app, &session_id);
    }
    Ok(())
}

/// 关闭一条会话：关窗 + 删状态文件 + 移出列表。
/// 若该会话在 ZCode 中仍在执行，后续事件会重新出现。
#[tauri::command]
pub fn close_session(app: AppHandle, session_id: String) {
    crate::window::close_session(&app, &session_id);
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
