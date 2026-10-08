#[path = "../src/translate.rs"]
mod translate;

#[tokio::main]
async fn main() {
    let dir = std::env::temp_dir().join("cevirio_test");
    let _ = std::fs::remove_dir_all(&dir);
    let t = translate::Translator::open(dir).expect("acilamadi");
    let text = std::env::args().nth(1).unwrap_or("Press START to begin your quest".into());
    for i in 1..=2 {
        let start = std::time::Instant::now();
        match t.translate(&text).await {
            Ok(r) => println!("#{i} [{} cached={}] {:?}: {}", r.engine, r.cached, start.elapsed(), r.text),
            Err(e) => println!("#{i} HATA: {e}"),
        }
    }
}
