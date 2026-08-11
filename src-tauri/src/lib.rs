//! zcode-pet 应用入口 -- Tauri 2 桌面宠物应用。

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

mod activate;
mod dashboard;
mod pet;
mod settings;
mod watcher;
mod window;

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, tray::TrayIconBuilder, WindowEvent};

/// 全局共享状态
pub struct AppState {
    pub sessions: Mutex<HashMap<String, SessionEntry>>,
    pub pet_name: Mutex<String>,
}

pub struct SessionEntry {
    pub raw_state: String,
    pub ts: i64,
}

/// 构建托盘菜单
fn build_tray_menu(app: &tauri::AppHandle) -> Menu<tauri::Wry> {
    let show = MenuItem::with_id(app, "show-main", "显示主窗口", true, None::<&str>)
        .expect("show item");
    let wake = MenuItem::with_id(app, "wake", "唤醒全部宠物", true, None::<&str>)
        .expect("wake item");
    let tuck = MenuItem::with_id(app, "tuck", "收起全部宠物", true, None::<&str>)
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

    let _ = menu.append(&show);
    let _ = menu.append(&sep.clone());
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
                    window::show_main_window(tray.app_handle());
                }
            }
        })
        .build(app);
}

/// Tauri 命令：前端加载完成后调用，激活并显示主窗口。
/// （必须放在子模块中，Tauri 2 的 command 宏在 crate root 会冲突）
mod cmd {
    use crate::window;
    use tauri::AppHandle;

    #[tauri::command]
    pub fn frontend_ready(app: AppHandle) {
        window::show_main_window(&app);
    }
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
                "show-main" => window::show_main_window(app),
                "wake" => window::show_all(app),
                "tuck" => window::hide_all(app),
                other if other.starts_with("pet-select-") => {
                    let name = &other["pet-select-".len()..];
                    let mut settings = settings::load();
                    settings.pet = name.to_string();
                    let _ = settings::save(&settings);
                    let state = app.state::<AppState>();
                    *state.pet_name.lock().unwrap() = name.to_string();
                    window::recreate_all(app);
                    log::info!("switched pet to {name}");
                }
                _ => {}
            }
        })
        .on_window_event(|window, event| {
            // 关闭主窗口时隐藏到托盘，不退出进程
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            // 确保应用作为常规应用激活（Dock 可见，而非后台辅助）
            #[cfg(target_os = "macos")]
            {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
            }

            // 手动创建主窗口
            use tauri::{WebviewUrl, WebviewWindowBuilder};
            let main_window = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::App("index.html".into()),
            )
            .title("zcode-pet")
            .inner_size(960.0, 640.0)
            .min_inner_size(800.0, 520.0)
            .center()
            .visible(true)
            // macOS: 标题栏 Overlay 模式——webview 延伸到标题栏区域，
            // 原生交通灯按钮浮在透明玻璃背景上（液态玻璃效果）
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true);

            match main_window.build() {
                Ok(_) => log::info!("main window created"),
                Err(e) => log::error!("failed to create main window: {e}"),
            }

            // 扫描现有状态文件，为每个活跃会话创建宠物窗口
            window::scan_and_create(app.handle());

            // 启动状态目录监听器
            watcher::start_watcher(app.handle().clone());

            // 启动衰减定时器
            watcher::start_decay_timer(app.handle().clone());

            // 设置托盘菜单
            setup_tray(app.handle());

            // 强制激活主窗口到最前面
            window::show_main_window(app.handle());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            activate::activate_target,
            settings::get_settings,
            settings::save_settings,
            settings::list_pets,
            dashboard::list_sessions,
            dashboard::set_pet_visible,
            dashboard::get_pet_sheet,
            cmd::frontend_ready
        ])
        .run(tauri::generate_context!())
        .expect("error while running zcode-pet");
}
