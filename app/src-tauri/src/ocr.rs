use image::RgbaImage;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct OcrLanguage {
    pub tag: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct OcrLine {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

fn prepare(img: &RgbaImage) -> (RgbaImage, u32) {
    let h = img.height();
    if h < 80 {
        let factor = (120 / h.max(1)).clamp(2, 4);
        (
            image::imageops::resize(
                img,
                img.width() * factor,
                h * factor,
                image::imageops::FilterType::CatmullRom,
            ),
            factor,
        )
    } else {
        (img.clone(), 1)
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::{
        core::HSTRING,
        Globalization::Language,
        Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
        Media::Ocr::OcrEngine,
        Storage::Streams::DataWriter,
    };

    fn err(e: windows::core::Error) -> String {
        e.message()
    }

    pub fn languages() -> Result<Vec<OcrLanguage>, String> {
        let list = OcrEngine::AvailableRecognizerLanguages().map_err(err)?;
        let mut out = Vec::new();
        for l in list {
            out.push(OcrLanguage {
                tag: l.LanguageTag().map_err(err)?.to_string(),
                name: l.DisplayName().map_err(err)?.to_string(),
            });
        }
        Ok(out)
    }

    struct Word {
        text: String,
        x: f32,
        y: f32,
        r: f32,
        b: f32,
    }

    fn flush(words: &mut Vec<Word>, factor: f32, out: &mut Vec<OcrLine>) {
        if words.is_empty() {
            return;
        }
        let x = words.iter().map(|w| w.x).fold(f32::MAX, f32::min);
        let y = words.iter().map(|w| w.y).fold(f32::MAX, f32::min);
        let r = words.iter().map(|w| w.r).fold(f32::MIN, f32::max);
        let b = words.iter().map(|w| w.b).fold(f32::MIN, f32::max);
        let text = words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
        out.push(OcrLine {
            text,
            x: x / factor,
            y: y / factor,
            w: (r - x) / factor,
            h: (b - y) / factor,
        });
        words.clear();
    }

    pub fn recognize_lines(img: &RgbaImage, lang: Option<&str>) -> Result<Vec<OcrLine>, String> {
        let (img, factor) = prepare(img);
        let factor = factor as f32;
        let mut bgra = img.as_raw().clone();
        for px in bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        let writer = DataWriter::new().map_err(err)?;
        writer.WriteBytes(&bgra).map_err(err)?;
        let buffer = writer.DetachBuffer().map_err(err)?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            img.width() as i32,
            img.height() as i32,
        )
        .map_err(err)?;

        let engine = match lang {
            Some(tag) if !tag.is_empty() => {
                let language = Language::CreateLanguage(&HSTRING::from(tag)).map_err(err)?;
                OcrEngine::TryCreateFromLanguage(&language)
                    .map_err(|_| format!("'{tag}' OCR dil paketi Windows'ta kurulu değil"))?
            }
            _ => OcrEngine::TryCreateFromUserProfileLanguages().map_err(err)?,
        };

        let result = engine
            .RecognizeAsync(&bitmap)
            .map_err(err)?
            .join()
            .map_err(err)?;

        let mut out = Vec::new();
        for line in result.Lines().map_err(err)? {
            let mut words: Vec<Word> = Vec::new();
            for word in line.Words().map_err(err)? {
                let rect = word.BoundingRect().map_err(err)?;
                let w = Word {
                    text: word.Text().map_err(err)?.to_string(),
                    x: rect.X,
                    y: rect.Y,
                    r: rect.X + rect.Width,
                    b: rect.Y + rect.Height,
                };
                if let Some(prev) = words.last() {
                    let height = (prev.b - prev.y).max(w.b - w.y);
                    if w.x - prev.r > height * 1.1 {
                        flush(&mut words, factor, &mut out);
                    }
                }
                words.push(w);
            }
            flush(&mut words, factor, &mut out);
        }
        Ok(out)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub fn languages() -> Result<Vec<OcrLanguage>, String> {
        Ok(Vec::new())
    }

    pub fn recognize_lines(_img: &RgbaImage, _lang: Option<&str>) -> Result<Vec<OcrLine>, String> {
        Err("Bu platformda OCR motoru henüz eklenmedi".into())
    }
}

pub use platform::{languages, recognize_lines};

#[allow(dead_code)]
pub fn recognize(img: &RgbaImage, lang: Option<&str>) -> Result<String, String> {
    Ok(recognize_lines(img, lang)?
        .into_iter()
        .map(|l| l.text)
        .collect::<Vec<_>>()
        .join("\n"))
}
