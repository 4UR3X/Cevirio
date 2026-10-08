use std::{fs, path::PathBuf, sync::Mutex, time::Duration};

use futures_util::future::join_all;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const GOOGLE_CHUNK_CHARS: usize = 3000;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub source_lang: String,
    pub target_lang: String,
    pub engines: Vec<String>,
    pub deepl_key: String,
    pub openai_base: String,
    pub openai_key: String,
    pub openai_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            source_lang: "auto".into(),
            target_lang: "tr".into(),
            engines: vec!["google".into()],
            deepl_key: String::new(),
            openai_base: "http://localhost:11434/v1".into(),
            openai_key: String::new(),
            openai_model: String::new(),
        }
    }
}

#[derive(Serialize, Clone)]
pub struct Translation {
    pub text: String,
    pub engine: String,
    pub cached: bool,
}

pub struct Translator {
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    db: Mutex<Connection>,
    client: reqwest::Client,
}

impl Translator {
    pub fn open(dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let settings_path = dir.join("settings.json");
        let settings = fs::read_to_string(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let db = Connection::open(dir.join("cache.db")).map_err(|e| e.to_string())?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS cache (
                 src TEXT NOT NULL,
                 sl TEXT NOT NULL,
                 tl TEXT NOT NULL,
                 out TEXT NOT NULL,
                 engine TEXT NOT NULL,
                 PRIMARY KEY (src, sl, tl)
             );",
        )
        .map_err(|e| e.to_string())?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(120))
            .tcp_nodelay(true)
            .user_agent("Cevirio/0.1")
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self {
            settings: Mutex::new(settings),
            settings_path,
            db: Mutex::new(db),
            client,
        })
    }

    pub async fn warmup(&self) {
        let _ = self
            .client
            .get("https://translate.googleapis.com/translate_a/single")
            .query(&[("client", "gtx"), ("sl", "en"), ("tl", "tr"), ("dt", "t"), ("q", "hi")])
            .send()
            .await;
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn save_settings(&self, s: Settings) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?;
        fs::write(&self.settings_path, json).map_err(|e| e.to_string())?;
        *self.settings.lock().unwrap() = s;
        Ok(())
    }

    pub fn clear_cache(&self) -> Result<(), String> {
        self.db
            .lock()
            .unwrap()
            .execute("DELETE FROM cache", [])
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn cache_get(&self, src: &str, sl: &str, tl: &str) -> Option<(String, String)> {
        self.db
            .lock()
            .unwrap()
            .query_row(
                "SELECT out, engine FROM cache WHERE src=?1 AND sl=?2 AND tl=?3",
                params![src, sl, tl],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok()
    }

    fn cache_put(&self, src: &str, sl: &str, tl: &str, out: &str, engine: &str) {
        let _ = self.db.lock().unwrap().execute(
            "INSERT OR REPLACE INTO cache (src, sl, tl, out, engine) VALUES (?1,?2,?3,?4,?5)",
            params![src, sl, tl, out, engine],
        );
    }

    pub async fn translate(&self, text: &str) -> Result<Translation, String> {
        self.translate_many(&[text.to_string()])
            .await
            .pop()
            .unwrap_or_else(|| Err("Çevrilecek metin yok".into()))
    }

    pub async fn translate_many(&self, texts: &[String]) -> Vec<Result<Translation, String>> {
        let s = self.settings();
        let norm: Vec<String> = texts.iter().map(|t| normalize(t)).collect();
        let mut out: Vec<Option<Result<Translation, String>>> = vec![None; norm.len()];

        for (i, t) in norm.iter().enumerate() {
            if t.is_empty() {
                out[i] = Some(Err("Çevrilecek metin yok".into()));
            } else if let Some((text, engine)) = self.cache_get(t, &s.source_lang, &s.target_lang) {
                out[i] = Some(Ok(Translation { text, engine, cached: true }));
            }
        }

        let mut errors: Vec<String> = Vec::new();
        for engine in &s.engines {
            let pending: Vec<usize> = (0..norm.len()).filter(|i| out[*i].is_none()).collect();
            if pending.is_empty() {
                break;
            }
            let batch: Vec<&str> = pending.iter().map(|&i| norm[i].as_str()).collect();
            let result = match engine.as_str() {
                "google" => self.google_batch(&batch, &s).await,
                "deepl" => self.deepl_batch(&batch, &s).await,
                "openai" => self.openai_batch(&batch, &s).await,
                other => Err(format!("bilinmeyen motor: {other}")),
            };
            match result {
                Ok(list) if list.len() == batch.len() => {
                    for (k, &i) in pending.iter().enumerate() {
                        if list[k].trim().is_empty() {
                            continue;
                        }
                        self.cache_put(&norm[i], &s.source_lang, &s.target_lang, &list[k], engine);
                        out[i] = Some(Ok(Translation {
                            text: list[k].clone(),
                            engine: engine.clone(),
                            cached: false,
                        }));
                    }
                }
                Ok(_) => errors.push(format!("{engine}: yanıt eksik")),
                Err(e) => errors.push(format!("{engine}: {e}")),
            }
        }

        let message = if errors.is_empty() {
            "Etkin çeviri motoru yok".to_string()
        } else {
            errors.join(" | ")
        };
        out.into_iter()
            .map(|o| o.unwrap_or_else(|| Err(message.clone())))
            .collect()
    }

    async fn google_single(&self, text: &str, s: &Settings) -> Result<String, String> {
        let resp = self
            .client
            .get("https://translate.googleapis.com/translate_a/single")
            .query(&[
                ("client", "gtx"),
                ("sl", s.source_lang.as_str()),
                ("tl", s.target_lang.as_str()),
                ("dt", "t"),
                ("q", text),
            ])
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let v: Value = resp.json().await.map_err(|e| e.to_string())?;
        let parts = v
            .get(0)
            .and_then(Value::as_array)
            .ok_or("beklenmeyen yanıt")?;
        Ok(parts
            .iter()
            .filter_map(|p| p.get(0).and_then(Value::as_str))
            .collect::<String>())
    }

    async fn google_chunk(&self, texts: &[&str], s: &Settings) -> Result<Vec<String>, String> {
        if texts.len() == 1 {
            return Ok(vec![self.google_single(texts[0], s).await?]);
        }
        let mut query: Vec<(&str, &str)> = vec![
            ("client", "gtx"),
            ("sl", s.source_lang.as_str()),
            ("tl", s.target_lang.as_str()),
            ("dt", "t"),
        ];
        for t in texts {
            query.push(("q", t));
        }
        let v: Value = self
            .client
            .get("https://translate.googleapis.com/translate_a/t")
            .query(&query)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let items = v.as_array().ok_or("beklenmeyen yanıt")?;
        items
            .iter()
            .map(|it| match it {
                Value::String(s) => Ok(s.clone()),
                Value::Array(a) => a
                    .first()
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| "beklenmeyen yanıt".to_string()),
                _ => Err("beklenmeyen yanıt".to_string()),
            })
            .collect()
    }

    async fn google_batch(&self, texts: &[&str], s: &Settings) -> Result<Vec<String>, String> {
        let mut chunks: Vec<Vec<&str>> = Vec::new();
        let mut size = 0;
        for t in texts {
            if chunks.is_empty() || size + t.len() > GOOGLE_CHUNK_CHARS {
                chunks.push(Vec::new());
                size = 0;
            }
            chunks.last_mut().unwrap().push(t);
            size += t.len();
        }
        let parts = join_all(chunks.iter().map(|c| self.google_chunk(c, s))).await;
        let mut out = Vec::with_capacity(texts.len());
        for p in parts {
            out.extend(p?);
        }
        Ok(out)
    }

    async fn deepl_batch(&self, texts: &[&str], s: &Settings) -> Result<Vec<String>, String> {
        if s.deepl_key.is_empty() {
            return Err("API anahtarı girilmemiş".into());
        }
        let host = if s.deepl_key.ends_with(":fx") {
            "api-free.deepl.com"
        } else {
            "api.deepl.com"
        };
        let mut body = json!({
            "text": texts,
            "target_lang": s.target_lang.to_uppercase(),
        });
        if s.source_lang != "auto" {
            body["source_lang"] = json!(s.source_lang.to_uppercase());
        }
        let v: Value = self
            .client
            .post(format!("https://{host}/v2/translate"))
            .header("Authorization", format!("DeepL-Auth-Key {}", s.deepl_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        v["translations"]
            .as_array()
            .ok_or("beklenmeyen yanıt")?
            .iter()
            .map(|t| {
                t["text"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "beklenmeyen yanıt".to_string())
            })
            .collect()
    }

    async fn openai_one(&self, text: &str, s: &Settings) -> Result<String, String> {
        let system = format!(
            "You are a professional game and software localization translator. Translate the user's text into the language with code '{}'. Keep names, numbers, placeholders and tags unchanged. Output only the translation, nothing else.",
            s.target_lang
        );
        let body = json!({
            "model": s.openai_model,
            "temperature": 0,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": text }
            ]
        });
        let url = format!("{}/chat/completions", s.openai_base.trim_end_matches('/'));
        let mut req = self.client.post(url).json(&body);
        if !s.openai_key.is_empty() {
            req = req.bearer_auth(&s.openai_key);
        }
        let v: Value = req
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        v["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.trim().to_owned())
            .ok_or_else(|| "beklenmeyen yanıt".into())
    }

    async fn openai_batch(&self, texts: &[&str], s: &Settings) -> Result<Vec<String>, String> {
        if s.openai_model.is_empty() {
            return Err("model adı girilmemiş".into());
        }
        join_all(texts.iter().map(|t| self.openai_one(t, s)))
            .await
            .into_iter()
            .collect()
    }
}

pub fn normalize(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if !out.is_empty() {
            let ends_sentence = out.ends_with(['.', '!', '?', ':', ';', '。', '！', '？']);
            out.push(if ends_sentence { '\n' } else { ' ' });
        }
        out.push_str(line);
    }
    out
}

#[tauri::command]
pub async fn translate_text(
    state: tauri::State<'_, Translator>,
    text: String,
) -> Result<Translation, String> {
    state.translate(&text).await
}

#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, Translator>) -> Result<Settings, String> {
    Ok(state.settings())
}

#[tauri::command]
pub async fn save_settings(
    state: tauri::State<'_, Translator>,
    settings: Settings,
) -> Result<(), String> {
    state.save_settings(settings)
}

#[tauri::command]
pub async fn clear_cache(state: tauri::State<'_, Translator>) -> Result<(), String> {
    state.clear_cache()
}
