use std::{
    fs::OpenOptions,
    io::Write,
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("CEVIRIO_DEBUG").is_ok())
}

pub fn log(msg: &str) {
    if !on() {
        return;
    }
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() % 1_000_000)
        .unwrap_or(0);
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("cevirio_debug.log"))
    {
        let _ = writeln!(f, "[{ms:>6}] {msg}");
    }
}
