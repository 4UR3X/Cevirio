use image::RgbaImage;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct OcrLanguage {
    pub tag: String,
    pub name: String,
}

fn prepare(img: &RgbaImage) -> RgbaImage {
    let h = img.height();
    if h < 80 {
        let factor = (120 / h.max(1)).clamp(2, 4);
        image::imageops::resize(
            img,
            img.width() * factor,
            h * factor,
            image::imageops::FilterType::CatmullRom,
        )
    } else {
        img.clone()
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

    pub fn recognize(img: &RgbaImage, lang: Option<&str>) -> Result<String, String> {
        let img = prepare(img);
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

        let mut lines = Vec::new();
        for line in result.Lines().map_err(err)? {
            lines.push(line.Text().map_err(err)?.to_string());
        }
        Ok(lines.join("\n"))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub fn languages() -> Result<Vec<OcrLanguage>, String> {
        Ok(Vec::new())
    }

    pub fn recognize(_img: &RgbaImage, _lang: Option<&str>) -> Result<String, String> {
        Err("Bu platformda OCR motoru henüz eklenmedi".into())
    }
}

pub use platform::{languages, recognize};
