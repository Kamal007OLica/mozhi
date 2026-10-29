//! Model catalog, download and deletion. Models live in `<app data>/models/`.
//!
//! Sizes and SHA-256s are the Git LFS object ids published on Hugging Face, so every
//! download is verified byte-for-byte. Any other `*.bin` ggml model dropped into the
//! folder (e.g. one made with `scripts/convert-tamil-model.sh`) shows up as a custom model.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use super::whisper::ModelKind;
use crate::types::ModelInfo;

pub(crate) struct Entry {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub speed: u8,
    pub tamil_quality: u8,
    pub kind: ModelKind,
    pub recommended: bool,
}

const HF: &str = "https://huggingface.co";

pub(crate) const CATALOG: &[Entry] = &[
    Entry {
        id: "large-v3-turbo-q5",
        name: "Whisper Large v3 Turbo · Q5",
        description: "Best all-rounder: Tamil, Tanglish and English in one model, and keeps English words in English. Fast on Apple Silicon and modern PCs.",
        file: "ggml-large-v3-turbo-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        bytes: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        speed: 4,
        tamil_quality: 3,
        kind: ModelKind::Multilingual,
        recommended: true,
    },
    Entry {
        id: "tamil-small-q8",
        name: "Tamil Small · fine-tuned",
        description: "Whisper fine-tuned on Tamil by IIT Madras SPRING Lab (vasista22). The most accurate choice for pure Tamil, and light — but Tamil only: English words come out in Tamil script.",
        file: "ggml-tamil-small-q8_0.bin",
        url: "https://huggingface.co/Pomni/whisper-tamil-small-ggml-allquants/resolve/main/ggml-tamil-small-q8_0.bin",
        bytes: 264_464_607,
        sha256: "86b6b27f4cd9cf6038792731e004421fa08725a5c4eefd652d9b389e7e68a879",
        speed: 4,
        tamil_quality: 4,
        kind: ModelKind::TamilOnly,
        recommended: false,
    },
    Entry {
        id: "tamil-medium",
        name: "Tamil Medium · fine-tuned",
        description: "The larger vasista22 Tamil fine-tune. Highest Tamil accuracy; Tamil only, and slower.",
        file: "ggml-tamil-medium.bin",
        url: "https://huggingface.co/shiyammm/whisper-tamil-medium-ggml/resolve/main/ggml-tamil-medium.bin",
        bytes: 1_533_763_059,
        sha256: "584a2fbb4b4ef16355a53f759db42549ef5ec4736aece5456af3898cd0449a08",
        speed: 2,
        tamil_quality: 5,
        kind: ModelKind::TamilOnly,
        recommended: false,
    },
    Entry {
        id: "large-v3-turbo",
        name: "Whisper Large v3 Turbo · full precision",
        description: "Same as the recommended model without quantisation — marginally more accurate, three times the size.",
        file: "ggml-large-v3-turbo.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        bytes: 1_624_555_275,
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
        speed: 3,
        tamil_quality: 3,
        kind: ModelKind::Multilingual,
        recommended: false,
    },
    Entry {
        id: "large-v3-q5",
        name: "Whisper Large v3 · Q5",
        description: "The full 32-layer decoder. Most robust on accents and noisy rooms, noticeably slower.",
        file: "ggml-large-v3-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin",
        bytes: 1_081_140_203,
        sha256: "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1",
        speed: 1,
        tamil_quality: 3,
        kind: ModelKind::Multilingual,
        recommended: false,
    },
    Entry {
        id: "medium-q5",
        name: "Whisper Medium · Q5",
        description: "Middle ground for older machines. Usable Tamil, good English.",
        file: "ggml-medium-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q5_0.bin",
        bytes: 539_212_467,
        sha256: "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
        speed: 2,
        tamil_quality: 2,
        kind: ModelKind::Multilingual,
        recommended: false,
    },
    Entry {
        id: "small-q5",
        name: "Whisper Small · Q5",
        description: "Fast and small. Fine for English; Tamil accuracy is rough.",
        file: "ggml-small-q5_1.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
        bytes: 190_085_487,
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        speed: 5,
        tamil_quality: 1,
        kind: ModelKind::Multilingual,
        recommended: false,
    },
    Entry {
        id: "base-q5",
        name: "Whisper Base · Q5",
        description: "Tiny, for testing or very old hardware. English only in practice.",
        file: "ggml-base-q5_1.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin",
        bytes: 59_707_625,
        sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
        speed: 5,
        tamil_quality: 1,
        kind: ModelKind::Multilingual,
        recommended: false,
    },
];

const CUSTOM_PREFIX: &str = "custom:";

/// Model used when the user has not picked one.
pub fn default_model_id() -> &'static str {
    "large-v3-turbo-q5"
}

fn entry(id: &str) -> Option<&'static Entry> {
    CATALOG.iter().find(|e| e.id == id)
}

/// File path and kind for an installed model, or None if it isn't on disk.
pub(crate) fn resolve(models_dir: &Path, id: &str) -> Option<(PathBuf, ModelKind)> {
    let id = if id.is_empty() { default_model_id() } else { id };
    let (path, kind) = if let Some(file) = id.strip_prefix(CUSTOM_PREFIX) {
        if file.contains(['/', '\\']) || file.starts_with('.') {
            return None;
        }
        (models_dir.join(file), custom_kind(file))
    } else {
        let e = entry(id)?;
        (models_dir.join(e.file), e.kind)
    };
    path.is_file().then_some((path, kind))
}

fn custom_kind(file: &str) -> ModelKind {
    if file.to_lowercase().contains("tamil") {
        ModelKind::TamilOnly
    } else {
        ModelKind::Multilingual
    }
}

/// All known models, with `installed` filled from disk, followed by custom models.
pub fn catalog(models_dir: &Path) -> Vec<ModelInfo> {
    let mut out: Vec<ModelInfo> = CATALOG
        .iter()
        .map(|e| ModelInfo {
            id: e.id.into(),
            name: e.name.into(),
            description: e.description.into(),
            size_mb: (e.bytes / 1_000_000) as u32,
            speed: e.speed,
            tamil_quality: e.tamil_quality,
            url: e.url.into(),
            sha256: Some(e.sha256.into()),
            recommended: e.recommended,
            installed: models_dir.join(e.file).is_file(),
        })
        .collect();

    if let Ok(dir) = std::fs::read_dir(models_dir) {
        let mut custom: Vec<ModelInfo> = dir
            .flatten()
            .filter_map(|f| {
                let name = f.file_name().to_string_lossy().to_string();
                let meta = f.metadata().ok()?;
                let known = CATALOG.iter().any(|e| e.file == name);
                (meta.is_file() && name.ends_with(".bin") && !known && !name.starts_with('.')).then(|| ModelInfo {
                    id: format!("{CUSTOM_PREFIX}{name}"),
                    name: name.trim_end_matches(".bin").trim_start_matches("ggml-").to_string(),
                    description: match custom_kind(&name) {
                        ModelKind::TamilOnly => "Custom Tamil model from your models folder.".into(),
                        ModelKind::Multilingual => "Custom model from your models folder.".into(),
                    },
                    size_mb: (meta.len() / 1_000_000) as u32,
                    speed: 3,
                    tamil_quality: 3,
                    url: String::new(),
                    sha256: None,
                    recommended: false,
                    installed: true,
                })
            })
            .collect();
        custom.sort_by(|a, b| a.id.cmp(&b.id));
        out.extend(custom);
    }
    out
}

/// Streams the model to `<models_dir>/<file>.part`, resuming a previous partial download,
/// verifies its SHA-256, then renames it into place. `progress(downloaded, total)` is called
/// per received chunk. A cancelled download keeps its `.part` so the next attempt resumes.
pub async fn download(
    models_dir: &Path,
    id: &str,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(u64, u64) + Send + 'static,
) -> anyhow::Result<()> {
    let e = entry(id).ok_or_else(|| anyhow!("unknown model {id}"))?;
    tokio::fs::create_dir_all(models_dir).await.context("create models folder")?;
    let dest = models_dir.join(e.file);
    if dest.is_file() {
        progress(e.bytes, e.bytes);
        return Ok(());
    }
    let part = models_dir.join(format!("{}.part", e.file));
    let mut have = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
    if have > e.bytes {
        tokio::fs::remove_file(&part).await.ok();
        have = 0;
    }

    if have < e.bytes {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Mozhi/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(60))
            .build()?;
        let mut req = client.get(e.url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let resp = req.send().await.with_context(|| format!("could not reach {HF} — check your connection"))?;
        let status = resp.status();
        let mut file = if status == reqwest::StatusCode::PARTIAL_CONTENT {
            tokio::fs::OpenOptions::new().append(true).open(&part).await?
        } else if status.is_success() {
            have = 0;
            tokio::fs::File::create(&part).await?
        } else {
            bail!("download failed: HTTP {status}");
        };
        log::info!("downloading {id}: resuming at {have} of {} bytes", e.bytes);

        let mut stream = resp.bytes_stream();
        progress(have, e.bytes);
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                file.flush().await.ok();
                bail!("download cancelled");
            }
            let chunk = chunk.context("download interrupted — it will resume next time")?;
            file.write_all(&chunk).await?;
            have += chunk.len() as u64;
            progress(have, e.bytes);
        }
        file.flush().await?;
        file.sync_all().await.ok();
        drop(file);
        if have != e.bytes {
            bail!("download incomplete ({have} of {} bytes) — try again to resume", e.bytes);
        }
    }

    let check = part.clone();
    let digest = tokio::task::spawn_blocking(move || sha256_file(&check)).await??;
    if digest != e.sha256 {
        tokio::fs::remove_file(&part).await.ok();
        bail!("model file failed verification (checksum mismatch) — please download again");
    }
    tokio::fs::rename(&part, &dest).await.context("finish model download")?;
    progress(e.bytes, e.bytes);
    Ok(())
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn delete(models_dir: &Path, id: &str) -> anyhow::Result<()> {
    let file = match id.strip_prefix(CUSTOM_PREFIX) {
        Some(f) if !f.contains(['/', '\\']) && !f.starts_with('.') => f.to_string(),
        Some(_) => bail!("invalid model id"),
        None => entry(id).ok_or_else(|| anyhow!("unknown model {id}"))?.file.to_string(),
    };
    let path = models_dir.join(&file);
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("delete {}", path.display()))?;
    }
    let _ = std::fs::remove_file(models_dir.join(format!("{file}.part")));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        assert!(entry(default_model_id()).is_some());
        assert_eq!(CATALOG.iter().filter(|e| e.recommended).count(), 1);
        for e in CATALOG {
            assert_eq!(e.sha256.len(), 64, "{}", e.id);
            assert!(e.url.ends_with(e.file), "{}", e.id);
            assert!((1..=5).contains(&e.speed) && (1..=5).contains(&e.tamil_quality));
        }
    }

    #[test]
    fn custom_models_are_listed_and_resolved() {
        let dir = std::env::temp_dir().join(format!("mozhi-models-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ggml-my-tamil-large.bin"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        let list = catalog(&dir);
        let custom: Vec<_> = list.iter().filter(|m| m.id.starts_with("custom:")).collect();
        assert_eq!(custom.len(), 1);
        assert!(custom[0].installed);
        let (_, kind) = resolve(&dir, &custom[0].id).unwrap();
        assert_eq!(kind, ModelKind::TamilOnly);
        assert!(resolve(&dir, "custom:../etc/passwd").is_none());
        delete(&dir, &custom[0].id).unwrap();
        assert!(resolve(&dir, "custom:ggml-my-tamil-large.bin").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
