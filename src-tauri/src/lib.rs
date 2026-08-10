//! zcode-pet 应用入口 -- Tauri 2 透明悬浮桌宠。

mod activate;
mod pet;
mod watcher;
mod window;

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Manager, tray::TrayIconBuilder};

/// 全局共享状态
pub struct AppState {
    /// session_id -> 会话条目
    pub sessions: Mutex<HashMap<String, SessionEntry>>,
    /// 当前选中的宠物名
    pub pet_name: Mutex<String>,
}

pub struct SessionEntry {
    pub raw_state: String,
    pub ts: i64,
}

/// 构建托盘菜单
fn build_tray_menu(app: &tauri::AppHandle) -> Menu<tauri::Wry> {
    let wake = MenuItem::with_id(app, "wake", "唤醒全部", true, None::<&str>)
        .expect("wake item");
    let tuck = MenuItem::with_id(app, "tuck", "收起全部", true, None::<&str>)
        .expect("tuck item");
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)
        .expect("quit item");
    let sep = PredefinedMenuItem::separator(app).expect("separator");

    let menu = Menu::new(app).expect("menu");

    // 宠物切换子菜单
    let pet_names = pet::list_pets();
    if !pet_names.is_empty() {
        let pet_items: Vec<MenuItem<tauri::Wry>> = pet_names
            .iter()
            .filter_map(|name| {
                MenuItem::with_id(
                    app,
                    format!("pet-select-{name}"),
                    name,
                    true,
                    None::<&str>,
                )
                .ok()
            })
            .collect();

        if let Ok(sub) = Submenu::new(app, "切换宠物", true) {
            for item in &pet_items {
                let _ = sub.append(item);
            }
            let _ = menu.append(&sub);
            let _ = menu.append(&sep.clone());
        }
    }

    let _ = menu.append(&wake);
    let _ = menu.append(&tuck);
    let _ = menu.append(&PredefinedMenuItem::separator(app).expect("separator"));
    let _ = menu.append(&quit);
    menu
}

/// 设置托盘图标
fn setup_tray(app: &tauri::AppHandle) {
    let menu = build_tray_menu(app);

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("default window icon required");

    let _ = TrayIconBuilder::with_id("main")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click { button, .. } = event {
                if button == tauri::tray::MouseButton::Left {
                    window::show_all(tray.app_handle());
                }
            }
        })
        .build(app);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .manage(AppState {
            sessions: Mutex::new(HashMap::new()),
            pet_name: Mutex::new("zbuddy".to_string()),
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "quit" => app.exit(0),
                "wake" => window::show_all(app),
                "tuck" => window::hide_all(app),
                other if other.starts_with("pet-select-") => {
                    let name = &other["pet-select-".len()..];
                    let state = app.state::<AppState>();
                    *state.pet_name.lock().unwrap() = name.to_string();
                    log::info!("switched pet to {name}");
                    // 通过重新注入 __PET_INIT__ 并 reload 让窗口加载新宠物
                    if let Some(sheet) = pet::sheet_path(name) {
                        for label in window::all_pet_labels(app) {
                            if let Some(win) = app.get_webview_window(&label) {
                                let init_js = format!(
                                    r#"window.__PET_INIT__ = {{ pet: "{name}", state: "idle", sheet: "{}" }};"#,
                                    sheet.display()
                                );
                                let _ = win.eval(&init_js);
                                let _ = win.eval("location.reload();");
                            }
                        }
                    }
                }
                _ => {}
            }
        })
        .setup(|app| {
            // 确保应用作为常规应用激活（非后台/辅助模式）
            app.set_activation_policy(tauri::ActivationPolicy::Regular);

            // 扫描现有状态文件，为每个活跃会话创建窗口
            window::scan_and_create(app.handle());

            // 启动状态目录监听器
            watcher::start_watcher(app.handle().clone());

            // 启动衰减定时器
            watcher::start_decay_timer(app.handle().clone());

            // 设置托盘菜单
            setup_tray(app.handle());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![activate::activate_target])
        .run(tauri::generate_context!())
        .expect("error while running zcode-pet");
}
