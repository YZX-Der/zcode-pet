//! 状态目录监听器 + 衰减定时器。
//!
//! 监听 ~/.zcode-pet/state/ 变化 -> 读文件 -> 创建/更新窗口；
//! 监听 ~/.zcode-pet/requests/（权限请求详情）-> 状态写入被 debounce 合并时兜底触发 needs_input；
//! 监听 ~/.zcode/cli/rollout/（会话模型 IO 痕迹）-> 检测当前会话切换，立即刷新桌宠状态；
//! 每 60s 扫描一次应用衰减规则并回收失联会话。

use crate::pet;
use crate::window;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// 上一次识别的当前会话（rollout watcher 用它检测会话切换）。
static LAST_CURRENT: Mutex<Option<String>> = Mutex::new(None);
/// 已处理过的最新请求文件名（避免历史残留请求重复触发）。
static LAST_REQUEST: Mutex<Option<String>> = Mutex::new(None);

/// 给定 session_id 是否为当前活跃会话。
/// rollout 数据缺失（ZCode 未运行/无会话）时回退为 true，保持旧行为。
fn is_current_session(session_id: &str) -> bool {
    match pet::current_session_id() {
        Some(cur) => cur == session_id,
        None => true,
    }
}

/// 统一处理当前会话的状态变更：更新桌宠窗口 + 弹窗开关 + 托盘菜单。
fn apply_current_state(app: &AppHandle, session_id: &str, state: &str) {
    window::ensure_window(app, session_id, state, true);
    if state == "needs_input" {
        // 用户已在 ZCode 窗口时只保留气泡提示，不弹确认窗（避免遮挡权限框）
        if !crate::activate::is_zcode_frontmost() {
            window::show_request_window(app);
        }
    } else {
        window::close_request_window(app);
    }
    crate::refresh_tray_menu(app);
}

/// 处理单个状态文件变化。
fn handle_state_file(app: &AppHandle, path: &Path) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(state) = serde_json::from_str::<pet::StateFile>(&content) else {
        return;
    };

    // 更新 managed state（所有会话都记录，供衰减/切换逻辑使用）
    {
        let app_state = app.state::<crate::AppState>();
        let mut sessions = app_state.sessions.lock().unwrap();
        sessions.insert(
            state.session_id.clone(),
            crate::SessionEntry {
                raw_state: state.state.clone(),
                ts: state.ts,
            },
        );
    }

    // 只让当前会话的状态驱动桌宠：其他会话（后台任务）在跑时不干扰桌宠显示
    if !is_current_session(&state.session_id) {
        return;
    }
    apply_current_state(app, &state.session_id, &state.state);
}

/// rollout 目录变化（当前会话的任何模型 IO 都会触发）：
/// 当前会话切换时立即刷新桌宠，不再等下一个状态事件或 60s 衰减定时器。
fn handle_rollout_change(app: &AppHandle) {
    let Some(current) = pet::current_session_id() else {
        return;
    };
    {
        let mut last = LAST_CURRENT.lock().unwrap();
        if last.as_deref() == Some(current.as_str()) {
            return;
        }
        *last = Some(current.clone());
    }
    log::info!("current session switched to {current}");

    // 新会话状态：状态文件（可能已过期，但比 idle 准确）> idle。
    // 无状态文件的会话用 ts=0 而非当前时间：避免切换来回横跳时不断重置
    // 衰减时钟（否则空闲永远到不了变淡阈值，宠物一直不淡）。
    let (state, ts) = pet::read_state_file(&current)
        .map(|f| (f.state, f.ts))
        .unwrap_or_else(|| ("idle".to_string(), 0));
    {
        let app_state = app.state::<crate::AppState>();
        let mut sessions = app_state.sessions.lock().unwrap();
        sessions.insert(current.clone(), crate::SessionEntry {
            raw_state: state.clone(),
            ts,
        });
    }
    apply_current_state(app, &current, &state);
}

/// 新的权限请求详情文件出现：请求文件由 hook 在 PermissionRequest 分支写入，
/// 若用户批准太快，needs_input 状态写入可能被随后的 running 在 debounce 内合并掉，
/// 这里以请求文件为信号兜底触发 needs_input，保证权限请求不被漏掉。
fn handle_new_request(app: &AppHandle, path: &Path) {
    let Some(name) = path.file_name().and_then(|n| n.to_str()).map(String::from) else {
        return;
    };
    {
        let mut last = LAST_REQUEST.lock().unwrap();
        if last.as_deref() == Some(name.as_str()) {
            return;
        }
        *last = Some(name.clone());
    }
    log::info!("new permission request: {name}");
    let Some(current) = pet::current_session_id() else {
        return;
    };
    apply_current_state(app, &current, "needs_input");
}

/// 启动文件监听器（独立线程，debounce 200ms，监听状态/请求/rollout 三个目录）。
pub fn start_watcher(app: AppHandle) {
    let state_dir = pet::state_dir();
    std::fs::create_dir_all(&state_dir).ok();
    let requests_dir = pet::requests_dir();
    std::fs::create_dir_all(&requests_dir).ok();
    let rollout_dir = pet::home().join(".zcode").join("cli").join("rollout");

    // 启动时记住最新请求文件：历史残留请求不触发 needs_input
    if let Ok(entries) = std::fs::read_dir(&requests_dir) {
        let mut newest: Option<(std::time::SystemTime, String)> = None;
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            let Ok(mtime) = meta.modified() else { continue };
            let Some(name) = entry.file_name().to_str().map(String::from) else { continue };
            if newest.as_ref().map_or(true, |(t, _)| mtime > *t) {
                newest = Some((mtime, name));
            }
        }
        *LAST_REQUEST.lock().unwrap() = newest.map(|(_, n)| n);
    }

    let app_handle = app.clone();

    std::thread::spawn(move || {
        use notify::RecursiveMode;
        use notify_debouncer_full::new_debouncer;

        // 闭包按值捕获，watch 调用仍用原变量
        let sd = state_dir.clone();
        let rd = requests_dir.clone();
        let ro = rollout_dir.clone();

        let handler_app = app_handle.clone();
        let mut debouncer = match new_debouncer(
            Duration::from_millis(200),
            None,
            move |result: notify_debouncer_full::DebounceEventResult| {
                if let Ok(events) = result {
                    for event in events {
                        for path in &event.event.paths {
                            if path.starts_with(&sd) {
                                if path.extension().map_or(false, |e| e == "json") {
                                    handle_state_file(&handler_app, path);
                                }
                            } else if path.starts_with(&rd) {
                                if path.extension().map_or(false, |e| e == "json") {
                                    handle_new_request(&handler_app, path);
                                }
                            } else if path.starts_with(&ro) {
                                handle_rollout_change(&handler_app);
                            }
                        }
                    }
                }
            },
        ) {
            Ok(d) => d,
            Err(e) => {
                log::error!("watcher init failed: {e}");
                return;
            }
        };

        // 三个目录独立 watch，单个失败不影响其他
        if let Err(e) = debouncer.watch(&state_dir, RecursiveMode::NonRecursive) {
            log::error!("watch state dir failed: {e}");
        }
        if let Err(e) = debouncer.watch(&requests_dir, RecursiveMode::NonRecursive) {
            log::error!("watch requests dir failed: {e}");
        }
        // ZCode 尚未运行（rollout 目录不存在）时跳过，后续由状态事件驱动
        if rollout_dir.exists() {
            if let Err(e) = debouncer.watch(&rollout_dir, RecursiveMode::NonRecursive) {
                log::error!("watch rollout dir failed: {e}");
            }
        } else {
            log::info!("rollout dir missing, session-switch detection disabled");
        }

        log::info!("watching {} / {} / {}", state_dir.display(), requests_dir.display(), rollout_dir.display());

        // 阻塞线程以保持 debouncer 存活
        // notify_debouncer_full 内部线程持续运行，这里 sleep 保活
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    });
}

/// 启动衰减定时器：每次扫描后按「下一次状态切换时刻」自适应等待
/// （上限 60s、下限 5s），保证 ready→空闲→休眠等转换准时发射——
/// 固定 60s 粒度下，短于 60s 的状态窗口（如 30s 的空闲）可能整段错过。
pub fn start_decay_timer(app: AppHandle) {
    std::thread::spawn(move || loop {
        apply_decay(&app);
        let wait = next_decay_wait(&app);
        std::thread::sleep(Duration::from_secs(wait));
    });
}

/// 计算到下一次状态切换（衰减/回收）的等待秒数（夹在 5~60s）。
fn next_decay_wait(app: &AppHandle) -> u64 {
    let now = chrono::Utc::now().timestamp();
    let idle_fade = crate::settings::load().idle_fade_seconds as i64;
    let current_sid = pet::current_session_id();
    let delta = {
        let app_state = app.state::<crate::AppState>();
        let sessions = app_state.sessions.lock().unwrap();
        next_transition_delta(&sessions, now, idle_fade, current_sid.as_deref())
    };
    delta.clamp(5, 60) as u64
}

/// 纯函数：距离下一次状态切换的秒数（无未来切换时返回 60）。
/// 与 apply_decay 的判定共用同一套阈值（含 `>` 边界的 +1s）。
fn next_transition_delta(
    entries: &std::collections::HashMap<String, crate::SessionEntry>,
    now: i64,
    idle_fade_seconds: i64,
    current_sid: Option<&str>,
) -> i64 {
    let mut next: Option<i64> = None;
    for (session_id, entry) in entries {
        let ts = entry.ts;
        let mut moments: Vec<i64> = Vec::new();
        match entry.raw_state.as_str() {
            // R1: ready 持续 300s -> idle；空闲再持续 idle_fade -> sleep
            "ready" => {
                moments.push(ts + 301);
                if idle_fade_seconds > 0 {
                    moments.push(ts + 301 + idle_fade_seconds);
                }
            }
            // 空闲：持续 idle_fade -> sleep（0 = 关闭，无转换）
            "idle" => {
                if idle_fade_seconds > 0 {
                    moments.push(ts + idle_fade_seconds + 1);
                }
            }
            // 任务状态：持续 600s -> sleep
            _ => moments.push(ts + 601),
        }
        // R3: 非当前会话 1800s 回收
        if current_sid != Some(session_id.as_str()) {
            moments.push(ts + 1801);
        }
        for m in moments {
            let d = m - now;
            if d > 0 {
                next = Some(next.map_or(d, |n| n.min(d)));
            }
        }
    }
    next.unwrap_or(60)
}

/// 应用衰减规则：更新窗口状态、回收失联会话。
fn apply_decay(app: &AppHandle) {
    let now = chrono::Utc::now().timestamp();
    // 当前活跃会话豁免回收：即使长时间无事件也保持（变淡显示），不消失
    let current_sid = pet::current_session_id();
    let mut dead_sessions = Vec::new();

    {
        let app_state = app.state::<crate::AppState>();
        let mut sessions = app_state.sessions.lock().unwrap();

        // 衰减后当前会话的有效状态（单一桌宠只更新这一个）
        let mut current_effective: Option<String> = None;

        for (session_id, entry) in sessions.iter_mut() {
            let effective = window::effective_state(&entry.raw_state, entry.ts);

            // R3: 1800s 无事件 -> 会话死亡（当前会话豁免，保持淡显示不回收）
            if now - entry.ts > 1800 && current_sid.as_deref() != Some(session_id.as_str()) {
                dead_sessions.push(session_id.clone());
                continue;
            }

            // 只更新当前会话的桌宠状态
            if current_sid.as_deref() == Some(session_id.as_str()) {
                current_effective = Some(effective);
            }
        }

        // 更新桌宠窗口状态（单一桌宠反映当前会话）
        if let Some(state) = current_effective {
            if let Some(window) = app.get_webview_window("pet") {
                let _ = window.emit(
                    "pet://state",
                    window::StatePayload { state },
                );
            }
        }

        for sid in &dead_sessions {
            sessions.remove(sid);
        }
    }

    // 清理死亡会话的状态文件（单一桌宠模式下不需要关窗，桌宠只跟随当前会话）
    for sid in &dead_sessions {
        let file = pet::state_dir().join(format!("{sid}.json"));
        let _ = std::fs::remove_file(&file);
        log::info!("reaped dead session {sid}");
    }

    // 刷新托盘菜单（Token 用量等可能在无 hook 事件时变化，定时兜底）
    crate::refresh_tray_menu(app);
}

#[cfg(test)]
mod tests {
    use super::next_transition_delta;
    use crate::SessionEntry;
    use std::collections::HashMap;

    const NOW: i64 = 10_000;

    fn entry(state: &str, ts: i64) -> SessionEntry {
        SessionEntry {
            raw_state: state.to_string(),
            ts,
        }
    }

    #[test]
    fn test_ready_next_transition_at_300s() {
        let mut m = HashMap::new();
        m.insert("s1".into(), entry("ready", NOW - 100));
        // ready -> idle 在 ts+301（即 NOW+201）；空闲再 idle_fade 后 sleep
        assert_eq!(next_transition_delta(&m, NOW, 600, None), 201);
        assert_eq!(next_transition_delta(&m, NOW, 30, None), 201);
    }

    #[test]
    fn test_idle_uses_configured_delay() {
        let mut m = HashMap::new();
        m.insert("s1".into(), entry("idle", NOW - 10));
        // idle -> sleep 在 ts + 30 + 1 = NOW + 21
        assert_eq!(next_transition_delta(&m, NOW, 30, Some("s1")), 21);
        // 关闭（0）：空闲无衰减转换，当前会话豁免回收 -> 默认 60
        assert_eq!(next_transition_delta(&m, NOW, 0, Some("s1")), 60);
    }

    #[test]
    fn test_task_state_600s() {
        let mut m = HashMap::new();
        m.insert("s1".into(), entry("running", NOW - 100));
        assert_eq!(next_transition_delta(&m, NOW, 600, None), 501);
    }

    #[test]
    fn test_non_current_reclaim() {
        let mut m = HashMap::new();
        m.insert("s2".into(), entry("idle", NOW - 1790));
        // 非当前会话：idle 睡眠时刻已过，回收在 ts+1801 = NOW+11
        assert_eq!(next_transition_delta(&m, NOW, 600, Some("s1")), 11);
        // 当前会话豁免回收：无未来转换 -> 60
        assert_eq!(next_transition_delta(&m, NOW, 600, Some("s2")), 60);
    }

    #[test]
    fn test_empty_map_defaults_60() {
        assert_eq!(next_transition_delta(&HashMap::new(), NOW, 600, None), 60);
    }

    #[test]
    fn test_all_transitions_past_defaults_60() {
        let mut m = HashMap::new();
        m.insert("s1".into(), entry("running", NOW - 1000));
        // 当前会话、任务状态 600s 已过 -> 60
        assert_eq!(next_transition_delta(&m, NOW, 600, Some("s1")), 60);
    }
}
