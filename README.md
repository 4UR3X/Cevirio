# Çevirio

Hızlı, çapraz platform (Windows / Linux / macOS) ekran çeviri aracı. Ekran yakalama, OCR ve overlay ile çalışır; oyun veya uygulama sürecine müdahale etmez (enjeksiyon yok).

## Özellikler
- Bölge seçimi ile ekran yakalama
- Windows yerleşik OCR (dil seçilebilir)
- Çeviri motorları: Google, DeepL, OpenAI uyumlu API / yerel model (Ollama); hata olursa sıradaki motora geçer
- SQLite çeviri önbelleği
- Çeviriyi seçilen bölgenin üstüne bindiren, tıklama geçiren overlay (ekran yakalamadan gizli)
- Canlı mod: bölgeyi izler, metin değişince otomatik çevirir

## Kısayollar
| Kısayol | İşlev |
|---|---|
| `Ctrl+Shift+X` | Bölge seç |
| `Ctrl+Shift+L` | Canlı modu aç/kapat |
| `Ctrl+Shift+H` | Overlay'i gizle |

## Durum
Erken geliştirme aşaması. Tasarım: [docs/TASARIM.md](docs/TASARIM.md)

## Geliştirme
Gereksinimler: Rust (stable), Windows'ta MSVC Build Tools.

```
cd app/src-tauri
cargo run
```

## Katkı
Katkılar açıktır. Bir issue açın veya pull request gönderin.

## Lisans
[GNU GPL v3.0](LICENSE). Projeyi kullanabilir ve değiştirebilirsiniz; türev işlerin de aynı lisansla ve orijinal projeye atıfla açık kalması zorunludur.
