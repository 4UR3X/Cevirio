# Çevirio – Tasarım Belgesi

## MemoFast / benzeri araçların mantığı
Bu tür araçlar iki yöntemden birini kullanır:
1. **Ekran yakalama + OCR + overlay**: Ekrandaki bölge yakalanır, metin okunur, çevrilir, şeffaf bir pencerede üstüne yazılır. Oyuna dokunmaz → anti-cheat güvenli.
2. **Oyun motoru hook/enjeksiyon** (Unity/Unreal): Oyun sürecine DLL enjekte edilir. Hızlı ama **ban riski** var.

**Çevirio kararı:** Yöntem 1 (enjeksiyon yok, ban riski yok), her oyunda/uygulamada çalışır.

## Boru hattı (pipeline)
```
Yakalama -> Ön işleme -> OCR -> Metin birleştirme/temizleme -> Önbellek? -> Çeviri -> Overlay
```
Hız stratejileri:
- Değişiklik algılama (frame hash/diff): ekran değişmediyse OCR yok.
- Çeviri önbelleği (SQLite, kalıcı) + sözlük/terim listesi.
- OCR ve çeviri paralel/asenkron; sonuç gelince overlay güncellenir.
- Yerel OCR (Windows.Media.Ocr / Tesseract / PaddleOCR-ONNX) -> ağ gecikmesi yok.

## Çeviri motorları (eklenti mimarisi)
| Motor | Not |
|---|---|
| Google/Bing (ücretsiz uç noktalar) | Hızlı, varsayılan |
| DeepL / LLM API (Gemini, OpenAI) | Kaliteli, kullanıcı anahtarı |
| Yerel LLM (Ollama, llama.cpp) / Argos / NLLB | Çevrimdışı, ücretsiz |

Ortak `Translator` trait'i; yedekleme (fallback) zinciri.

## Teknoloji
- **Tauri 2 (Rust çekirdek) + hafif arayüz (Svelte/TS)** – Windows, Linux, macOS
- Yakalama: `xcap`/`scrap` (Win: DXGI/WGC, Linux: PipeWire/X11)
- Overlay: şeffaf, tıklama geçiren, hep üstte pencere (borderless oyun modunda çalışır; exclusive fullscreen için kullanıcıya uyarı)
- Kısayollar: global hotkey (bölge seç, çevir, aç/kapat)

## Aşamalar
1. **İskelet**: Tauri projesi, ayarlar, global kısayol, boş overlay
2. **Yakalama**: bölge seçimi + ekran yakalama
3. **OCR**: Windows OCR (Win), Tesseract/Paddle (Linux), dil seçimi
4. **Çeviri motorları + önbellek**
5. **Canlı mod**: değişiklik algılama, sürekli çeviri, overlay stili
6. **Gelişmiş**: sözlük, çeviri belleği, ses/altyazı, yerel AI, paketleme (MSI/AppImage/deb)

## Gereksinimler (Windows geliştirme)
Rust (rustup), MSVC Build Tools, WebView2 (Win11'de hazır), Node 20 (mevcut).
