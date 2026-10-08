use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder,
};

use crate::capture::Session;

pub fn show(app: &AppHandle) {
    let session = app.state::<Session>();
    let Some(region) = session.region() else { return };

    let x = region.mon_x + region.x as i32;
    let y = region.mon_y + region.y as i32;
    let w = region.w.max(120);
    let h = region.h.max(48);

    let window = match app.get_webview_window("overlay") {
        Some(w) => w,
        None => {
            let built = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .focused(false)
                .shadow(false)
                .visible(false)
                .build();
            match built {
                Ok(w) => {
                    let _ = w.set_ignore_cursor_events(true);
                    let _ = w.set_content_protected(true);
                    w
                }
                Err(_) => return,
            }
        }
    };

    let _ = window.set_size(PhysicalSize::new(w, h));
    let _ = window.set_position(PhysicalPosition::new(x, y));
    let _ = window.show();
}

#[tauri::command]
pub async fn get_last_translation(session: State<'_, Session>) -> Result<String, String> {
    Ok(session.last_translation.lock().unwrap().clone())
}

#[tauri::command]
pub async fn hide_overlay(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        w.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}
