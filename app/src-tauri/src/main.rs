#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod live;
mod ocr;
mod overlay;
mod translate;

use tauri::Manager;
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
    let mods = Some(Modifiers::CONTROL | Modifiers::SHIFT);
    let key_capture = Shortcut::new(mods, Code::KeyX);
    let key_live = Shortcut::new(mods, Code::KeyL);
    let key_hide = Shortcut::new(mods, Code::KeyH);

    tauri::Builder::default()
        .manage(capture::Session::default())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let app = app.clone();
                    let shortcut = *shortcut;
                    std::thread::spawn(move || {
                        if shortcut == key_capture {
                            let _ = capture::begin(&app);
                        } else if shortcut == key_live {
                            let _ = live::toggle(&app);
                        } else if shortcut == key_hide {
                            if let Some(w) = app.get_webview_window("overlay") {
                                let _ = w.close();
                            }
                        }
                    });
                })
                .build(),
        )
        .setup(move |app| {
            let dir = app.path().app_data_dir()?;
            app.manage(translate::Translator::open(dir)?);
            let gs = app.global_shortcut();
            gs.register(key_capture)?;
            gs.register(key_live)?;
            gs.register(key_hide)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            capture::start_capture,
            capture::get_frozen,
            capture::finish_selection,
            capture::cancel_selection,
            capture::ocr_languages,
            capture::set_ocr_lang,
            translate::translate_text,
            translate::get_settings,
            translate::save_settings,
            translate::clear_cache,
            overlay::get_last_translation,
            overlay::hide_overlay,
            live::toggle_live
        ])
        .run(tauri::generate_context!())
        .expect("Çevirio başlatılamadı");
}
