#[path = "../src/ocr.rs"]
mod ocr;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("kullanim: ocr_test <png> [dil]");
    let lang = args.next();
    println!("Diller: {:?}", ocr::languages().map(|l| l.into_iter().map(|x| x.tag).collect::<Vec<_>>()));
    let img = image::open(&path).expect("resim acilamadi").to_rgba8();
    match ocr::recognize(&img, lang.as_deref()) {
        Ok(t) => println!("SONUC:\n{t}"),
        Err(e) => println!("HATA: {e}"),
    }
}
