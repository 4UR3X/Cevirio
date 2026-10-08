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

#[derive(Clone, Copy)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub mon_x: i32,
    pub mon_y: i32,
}

#[derive(Default)]
pub struct Session {
    frozen: Mutex<Option<RgbaImage>>,
    last: Mutex<Option<RgbaImage>>,
    hid_main: AtomicBool,
    lang: Mutex<Option<String>>,
    monitor_id: Mutex<Option<u32>>,
    mon_origin: Mutex<(i32, i32)>,
    region: Mutex<Option<Region>>,
    last_ocr: Mutex<String>,
    pub last_blocks: Mutex<serde_json::Value>,
    pub live: AtomicBool,
    pub live_gen: std::sync::atomic::AtomicU64,
}

impl Session {
    pub fn region(&self) -> Option<Region> {
        *self.region.lock().unwrap()
    }
    pub fn monitor_id(&self) -> Option<u32> {
        *self.monitor_id.lock().unwrap()
    }
    pub fn lang(&self) -> Option<String> {
        self.lang.lock().unwrap().clone()
    }
    pub fn last_image(&self) -> Option<RgbaImage> {
        self.last.lock().unwrap().clone()
    }
    pub fn set_target(&self, monitor_id: u32, region: Region) {
        *self.monitor_id.lock().unwrap() = Some(monitor_id);
        *self.region.lock().unwrap() = Some(region);
        self.last_ocr.lock().unwrap().clear();
    }
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
    crate::live::stop(app);

    if let Some(main) = app.get_webview_window("main") {
        if main.is_visible().unwrap_or(false) && !main.is_minimized().unwrap_or(false) {
            let _ = main.hide();
            session.hid_main.store(true, Ordering::SeqCst);
        }
    }
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }
    std::thread::sleep(Duration::from_millis(220));

    let result = (|| -> Result<(), String> {
        let cursor = app.cursor_position().map_err(|e| e.to_string())?;
        let monitor = xcap::Monitor::from_point(cursor.x as i32, cursor.y as i32)
            .map_err(|e| e.to_string())?;
        let img = monitor.capture_image().map_err(|e| e.to_string())?;
        let scale = monitor.scale_factor().map_err(|e| e.to_string())? as f64;
        let monitor_id = monitor.id().map_err(|e| e.to_string())?;
        let mx = monitor.x().map_err(|e| e.to_string())?;
        let my = monitor.y().map_err(|e| e.to_string())?;

        *session.frozen.lock().unwrap() = Some(img);
        *session.monitor_id.lock().unwrap() = Some(monitor_id);
        *session.mon_origin.lock().unwrap() = (mx, my);

        let window = WebviewWindowBuilder::new(app, "selector", WebviewUrl::App("selector.html".into()))
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .position(mx as f64 / scale, my as f64 / scale)
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
    let (cropped, region) = {
        let guard = session.frozen.lock().unwrap();
        let img = guard.as_ref().ok_or("Görüntü yok")?;
        let x = x.min(img.width().saturating_sub(1));
        let y = y.min(img.height().saturating_sub(1));
        let w = w.min(img.width() - x).max(1);
        let h = h.min(img.height() - y).max(1);
        let (mon_x, mon_y) = *session.mon_origin.lock().unwrap();
        (
            image::imageops::crop_imm(img, x, y, w, h).to_image(),
            Region { x, y, w, h, mon_x, mon_y },
        )
    };
    let url = to_data_url(&cropped)?;
    *session.last.lock().unwrap() = Some(cropped);
    *session.region.lock().unwrap() = Some(region);
    session.last_ocr.lock().unwrap().clear();
    *session.last_blocks.lock().unwrap() = serde_json::Value::Null;
    close_selector(&app);
    app.emit("captured", url).map_err(|e| e.to_string())?;
    let _ = crate::live::start(&app);
    run_ocr(app);
    Ok(())
}

pub fn run_ocr(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let session = app.state::<Session>();
        let img = session.last.lock().unwrap().clone();
        let lang = session.lang();
        match img {
            Some(img) => {
                pipeline(&app, img, lang, false).await;
            }
            None => {
                let _ = app.emit("ocr_result", serde_json::json!({ "error": "Görüntü yok" }));
            }
        }
    });
}

pub async fn pipeline(app: &AppHandle, img: RgbaImage, lang: Option<String>, live: bool) {
    let t0 = std::time::Instant::now();
    let img = std::sync::Arc::new(img);
    let ocr_img = img.clone();
    let ocr = tauri::async_runtime::spawn_blocking(move || {
        crate::ocr::recognize_lines(&ocr_img, lang.as_deref())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    let t_ocr = t0.elapsed().as_millis();

    let lines = match ocr {
        Ok(lines) => lines,
        Err(e) => {
            crate::debug::log(&format!("OCR hatasi: {e}"));
            let _ = app.emit("ocr_result", serde_json::json!({ "error": e }));
            return;
        }
    };

    let blocks = crate::blocks::group(lines);
    let text = blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let session = app.state::<Session>();
    {
        let mut last = session.last_ocr.lock().unwrap();
        if live && *last == text {
            return;
        }
        *last = text.clone();
    }
    let _ = app.emit("ocr_result", serde_json::json!({ "text": text }));

    let translator = app.state::<crate::translate::Translator>();
    let mut unique: Vec<String> = Vec::new();
    for b in &blocks {
        if crate::blocks::translatable(&b.text) && !unique.contains(&b.text) {
            unique.push(b.text.clone());
        }
    }
    let translated = translator.translate_many(&unique).await;
    let lookup: std::collections::HashMap<&str, &Result<crate::translate::Translation, String>> =
        unique.iter().map(String::as_str).zip(translated.iter()).collect();
    let results: Vec<Option<Result<crate::translate::Translation, String>>> = blocks
        .iter()
        .map(|b| lookup.get(b.text.as_str()).map(|r| (*r).clone()))
        .collect();
    let t_total = t0.elapsed().as_millis();
    crate::debug::log(&format!(
        "pipeline: ocr {t_ocr}ms, ceviri {}ms, toplam {t_total}ms, parca {}, benzersiz {}",
        t_total - t_ocr,
        blocks.len(),
        unique.len()
    ));


    let mut out: Vec<crate::blocks::BlockOut> = Vec::new();
    let mut first_error: Option<String> = None;
    let mut engine = String::new();
    let mut all_cached = true;
    for (b, r) in blocks.iter().zip(results) {
        match r {
            Some(Ok(t)) => {
                let same = normalize_cmp(&t.text) == normalize_cmp(&b.text);
                if same {
                    continue;
                }
                engine = t.engine.clone();
                all_cached &= t.cached;
                out.push(crate::blocks::out_block(&img, b, t.text));
            }
            Some(Err(e)) => {
                first_error.get_or_insert(e);
            }
            None => {}
        }
    }

    let value = serde_json::to_value(&out).unwrap_or_default();
    *session.last_blocks.lock().unwrap() = value.clone();
    let _ = app.emit("blocks", value);
    if !out.is_empty() {
        crate::overlay::show(app);
    }

    let payload = match (out.is_empty(), first_error) {
        (true, Some(e)) => serde_json::json!({ "error": e }),
        _ => serde_json::json!({
            "text": out.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n"),
            "engine": engine,
            "cached": all_cached,
        }),
    };
    let _ = app.emit("translation", payload);
}

fn normalize_cmp(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
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
        session.last_ocr.lock().unwrap().clear();
        run_ocr(app);
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_selection(app: AppHandle) -> Result<(), String> {
    close_selector(&app);
    Ok(())
}
