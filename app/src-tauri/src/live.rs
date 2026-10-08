use std::{sync::atomic::Ordering, time::Duration};

use image::RgbaImage;
use tauri::{AppHandle, Emitter, Manager};

use crate::capture::{self, Region, Session};

const TICK: Duration = Duration::from_millis(350);
const STABLE: f32 = 0.5;
const CHANGED: f32 = 1.5;

fn diff(a: &RgbaImage, b: &RgbaImage) -> f32 {
    if a.dimensions() != b.dimensions() {
        return f32::MAX;
    }
    let (ra, rb) = (a.as_raw(), b.as_raw());
    let mut sum = 0u64;
    let mut count = 0u64;
    let mut i = 0;
    while i + 2 < ra.len() {
        for c in 0..3 {
            sum += (ra[i + c] as i32 - rb[i + c] as i32).unsigned_abs() as u64;
        }
        count += 3;
        i += 28;
    }
    if count == 0 {
        0.0
    } else {
        sum as f32 / count as f32
    }
}

fn grab(monitor: &xcap::Monitor, r: &Region) -> Result<RgbaImage, String> {
    monitor
        .capture_region(r.x, r.y, r.w, r.h)
        .map_err(|e| e.to_string())
}

pub fn toggle(app: &AppHandle) -> Result<bool, String> {
    let session = app.state::<Session>();
    if session.live.swap(false, Ordering::SeqCst) {
        let _ = app.emit("live_state", false);
        return Ok(false);
    }
    let region = session.region().ok_or("Önce bir bölge seç")?;
    let monitor_id = session.monitor_id().ok_or("Monitör bulunamadı")?;
    session.live.store(true, Ordering::SeqCst);
    let _ = app.emit("live_state", true);

    let app = app.clone();
    std::thread::spawn(move || run(app, monitor_id, region));
    Ok(true)
}

fn run(app: AppHandle, monitor_id: u32, region: Region) {
    let monitor = xcap::Monitor::all()
        .ok()
        .and_then(|all| all.into_iter().find(|m| m.id().ok() == Some(monitor_id)));
    let Some(monitor) = monitor else {
        app.state::<Session>().live.store(false, Ordering::SeqCst);
        let _ = app.emit("live_state", false);
        return;
    };
    let mut prev: Option<RgbaImage> = None;
    let mut done: Option<RgbaImage> = None;

    while app.state::<Session>().live.load(Ordering::SeqCst) {
        std::thread::sleep(TICK);
        let Ok(cur) = grab(&monitor, &region) else { continue };

        let stable = prev.as_ref().map_or(false, |p| diff(p, &cur) < STABLE);
        let changed = done.as_ref().map_or(true, |d| diff(d, &cur) >= CHANGED);
        prev = Some(cur.clone());

        if stable && changed {
            done = Some(cur.clone());
            let lang = app.state::<Session>().lang();
            tauri::async_runtime::block_on(capture::pipeline(&app, cur, lang, true));
        }
    }
    let _ = app.emit("live_state", false);
}

#[tauri::command]
pub async fn toggle_live(app: AppHandle) -> Result<bool, String> {
    toggle(&app)
}
