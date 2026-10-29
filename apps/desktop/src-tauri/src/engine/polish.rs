//! Optional clean-up pass by a local LLM (Ollama API at `PolishConfig::endpoint`).
//! The model only fixes punctuation, spacing and obvious mis-hearings; anything that
//! looks like rewriting, translating or chatting is rejected and the raw text is kept.

use std::time::Duration;

use anyhow::{anyhow, Context};
use serde::Deserialize;
use serde_json::json;

use crate::types::{OutputMode, PolishConfig, PolishStatus};

const RULES: &str = "You are a dictation clean-up filter, not an assistant. The user dictated the text below by voice and a speech recogniser transcribed it. \
Return the same text with only these fixes: punctuation, capitalisation, spacing, and words that were obviously mis-heard. \
Never translate, summarise, answer, explain, add or remove content, or change the speaker's wording or dialect. \
Do not wrap the result in quotes. Output only the corrected text, nothing else.";

fn system_prompt(mode: OutputMode) -> String {
    let style = match mode {
        OutputMode::Tamil => "The text is Tamil in Tamil script, often with English words mixed in. If an English word was transcribed in Tamil script (for example மீட்டிங், லேட், ஆபீஸ்), write it in English letters (meeting, late, office). Keep everything else in Tamil script exactly as spoken, including colloquial forms.",
        OutputMode::Tanglish => "The text is Tamil written in English letters (Tanglish), possibly mixed with English. Keep it in English letters; keep the casual spelling; do not convert it to Tamil script.",
        OutputMode::English => "The text is English, possibly spoken with an Indian accent and with a few Tamil words. Keep Tamil words as they are.",
        OutputMode::Auto => "The text may be Tamil (Tamil script), English, or a mix. Keep each part in the script it is in, except English words that were transcribed in Tamil script — write those in English letters.",
    };
    format!("{RULES}\n\n{style}")
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

#[derive(Deserialize)]
struct Tags {
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
}

fn client(timeout: Duration) -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder().connect_timeout(Duration::from_millis(800)).timeout(timeout).build()?)
}

fn base(cfg: &PolishConfig) -> String {
    cfg.endpoint.trim_end_matches('/').to_string()
}

/// Fixes punctuation and obvious mis-hearings without changing meaning; keeps English words
/// said mid-Tamil in English. Returns the input unchanged on any doubt. Respects `timeout_ms`.
pub async fn polish(cfg: &PolishConfig, text: &str, mode: OutputMode) -> anyhow::Result<String> {
    if text.trim().is_empty() {
        return Ok(text.to_string());
    }
    let timeout = Duration::from_millis(cfg.timeout_ms.max(500) as u64);
    let body = json!({
        "model": cfg.model,
        "stream": false,
        "keep_alive": "10m",
        "think": false,
        "options": { "temperature": 0, "top_p": 1, "num_predict": (text.len() * 2).clamp(64, 2048) },
        "messages": [
            { "role": "system", "content": system_prompt(mode) },
            { "role": "user", "content": text },
        ],
    });
    let resp = client(timeout)?
        .post(format!("{}/api/chat", base(cfg)))
        .json(&body)
        .send()
        .await
        .context("polish model unreachable")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(anyhow!("polish model returned HTTP {status}: {}", detail.chars().take(200).collect::<String>()));
    }
    let out: ChatResponse = resp.json().await.context("polish response")?;
    Ok(accept(text, &out.message.content).unwrap_or_else(|| {
        log::info!("polish output rejected, keeping the transcript");
        text.to_string()
    }))
}

/// Returns the polished text only if it still looks like the same utterance.
fn accept(input: &str, output: &str) -> Option<String> {
    let mut out = strip_think(output).trim().to_string();
    // Unwrap a single pair of quotes the model may add.
    for (a, b) in [('"', '"'), ('“', '”'), ('\'', '\''), ('`', '`')] {
        if out.len() > 2 && out.starts_with(a) && out.ends_with(b) && !input.trim().starts_with(a) {
            out = out[a.len_utf8()..out.len() - b.len_utf8()].trim().to_string();
        }
    }
    if out.is_empty() || (out.contains('\n') && !input.contains('\n')) {
        return None;
    }
    let lower = out.to_lowercase();
    const PREAMBLES: &[&str] = &["here is", "here's", "corrected", "sure", "certainly", "the text", "output:", "i have", "i've", "note:", "translation"];
    let input_lower = input.trim().to_lowercase();
    if PREAMBLES.iter().any(|p| lower.starts_with(p) && !input_lower.starts_with(p)) {
        return None;
    }
    let ratio = out.chars().count() as f32 / input.chars().count().max(1) as f32;
    if !(0.5..=1.6).contains(&ratio) {
        return None;
    }
    Some(out)
}

fn strip_think(s: &str) -> String {
    match (s.find("<think>"), s.find("</think>")) {
        (Some(a), Some(b)) if b > a => format!("{}{}", &s[..a], &s[b + "</think>".len()..]),
        _ => s.to_string(),
    }
}

/// Is the endpoint reachable, and which models does it have?
pub async fn status(cfg: &PolishConfig) -> PolishStatus {
    let res = async {
        let resp = client(Duration::from_secs(3))?.get(format!("{}/api/tags", base(cfg))).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("HTTP {}", resp.status()));
        }
        let tags: Tags = resp.json().await?;
        Ok::<_, anyhow::Error>(tags.models.into_iter().map(|m| m.name).collect::<Vec<_>>())
    }
    .await;
    match res {
        Ok(models) => PolishStatus { reachable: true, models, error: None },
        Err(e) => PolishStatus {
            reachable: false,
            models: Vec::new(),
            error: Some(if e.to_string().contains("connect") || e.is::<reqwest::Error>() {
                format!("Ollama is not running at {} — install it from ollama.com and run `ollama pull {}`", cfg.endpoint, cfg.model)
            } else {
                e.to_string()
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::accept;

    #[test]
    fn accepts_light_fixes() {
        assert_eq!(accept("நான் நாளைக்கு வருவேன்", "நான் நாளைக்கு வருவேன்.").as_deref(), Some("நான் நாளைக்கு வருவேன்."));
        assert_eq!(accept("hello how are you", "\"Hello, how are you?\"").as_deref(), Some("Hello, how are you?"));
        assert_eq!(accept("ok", "<think>hmm</think>OK.").as_deref(), Some("OK."));
    }

    #[test]
    fn rejects_rewrites_and_chatter() {
        assert_eq!(accept("send the file", "Here is the corrected text: Send the file."), None);
        assert_eq!(accept("நான் வருவேன்", "I will come. நான் வருவேன். This means the speaker will arrive soon."), None);
        assert_eq!(accept("one two three four five six seven", "One."), None);
        assert_eq!(accept("hi", "Sure! Hi."), None);
    }
}
