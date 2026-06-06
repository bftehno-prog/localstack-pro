use std::sync::OnceLock;
use tauri::{
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalPosition, Manager, Position, WebviewUrl, WebviewWindowBuilder,
};

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

const TRAY_PANEL_WIDTH: f64 = 360.0;
const TRAY_PANEL_HEIGHT: f64 = 430.0;
const TRAY_PANEL_MARGIN: f64 = 10.0;

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let _ = APP_HANDLE.set(app.clone());
    create_tray_panel(app)?;
    let icon = app.default_window_icon().cloned();
    let mut builder = TrayIconBuilder::new()
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button,
                button_state: _,
                ..
            } = event
            {
                if matches!(button, MouseButton::Left | MouseButton::Right) {
                    show_tray_panel(&tray.app_handle());
                }
            }
        });
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

pub fn rebuild_menu_for_settings_change() {
    if let Some(app) = APP_HANDLE.get() {
        if let Some(window) = app.get_webview_window("tray-panel") {
            let _ = window.emit("tray-panel-opened", ());
        }
    }
}

fn create_tray_panel(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("tray-panel").is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "tray-panel", WebviewUrl::App("index.html#tray".into()))
        .title("LocalStack Pro")
        .inner_size(TRAY_PANEL_WIDTH, TRAY_PANEL_HEIGHT)
        .min_inner_size(TRAY_PANEL_WIDTH, TRAY_PANEL_HEIGHT)
        .max_inner_size(TRAY_PANEL_WIDTH, TRAY_PANEL_HEIGHT)
        .visible(false)
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .shadow(true)
        .transparent(true)
        .build()?;
    Ok(())
}

fn show_tray_panel(app: &AppHandle) {
    let Some(window) = app.get_webview_window("tray-panel") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        position_tray_panel(app, &window);
        let _ = window.set_focus();
        let _ = window.emit("tray-panel-opened", ());
        return;
    }
    position_tray_panel(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
    let _ = window.emit("tray-panel-opened", ());
}

fn position_tray_panel(app: &AppHandle, window: &tauri::WebviewWindow) {
    let monitor = app.primary_monitor().ok().flatten().or_else(|| {
        app.available_monitors()
            .ok()
            .and_then(|items| items.into_iter().next())
    });
    if let Some(monitor) = monitor {
        let area = monitor.work_area();
        let scale = monitor.scale_factor();
        let width = TRAY_PANEL_WIDTH * scale;
        let height = TRAY_PANEL_HEIGHT * scale;
        let margin = TRAY_PANEL_MARGIN * scale;
        let x = area.position.x as f64 + area.size.width as f64 - width - margin;
        let y = area.position.y as f64 + area.size.height as f64 - height - margin;
        let logical = LogicalPosition::new(x / scale, y / scale);
        let _ = window.set_position(Position::Logical(logical));
    } else {
        let _ = window.center();
    }
}

pub fn hide_tray_panel(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("tray-panel") {
        let _ = window.hide();
    }
}

pub fn open_main_page(app: &AppHandle, page: Option<String>) {
    hide_tray_panel(app);
    show_main(app);
    if let Some(page) = page {
        let _ = app.emit("navigate", page);
    }
}

fn show_main(app: &AppHandle) {
    hide_tray_panel(app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
