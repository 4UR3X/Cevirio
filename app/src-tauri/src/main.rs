#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;

use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({
        "name": "Çevirio",
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    })
}

fn main() {
    let capture_key = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyX);

    tauri::Builder::default()
        .manage(capture::Session::default())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &capture_key && event.state() == ShortcutState::Pressed {
                        let app = app.clone();
                        std::thread::spawn(move || {
                            let _ = capture::begin(&app);
                        });
                    }
                })
                .build(),
        )
        .setup(move |app| {
            app.global_shortcut().register(capture_key)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            capture::start_capture,
            capture::get_frozen,
            capture::finish_selection,
            capture::cancel_selection
        ])
        .run(tauri::generate_context!())
        .expect("Çevirio başlatılamadı");
}
