use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::commands;
use crate::server::Lang;
use crate::state::{self, AppState};

fn labels(lang: Lang) -> [&'static str; 5] {
    match lang {
        Lang::Zh => [
            "打开 LanTransport",
            "复制链接",
            "开始共享",
            "停止共享",
            "退出",
        ],
        Lang::En => [
            "Open LanTransport",
            "Copy link",
            "Start sharing",
            "Stop sharing",
            "Quit",
        ],
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    #[cfg(target_os = "macos")]
    let icon_bytes = include_bytes!("../icons/tray-template.png");
    #[cfg(not(target_os = "macos"))]
    let icon_bytes = include_bytes!("../icons/tray.png");
    let icon = tauri::image::Image::from_bytes(icon_bytes)?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("LanTransport")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn refresh(app: &AppHandle) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id("main") else { return Ok(()) };
    let menu = build_menu(app)?;
    tray.set_menu(Some(menu))?;
    let snapshot = state::snapshot(app);
    let tooltip = match snapshot.urls.first() {
        Some(url) if snapshot.running => format!("LanTransport · {url}"),
        _ => "LanTransport".to_string(),
    };
    tray.set_tooltip(Some(tooltip))?;
    Ok(())
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let lang = app.state::<AppState>().lang_value();
    let snapshot = state::snapshot(app);
    let [show, copy, start, stop, quit] = labels(lang);

    let show_item = MenuItem::with_id(app, "show", show, true, None::<&str>)?;
    let copy_item = MenuItem::with_id(app, "copy", copy, snapshot.running, None::<&str>)?;
    let toggle_item = MenuItem::with_id(
        app,
        "toggle",
        if snapshot.running { stop } else { start },
        true,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, "quit", quit, true, None::<&str>)?;
    let first_separator = PredefinedMenuItem::separator(app)?;
    let second_separator = PredefinedMenuItem::separator(app)?;

    Menu::with_items(
        app,
        &[
            &show_item,
            &copy_item,
            &first_separator,
            &toggle_item,
            &second_separator,
            &quit_item,
        ],
    )
}

fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        "show" => show_main(app),
        "copy" => copy_link(app),
        "toggle" => toggle(app),
        "quit" => quit(app),
        _ => {}
    }
}

fn copy_link(app: &AppHandle) {
    let snapshot = state::snapshot(app);
    if let Some(url) = snapshot.urls.first() {
        let _ = app.clipboard().write_text(url.clone());
    }
}

fn toggle(app: &AppHandle) {
    if state::snapshot(app).running {
        commands::stop_sharing(app.clone());
    } else {
        let _ = commands::start_from_settings(app);
        commands::notify(app);
    }
}

fn quit(app: &AppHandle) {
    commands::shutdown(app);
    app.exit(0);
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
