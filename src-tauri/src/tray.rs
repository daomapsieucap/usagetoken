use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let last_hidden: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let last_hidden_blur = last_hidden.clone();

    // Hide popup on blur; record timestamp so the tray-click handler can ignore the
    // click that caused the blur (prevents immediate re-open on dismiss).
    if let Some(popup) = app.get_webview_window("popup") {
        let app_h = app.clone();
        popup.on_window_event(move |event| {
            if let tauri::WindowEvent::Focused(false) = event {
                if let Some(w) = app_h.get_webview_window("popup") {
                    let _ = w.hide();
                }
                *last_hidden_blur.lock().unwrap() = Some(Instant::now());
            }
        });
    }

    // Right-click context menu
    let open_item   = MenuItem::with_id(app, "open",   "Open",          true, None::<&str>)?;
    let widget_item = MenuItem::with_id(app, "widget", "Toggle widget", true, None::<&str>)?;
    let sep         = PredefinedMenuItem::separator(app)?;
    let quit_item   = MenuItem::with_id(app, "quit",   "Quit",          true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &widget_item, &sep, &quit_item])?;

    // Load tray icon from the bundle (configured in tauri.conf.json → bundle.icon).
    // app.default_window_icon().clone() copies the pixel data into an owned Image<'static>.
    let icon = app
        .default_window_icon()
        .cloned()
        .unwrap_or_else(|| Image::new_owned(vec![0, 0, 0, 0], 1, 1));

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("UsageToken")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event({
            let last_hidden = last_hidden.clone();
            move |tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    rect,
                    ..
                } = event
                {
                    let app = tray.app_handle();
                    let Some(popup) = app.get_webview_window("popup") else { return };

                    // Suppress re-open if this click was what caused the blur-hide.
                    let just_hidden = last_hidden
                        .lock()
                        .unwrap()
                        .map(|t| t.elapsed() < Duration::from_millis(300))
                        .unwrap_or(false);
                    if just_hidden {
                        return;
                    }

                    if popup.is_visible().unwrap_or(false) {
                        let _ = popup.hide();
                    } else {
                        // Position the popup above the tray icon.
                        // rect.position / rect.size are tauri::Position / tauri::Size enums.
                        let win_size = popup
                            .outer_size()
                            .unwrap_or_else(|_| tauri::PhysicalSize::new(480u32, 640u32));

                        let (icon_x, icon_y) = match rect.position {
                            tauri::Position::Physical(p) => (p.x as f64, p.y as f64),
                            tauri::Position::Logical(l)  => (l.x,        l.y),
                        };
                        let (icon_w, _icon_h) = match rect.size {
                            tauri::Size::Physical(s) => (s.width as f64, s.height as f64),
                            tauri::Size::Logical(l)  => (l.width,        l.height),
                        };

                        let cx = icon_x + icon_w / 2.0;
                        let win_w = win_size.width as i32;
                        let win_h = win_size.height as i32;

                        let mut x = (cx - win_w as f64 / 2.0) as i32;
                        let y = (icon_y - win_h as f64 - 4.0).max(0.0) as i32;

                        // Clamp x to the monitor containing the tray icon so the popup
                        // doesn't overflow the right (or left) screen edge.
                        let monitors = app.available_monitors().unwrap_or_default();
                        if let Some(monitor) = monitors.iter().find(|m| {
                            let mx = m.position().x as f64;
                            let mw = m.size().width as f64;
                            icon_x >= mx && icon_x < mx + mw
                        }) {
                            let mon_left  = monitor.position().x;
                            let mon_right = mon_left + monitor.size().width as i32;
                            x = x.clamp(mon_left, mon_right - win_w);
                        } else {
                            x = x.max(0);
                        }

                        let _ = popup.set_position(tauri::PhysicalPosition::new(x, y));
                        let _ = popup.show();
                        let _ = popup.set_focus();
                    }
                }
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => {
                if let Some(popup) = app.get_webview_window("popup") {
                    let _ = popup.show();
                    let _ = popup.set_focus();
                }
            }
            "widget" => {
                let state = app.state::<Mutex<crate::data::Settings>>();
                let mut settings = state.lock().unwrap();
                settings.show_widget = !settings.show_widget;
                let show = settings.show_widget;
                drop(settings);
                if let Some(w) = app.get_webview_window("widget") {
                    if show { let _ = w.show(); } else { let _ = w.hide(); }
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}
