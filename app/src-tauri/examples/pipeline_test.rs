#[path = "../src/blocks.rs"]
mod blocks;
#[path = "../src/ocr.rs"]
mod ocr;
#[path = "../src/translate.rs"]
mod translate;

#[tokio::main]
async fn main() {
    let path = std::env::args().nth(1).expect("kullanim: pipeline_test <png> [dil]");
    let lang = std::env::args().nth(2);
    let img = image::open(&path).expect("resim acilamadi").to_rgba8();

    let lines = ocr::recognize_lines(&img, lang.as_deref()).expect("ocr hatasi");
    println!("OCR parca sayisi: {}", lines.len());
    let blocks = blocks::group(lines);
    println!("Blok sayisi: {}", blocks.len());

    let dir = std::env::temp_dir().join("cevirio_test2");
    let _ = std::fs::remove_dir_all(&dir);
    let t = translate::Translator::open(dir).expect("acilamadi");

    for b in &blocks {
        let (bg, fg) = blocks::colors(&img, b.x, b.y, b.w, b.h);
        let tr = if blocks::translatable(&b.text) {
            t.translate(&b.text).await.map(|x| x.text).unwrap_or_else(|e| format!("HATA {e}"))
        } else {
            "(atlandi)".into()
        };
        println!(
            "[{:>4.0},{:>4.0} {:>4.0}x{:>3.0}] {} {} | {:?} -> {:?}",
            b.x, b.y, b.w, b.h, bg, fg, b.text, tr
        );
    }
}
