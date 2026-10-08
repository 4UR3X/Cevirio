#[path = "../src/blocks.rs"]
mod blocks;
#[path = "../src/ocr.rs"]
mod ocr;
#[path = "../src/translate.rs"]
mod translate;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("kullanim: pipeline_test <png> [dil] [cikis.json]");
    let lang = args.next().filter(|l| l != "-");
    let json_out = args.next();
    let img = image::open(&path).expect("resim acilamadi").to_rgba8();

    let t0 = std::time::Instant::now();
    let lines = ocr::recognize_lines(&img, lang.as_deref()).expect("ocr hatasi");
    let t_ocr = t0.elapsed();
    let found = lines.len();
    let blocks = blocks::group(lines);

    let dir = std::env::temp_dir().join("cevirio_test2");
    let _ = std::fs::remove_dir_all(&dir);
    let t = translate::Translator::open(dir).expect("acilamadi");

    let mut unique: Vec<String> = Vec::new();
    for b in &blocks {
        if blocks::translatable(&b.text) && !unique.contains(&b.text) {
            unique.push(b.text.clone());
        }
    }
    let t1 = std::time::Instant::now();
    t.warmup().await;
    let t1 = std::time::Instant::now();
    let results = t.translate_many(&unique).await;
    let _ = &t1;
    let t_tr = t1.elapsed();

    let mut out = Vec::new();
    for b in &blocks {
        let Some(i) = unique.iter().position(|u| *u == b.text) else { continue };
        if let Ok(r) = &results[i] {
            println!("[{:>4.0},{:>4.0} {:>4.0}x{:>3.0}] {:?} -> {:?}", b.x, b.y, b.w, b.h, b.text, r.text);
            out.push(blocks::out_block(&img, b, r.text.clone()));
        }
    }
    println!(
        "OCR {:?} ({} parca, {} blok), ceviri {:?} ({} benzersiz)",
        t_ocr, found, blocks.len(), t_tr, unique.len()
    );
    if let Some(p) = json_out {
        std::fs::write(p, serde_json::to_string(&out).unwrap()).unwrap();
    }
}
