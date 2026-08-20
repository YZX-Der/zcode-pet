//! 权限请求详情：hook 在 PermissionRequest 时写入，桌宠确认弹窗读取。

use crate::pet;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 待确认请求详情目录。
fn requests_dir() -> PathBuf {
    pet::home().join(".zcode-pet").join("requests")
}

/// 待确认请求详情（hook 在 PermissionRequest 时写入）。
#[derive(Serialize, Deserialize)]
pub struct PendingRequest {
    pub request_id: String,
    pub tool: String,
    pub input_summary: String,
    pub risk: String,
    pub reason: String,
    pub ts: i64,
}

/// 读取最新的待确认请求详情（按 ts 排序，取最新一条）。
#[tauri::command]
pub fn get_pending_request() -> Option<PendingRequest> {
    let dir = requests_dir();
    let mut latest: Option<(i64, PendingRequest)> = None;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "json") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else { continue };
            let Ok(req) = serde_json::from_str::<PendingRequest>(&content) else { continue };
            if latest.as_ref().map_or(true, |(ts, _)| req.ts > *ts) {
                latest = Some((req.ts, req));
            }
        }
    }
    latest.map(|(_, req)| req)
}

/// 清理已处理的请求详情文件（用户确认后删除）。
#[tauri::command]
pub fn clear_pending_request(request_id: String) {
    let dir = requests_dir();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "json") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(req) = serde_json::from_str::<PendingRequest>(&content) {
                    if req.request_id == request_id {
                        let _ = std::fs::remove_file(&path);
                        return;
                    }
                }
            }
        }
    }
}
