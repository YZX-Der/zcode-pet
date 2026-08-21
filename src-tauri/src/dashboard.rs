//! Dashboard 面板的 Tauri 命令。

use crate::pet;
use crate::settings;
use crate::window::compute_effective_state;
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
    /// 是否为 ZCode 当前活跃会话（最近有模型 IO 的会话）
    pub is_current: bool,
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

/// 列出会话及其状态（供 Dashboard 会话页展示）。
///
/// 展示范围：非 sleep 的会话（idle/running/needs_input/ready/blocked）∪ 当前活跃会话
/// （当前会话即使 sleep 也显示，让用户能看到正在用的会话）。
#[tauri::command]
pub fn list_sessions() -> Vec<SessionInfo> {
    let state_dir = pet::state_dir();
    let now = chrono::Utc::now().timestamp();
    let current_sid = pet::current_session_id();
    let mut sessions = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&state_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "json") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else { continue };
            let Ok(sf) = serde_json::from_str::<pet::StateFile>(&content) else { continue };
            let effective = compute_effective_state(
                &sf.state,
                sf.ts,
                now,
                crate::settings::load().idle_fade_seconds as i64,
            );
            let is_current = current_sid.as_deref() == Some(&sf.session_id);
            // 非 sleep 的会话 或 当前活跃会话 才展示
            if effective == "sleep" && !is_current {
                continue;
            }
            sessions.push(SessionInfo {
                effective_state: effective,
                session_id: sf.session_id.clone(),
                state: sf.state,
                project: sf.project,
                title: String::new(),
                is_current,
                ts: sf.ts,
            });
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

    // 当前会话排第一
    sessions.sort_by(|a, b| b.is_current.cmp(&a.is_current));

    sessions
}

/// 桌宠全局开关（显示/隐藏）。
#[tauri::command]
pub fn set_pet_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    let mut cfg = settings::load();
    cfg.pet_hidden = !visible;
    settings::save(&cfg)?;
    crate::window::set_pet_visible(&app, visible);
    Ok(())
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

// ── 会话详情（托盘菜单信息区）──────────────────────────

/// 当前会话详情（模型/思考等级/上下文/Token/缓存），从 rollout 尾部读取。
#[derive(Serialize)]
pub struct SessionDetail {
    pub model: String,
    pub thinking: String,
    pub context: String,
    pub token_total: String,
    pub cache_rate: String,
    pub reasoning: String,
}

/// 格式化数字为 K 单位（>=1000 时保留 1 位小数）。
fn fmt_k(n: u64) -> String {
    if n >= 1000 {
        format!("{:.1}K", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

/// 读取当前会话 rollout 文件尾部，解析最后一条完整记录提取会话详情。
pub fn session_detail() -> Option<SessionDetail> {
    use std::io::{Read, Seek, SeekFrom};

    let sid = pet::current_session_id()?;
    let path = pet::home()
        .join(".zcode")
        .join("cli")
        .join("rollout")
        .join(format!("model-io-{sid}.jsonl"));
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    if size == 0 {
        return None;
    }
    // 只读尾部最多 1MB，避免全量解析大文件
    let read_len = size.min(1_000_000);
    file.seek(SeekFrom::Start(size - read_len)).ok()?;
    let mut buf = vec![0u8; read_len as usize];
    file.read_exact(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);

    // 从尾部往前找最后一条能完整解析的 JSON 行
    let mut record: Option<serde_json::Value> = None;
    for line in text.lines().rev() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
            record = Some(v);
            break;
        }
    }
    let rec = record?;

    let model = rec["model"]["modelId"].as_str()?.to_string();
    let thinking = rec["request"]["body"]["reasoning_effort"]
        .as_str()
        .unwrap_or("?")
        .to_string();
    let context = rec["request"]["maxOutputTokens"]
        .as_u64()
        .map(fmt_k)
        .unwrap_or_else(|| "?".into());
    let usage = &rec["response"]["usage"];
    let input = usage["inputTokens"].as_u64().unwrap_or(0);
    let total = usage["totalTokens"].as_u64().unwrap_or(0);
    let cache_read = usage["cacheReadTokens"].as_u64().unwrap_or(0);
    let reasoning = usage["reasoningTokens"].as_u64().unwrap_or(0);
    // 缓存命中率 = 缓存读取 / 总输入
    let cache_rate = if input > 0 {
        format!("{:.1}% ({})", cache_read as f64 / input as f64 * 100.0, fmt_k(cache_read))
    } else {
        "-".into()
    };

    Some(SessionDetail {
        model,
        thinking,
        context,
        token_total: fmt_k(total),
        cache_rate,
        reasoning: fmt_k(reasoning),
    })
}
