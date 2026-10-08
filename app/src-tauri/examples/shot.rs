fn main() {
    let out = std::env::args().nth(1).expect("kullanim: shot <cikis.png>");
    let monitors = xcap::Monitor::all().expect("monitor yok");
    let m = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or(monitors.first())
        .expect("monitor yok");
    println!(
        "monitor {}x{} @({},{}) olcek {}",
        m.width().unwrap(),
        m.height().unwrap(),
        m.x().unwrap(),
        m.y().unwrap(),
        m.scale_factor().unwrap()
    );
    m.capture_image().expect("yakalanamadi").save(&out).expect("kaydedilemedi");
}
