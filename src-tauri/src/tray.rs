use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::commands;
use crate::server::i18n;
use crate::state::{self, AppState};

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
    let Some(tray) = app.tray_by_id("main") else {
        return Ok(());
    };
    let menu = build_menu(app)?;
    tray.set_menu(Some(menu))?;
    let snapshot = state::snapshot(app);
    let tooltip = match snapshot.running.then(|| share_url(&snapshot.urls)) {
        Some(Some(url)) => format!("LanTransport · {url}"),
        _ => "LanTransport".to_string(),
    };
    tray.set_tooltip(Some(tooltip))?;
    Ok(())
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let locale = app.state::<AppState>().ui_locale();
    let snapshot = state::snapshot(app);
    let t = |key: &str| i18n::t(&locale, key);

    let show_item = MenuItem::with_id(app, "show", t("tray.open"), true, None::<&str>)?;
    let copy_item = MenuItem::with_id(app, "copy", t("tray.copy"), snapshot.running, None::<&str>)?;
    let toggle_item = MenuItem::with_id(
        app,
        "toggle",
        if snapshot.running {
            t("tray.stop")
        } else {
            t("tray.start")
        },
        true,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, "quit", t("tray.quit"), true, None::<&str>)?;
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
    if let Some(url) = share_url(&snapshot.urls) {
        let _ = app.clipboard().write_text(url);
    }
}

fn share_url(urls: &[String]) -> Option<String> {
    urls.iter()
        .find(|url| !url.contains("127.0.0.1"))
        .or_else(|| urls.first())
        .cloned()
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
