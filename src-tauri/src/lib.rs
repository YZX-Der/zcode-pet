//! zcode-pet 应用入口 -- Tauri 2 桌面宠物应用。

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

mod activate;
mod dashboard;
mod installer;
mod permission;
mod pet;
mod settings;
mod watcher;
mod window;

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, tray::TrayIconBuilder, WindowEvent};

/// 全局共享状态
pub struct AppState {
    pub sessions: Mutex<HashMap<String, SessionEntry>>,
    pub pet_name: Mutex<String>,
}

pub struct SessionEntry {
    pub raw_state: String,
    pub ts: i64,
}

/// 构建桌宠右键菜单：
/// 切换宠物 / 显示宠物 / 隐藏宠物 / 显示主窗口 / 退出
fn build_pet_menu(app: &tauri::AppHandle) -> Menu<tauri::Wry> {
    let show = MenuItem::with_id(app, "show-main", "显示主窗口", true, None::<&str>)
        .expect("show item");
    let pet_show = MenuItem::with_id(app, "pet-show", "显示宠物", true, None::<&str>)
        .expect("pet-show item");
    let pet_hide = MenuItem::with_id(app, "pet-hide", "隐藏宠物", true, None::<&str>)
        .expect("pet-hide item");
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

    let _ = menu.append(&pet_show);
    let _ = menu.append(&pet_hide);
    let _ = menu.append(&sep.clone());
    let _ = menu.append(&show);
    let _ = menu.append(&sep.clone());
    let _ = menu.append(&quit);
    menu
}

/// 构建托盘菜单：当前会话信息区 + 切换宠物 / 显示隐藏宠物 / 显示主窗口 / 退出
fn build_tray_menu(app: &tauri::AppHandle) -> Menu<tauri::Wry> {
    let show = MenuItem::with_id(app, "show-main", "显示主窗口", true, None::<&str>)
        .expect("show item");
    let pet_show = MenuItem::with_id(app, "pet-show", "显示宠物", true, None::<&str>)
        .expect("pet-show item");
    let pet_hide = MenuItem::with_id(app, "pet-hide", "隐藏宠物", true, None::<&str>)
        .expect("pet-hide item");
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)
        .expect("quit item");
    let sep = PredefinedMenuItem::separator(app).expect("separator");

    let menu = Menu::new(app).expect("menu");

    // 当前会话信息区（实时读取 rollout；用 enabled 项保证黑色文字可读，
    // info-* id 在 on_menu_event 中默认忽略，点击无操作）
    if let Some(detail) = dashboard::session_detail() {
        let info_items = [
            "🦊 当前会话",
            &format!("模型: {}", detail.model),
            &format!("思考等级: {}", detail.thinking),
            &format!("上下文: {}", detail.context),
            &format!("Token: {} / {}", detail.token_total, detail.context),
            &format!("缓存命中: {}", detail.cache_rate),
            &format!("思考: {}", detail.reasoning),
        ];
        for (i, text) in info_items.iter().enumerate() {
            if let Ok(item) = MenuItem::with_id(app, format!("info-{i}"), *text, true, None::<&str>) {
                let _ = menu.append(&item);
            }
        }
        let _ = menu.append(&sep.clone());
    }

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

    let _ = menu.append(&pet_show);
    let _ = menu.append(&pet_hide);
    let _ = menu.append(&sep.clone());
    let _ = menu.append(&show);
    let _ = menu.append(&sep.clone());
    let _ = menu.append(&quit);
    menu
}

/// Tauri 命令：在桌宠窗口的鼠标位置弹出右键菜单（原生 NSMenu）。
/// 必须放在子模块中（Tauri 2 的 command 宏在 crate root 会冲突）。
mod cmd {
    use crate::window;
    use tauri::AppHandle;
    use tauri::Manager;
    use tauri::menu::ContextMenu;

    #[tauri::command]
    pub fn frontend_ready(app: AppHandle) {
        window::show_main_window(&app);
    }

    /// 桌宠右键菜单：切换宠物 / 显示主窗口 / 退出。
    /// 菜单位置移到窗口右侧（避开宠物窗口遮挡，不改变窗口 level）。
    #[tauri::command]
    pub fn show_pet_menu(app: AppHandle) -> Result<(), String> {
        use tauri::PhysicalPosition;
        let menu = crate::build_pet_menu(&app);
        let Some(webview_window) = app.get_webview_window("pet") else {
            return Ok(());
        };
        let win = webview_window.as_ref().window();

        // 菜单显示在宠物像素右侧（canvas 宽 + 4px），贴近宠物且不被遮挡。
        // 遮挡只来自宠物像素本体，气泡区是透明的，菜单显示在其上方不受影响。
        let (menu_x, menu_y) = match (win.outer_size(), win.outer_position()) {
            (Ok(_size), Ok(pos)) => {
                // canvas 物理宽 = 精灵帧宽 × 用户缩放 × 屏幕缩放
                let sf = win
                    .current_monitor()
                    .ok()
                    .flatten()
                    .map(|m| m.scale_factor())
                    .unwrap_or(1.0);
                let pet_scale = crate::settings::load().scale;
                let canvas_w = (192.0 * pet_scale * sf) as i32;
                // 菜单 y 跟随鼠标（相对窗口），限制在窗口高度内
                let cursor = win
                    .cursor_position()
                    .unwrap_or(tauri::PhysicalPosition::new(pos.x as f64, pos.y as f64));
                let rel_y = (cursor.y - pos.y as f64).clamp(0.0, 208.0 * pet_scale * sf - 20.0) as i32;
                (canvas_w + 4, rel_y)
            }
            _ => (0, 0),
        };

        menu.popup_at(win, PhysicalPosition::new(menu_x, menu_y))
            .map_err(|e| e.to_string())
    }
}

/// 刷新托盘菜单（后台调用，不阻塞点击弹出）。
/// 在会话状态变化/定时衰减时更新，保证打开菜单时内容是最新的。
pub fn refresh_tray_menu(app: &tauri::AppHandle) {
    let menu = build_tray_menu(app);
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_menu(Some(menu));
    }
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
                "pet-show" => {
                    let mut s = settings::load();
                    s.pet_hidden = false;
                    let _ = settings::save(&s);
                    window::set_pet_visible(app, true);
                }
                "pet-hide" => {
                    let mut s = settings::load();
                    s.pet_hidden = true;
                    let _ = settings::save(&s);
                    window::set_pet_visible(app, false);
                }
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
            // 桌宠窗口聚焦时通知前端显示气泡：
            // macOS 聚焦前不派发 hover 事件，聚焦后鼠标已在宠物上
            // （无"移入"动作，mouseenter 不触发），需要主动提示一次
            if window.label() == "pet" {
                if let WindowEvent::Focused(true) = event {
                    let _ = window.emit("pet://focused", ());
                }
            }
        })
        .setup(|app| {
            // 确保应用作为常规应用激活（Dock 可见，而非后台辅助）
            #[cfg(target_os = "macos")]
            {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
            }

            // 初始化打包资源目录（内置宠物精灵表）
            if let Ok(res_dir) = app.path().resource_dir() {
                crate::pet::init_resource_dir(res_dir);
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
            installer::is_hooks_installed,
            installer::install_hooks,
            permission::get_pending_request,
            permission::clear_pending_request,
            cmd::show_pet_menu,
            cmd::frontend_ready
        ])
        .run(tauri::generate_context!())
        .expect("error while running zcode-pet");
}
