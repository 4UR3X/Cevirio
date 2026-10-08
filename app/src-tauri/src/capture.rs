use std::{
    io::Cursor,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use image::{
    codecs::png::{CompressionType, FilterType, PngEncoder},
    ImageEncoder, RgbaImage,
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

#[derive(Default)]
pub struct Session {
    frozen: Mutex<Option<RgbaImage>>,
    last: Mutex<Option<RgbaImage>>,
    hid_main: AtomicBool,
    lang: Mutex<Option<String>>,
}

fn to_data_url(img: &RgbaImage) -> Result<String, String> {
    let mut buf = Vec::with_capacity(1 << 20);
    PngEncoder::new_with_quality(Cursor::new(&mut buf), CompressionType::Fast, FilterType::NoFilter)
        .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(buf)))
}

fn restore_main(app: &AppHandle) {
    let session = app.state::<Session>();
    if session.hid_main.swap(false, Ordering::SeqCst) {
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.show();
        }
    }
}

pub fn begin(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("selector").is_some() {
        return Ok(());
    }
    let session = app.state::<Session>();

    if let Some(main) = app.get_webview_window("main") {
        if main.is_visible().unwrap_or(false) && !main.is_minimized().unwrap_or(false) {
            let _ = main.hide();
            session.hid_main.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(220));
        }
    }

    let result = (|| -> Result<(), String> {
        let cursor = app.cursor_position().map_err(|e| e.to_string())?;
        let monitor = xcap::Monitor::from_point(cursor.x as i32, cursor.y as i32)
            .map_err(|e| e.to_string())?;
        let img = monitor.capture_image().map_err(|e| e.to_string())?;
        let scale = monitor.scale_factor().map_err(|e| e.to_string())? as f64;
        let x = monitor.x().map_err(|e| e.to_string())? as f64 / scale;
        let y = monitor.y().map_err(|e| e.to_string())? as f64 / scale;

        *session.frozen.lock().unwrap() = Some(img);

        let window = WebviewWindowBuilder::new(app, "selector", WebviewUrl::App("selector.html".into()))
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .position(x, y)
            .inner_size(200.0, 200.0)
            .build()
            .map_err(|e| e.to_string())?;
        window.set_fullscreen(true).map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        Ok(())
    })();

    if result.is_err() {
        restore_main(app);
    }
    result
}

fn close_selector(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("selector") {
        let _ = w.close();
    }
    *app.state::<Session>().frozen.lock().unwrap() = None;
    restore_main(app);
}

#[tauri::command]
pub async fn start_capture(app: AppHandle) -> Result<(), String> {
    begin(&app)
}

#[tauri::command]
pub async fn get_frozen(session: State<'_, Session>) -> Result<String, String> {
    let guard = session.frozen.lock().unwrap();
    let img = guard.as_ref().ok_or("Görüntü yok")?;
    to_data_url(img)
}

#[tauri::command]
pub async fn finish_selection(
    app: AppHandle,
    session: State<'_, Session>,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Result<(), String> {
    let cropped = {
        let guard = session.frozen.lock().unwrap();
        let img = guard.as_ref().ok_or("Görüntü yok")?;
        let x = x.min(img.width().saturating_sub(1));
        let y = y.min(img.height().saturating_sub(1));
        let w = w.min(img.width() - x).max(1);
        let h = h.min(img.height() - y).max(1);
        image::imageops::crop_imm(img, x, y, w, h).to_image()
    };
    let url = to_data_url(&cropped)?;
    *session.last.lock().unwrap() = Some(cropped);
    close_selector(&app);
    app.emit("captured", url).map_err(|e| e.to_string())?;
    run_ocr(app);
    Ok(())
}

pub fn run_ocr(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let (img, lang) = {
            let session = app.state::<Session>();
            let img = session.last.lock().unwrap().clone();
            let lang = session.lang.lock().unwrap().clone();
            (img, lang)
        };
        let Some(img) = img else {
            let _ = app.emit("ocr_result", serde_json::json!({ "error": "Görüntü yok" }));
            return;
        };

        let ocr = tauri::async_runtime::spawn_blocking(move || {
            crate::ocr::recognize(&img, lang.as_deref())
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);

        let text = match ocr {
            Ok(text) => {
                let _ = app.emit("ocr_result", serde_json::json!({ "text": text }));
                text
            }
            Err(e) => {
                let _ = app.emit("ocr_result", serde_json::json!({ "error": e }));
                return;
            }
        };

        if text.trim().is_empty() {
            return;
        }
        let translator = app.state::<crate::translate::Translator>();
        let payload = match translator.translate(&text).await {
            Ok(t) => serde_json::json!({ "text": t.text, "engine": t.engine, "cached": t.cached }),
            Err(e) => serde_json::json!({ "error": e }),
        };
        let _ = app.emit("translation", payload);
    });
}

#[tauri::command]
pub async fn ocr_languages() -> Result<Vec<crate::ocr::OcrLanguage>, String> {
    crate::ocr::languages()
}

#[tauri::command]
pub async fn set_ocr_lang(
    app: AppHandle,
    session: State<'_, Session>,
    tag: String,
) -> Result<(), String> {
    *session.lang.lock().unwrap() = if tag.is_empty() { None } else { Some(tag) };
    if session.last.lock().unwrap().is_some() {
        run_ocr(app);
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_selection(app: AppHandle) -> Result<(), String> {
    close_selector(&app);
    Ok(())
}
