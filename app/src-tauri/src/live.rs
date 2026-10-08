use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use image::RgbaImage;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    capture::{self, Region, Session},
    debug,
};

const TICK: Duration = Duration::from_millis(120);
const STABLE: f32 = 0.6;
const CHANGED: f32 = 1.2;

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
        i += 20;
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

pub fn start(app: &AppHandle) -> Result<(), String> {
    let session = app.state::<Session>();
    let region = session.region().ok_or("Önce bir bölge seç")?;
    let monitor_id = session.monitor_id().ok_or("Monitör bulunamadı")?;
    let generation = session.live_gen.fetch_add(1, Ordering::SeqCst) + 1;
    session.live.store(true, Ordering::SeqCst);
    let _ = app.emit("live_state", true);

    let initial = session.last_image();
    let app = app.clone();
    std::thread::spawn(move || run(app, monitor_id, region, generation, initial));
    Ok(())
}

pub fn stop(app: &AppHandle) {
    let session = app.state::<Session>();
    session.live.store(false, Ordering::SeqCst);
    session.live_gen.fetch_add(1, Ordering::SeqCst);
    let _ = app.emit("live_state", false);
}

pub fn toggle(app: &AppHandle) -> Result<bool, String> {
    if app.state::<Session>().live.load(Ordering::SeqCst) {
        stop(app);
        Ok(false)
    } else {
        start(app)?;
        Ok(true)
    }
}

fn run(app: AppHandle, monitor_id: u32, region: Region, generation: u64, initial: Option<RgbaImage>) {
    let monitor = xcap::Monitor::all()
        .ok()
        .and_then(|all| all.into_iter().find(|m| m.id().ok() == Some(monitor_id)));
    let Some(monitor) = monitor else {
        stop(&app);
        return;
    };

    let alive = |app: &AppHandle| {
        let s = app.state::<Session>();
        s.live.load(Ordering::SeqCst) && s.live_gen.load(Ordering::SeqCst) == generation
    };

    let mut prev: Option<RgbaImage> = None;
    let mut done: Option<RgbaImage> = initial;

    while alive(&app) {
        std::thread::sleep(TICK);
        let t = Instant::now();
        let cur = match grab(&monitor, &region) {
            Ok(c) => c,
            Err(e) => {
                debug::log(&format!("yakalama hatasi: {e}"));
                continue;
            }
        };
        let grab_ms = t.elapsed().as_millis();

        let stable = prev.as_ref().map_or(false, |p| diff(p, &cur) < STABLE);
        let changed = done.as_ref().map_or(true, |d| diff(d, &cur) >= CHANGED);
        prev = Some(cur.clone());

        if stable && changed {
            debug::log(&format!("canli: degisim algilandi (yakalama {grab_ms}ms)"));
            done = Some(cur.clone());
            let lang = app.state::<Session>().lang();
            tauri::async_runtime::block_on(capture::pipeline(&app, cur, lang, true));
        }
    }
}

#[tauri::command]
pub async fn toggle_live(app: AppHandle) -> Result<bool, String> {
    toggle(&app)
}
