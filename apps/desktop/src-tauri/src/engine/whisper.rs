//! whisper.cpp inference via whisper-rs.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};

use anyhow::anyhow;
use parking_lot::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

use super::dsp::{self, SAMPLE_RATE};

/// Which languages a model can speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Multilingual,
    /// Fine-tuned on Tamil only (e.g. vasista22/whisper-tamil-*): always decode as "ta", no prompt.
    TamilOnly,
}

pub struct Model {
    pub id: String,
    pub kind: ModelKind,
    ctx: WhisperContext,
    /// Decoder states are expensive to allocate (GPU buffers); keep a couple warm.
    states: Mutex<Vec<WhisperState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Tamil,
    English,
    /// Tamil, English or both in one utterance.
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// Final transcript.
    Final,
    /// Live preview: tighter audio context — fastest, a little rougher at the edges.
    Preview,
}

pub struct Decode<'a> {
    pub lang: Lang,
    pub quality: Quality,
    /// Text that came just before this audio (previous chunk), used as the prompt.
    pub previous: Option<&'a str>,
    pub abort: Option<Arc<AtomicBool>>,
}

/// Short, natural sentences in the target style. Whisper continues in the style of its prompt,
/// so these bias it toward Tamil punctuation and toward keeping English words in Latin script.
const PROMPT_TA: &str = "வணக்கம், இன்னைக்கு meeting-க்கு கொஞ்சம் late ஆயிடுச்சு. சரி, நாளைக்கு பார்க்கலாம்.";
const PROMPT_EN: &str = "Hello, how are you? I'll send the report by tomorrow.";

static LOGGING: Once = Once::new();

pub fn threads() -> i32 {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8) as i32
}

impl Model {
    pub fn load(id: &str, path: &Path, kind: ModelKind) -> anyhow::Result<Self> {
        LOGGING.call_once(whisper_rs::install_logging_hooks);
        if !path.exists() {
            return Err(anyhow!("speech model {id} is not downloaded"));
        }
        let mut params = WhisperContextParameters::default();
        params.use_gpu(cfg!(target_os = "macos"));
        params.flash_attn(true);
        let ctx = WhisperContext::new_with_params(path, params).map_err(|e| anyhow!("speech model {id} failed to load: {e:?}"))?;
        Ok(Self { id: id.to_string(), kind, ctx, states: Mutex::new(Vec::new()) })
    }

    fn with_state<R>(&self, f: impl FnOnce(&mut WhisperState) -> anyhow::Result<R>) -> anyhow::Result<R> {
        let popped = self.states.lock().pop();
        let mut state = match popped {
            Some(s) => s,
            None => self.ctx.create_state().map_err(|e| anyhow!("speech model state: {e:?}"))?,
        };
        let r = f(&mut state);
        let mut pool = self.states.lock();
        if pool.len() < 2 && std::env::var_os("MOZHI_EXP_NOPOOL").is_none() {
            pool.push(state);
        }
        r
    }

    /// Runs one tiny inference so Metal compiles its kernels now, not on the first dictation.
    pub fn warm_up(&self) -> anyhow::Result<()> {
        let noise: Vec<f32> = (0..SAMPLE_RATE).map(|i| ((i * 7919) % 97) as f32 / 97.0 * 0.002 - 0.001).collect();
        let lang = if self.kind == ModelKind::TamilOnly { "ta" } else { "en" };
        self.transcribe_chunk(&noise, lang, &Decode { lang: Lang::English, quality: Quality::Preview, previous: None, abort: None })
            .map(|_| ())
    }

    /// Transcribes ≤30 s of 16 kHz mono audio. Returns (text, language).
    pub fn transcribe_chunk(&self, audio: &[f32], lang: &str, opts: &Decode) -> anyhow::Result<String> {
        if audio.len() < SAMPLE_RATE / 10 {
            return Ok(String::new());
        }
        let lang = if self.kind == ModelKind::TamilOnly { "ta" } else { lang };
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        p.set_n_threads(threads());
        p.set_translate(false);
        p.set_language(Some(lang));
        p.set_no_context(true);
        p.set_no_timestamps(true);
        p.set_single_segment(false);
        p.set_print_special(false);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_timestamps(false);
        p.set_suppress_blank(true);
        p.set_suppress_nst(true);
        p.set_no_speech_thold(0.6);
        // Plain greedy decoding. whisper.cpp's temperature fallback re-decodes up to five times
        // and fired on clean speech in testing (≈4× slower, no accuracy gain on Tamil FLEURS);
        // runaway repetition is handled in clean.rs instead.
        p.set_temperature(0.0);
        // EXPERIMENT-BEGIN
        p.set_temperature_inc(std::env::var("MOZHI_EXP_TINC").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0));
        if let Some(v) = std::env::var("MOZHI_EXP_LP").ok().and_then(|v| v.parse::<f32>().ok()) { p.set_logprob_thold(v); }
        if std::env::var_os("MOZHI_EXP_BEAM").is_some() {
            let mut b = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 });
            b.set_n_threads(threads()); b.set_language(Some(lang)); b.set_no_context(true); b.set_no_timestamps(true);
            b.set_print_special(false); b.set_print_progress(false); b.set_print_realtime(false); b.set_print_timestamps(false);
            b.set_suppress_blank(true); b.set_suppress_nst(true); b.set_temperature(0.0); b.set_temperature_inc(0.0);
            if self.kind == ModelKind::Multilingual { let ctx = ((audio.len() as f32 / SAMPLE_RATE as f32 + 2.0) * 50.0).ceil() as i32; b.set_audio_ctx(ctx.clamp(256, 1500)); b.set_initial_prompt(if lang == "en" { PROMPT_EN } else { PROMPT_TA }); }
            p = b;
        }
        // EXPERIMENT-END
        // Encode only as much context as there is audio (+ padding) instead of a fixed 30 s:
        // ≈4× faster encoder for short dictation, identical Tamil CER on FLEURS for the stock
        // multilingual models. The Tamil fine-tunes were trained on full windows and stop early
        // when the context is cut, so they always get the full 30 s.
        if self.kind == ModelKind::Multilingual {
            let pad = match opts.quality {
                Quality::Final => 2.0,
                Quality::Preview => 1.0,
            };
            let ctx = ((audio.len() as f32 / SAMPLE_RATE as f32 + pad) * 50.0).ceil() as i32;
            p.set_audio_ctx(ctx.clamp(256, 1500));
        }
        if self.kind == ModelKind::Multilingual {
            let exp = std::env::var("MOZHI_EXP_PROMPT").ok();
            let base: &str = if lang == "en" { PROMPT_EN } else { exp.as_deref().unwrap_or(PROMPT_TA) };
            let prompt = match opts.previous {
                Some(prev) if !prev.trim().is_empty() => tail_chars(prev, 160),
                _ => base.to_string(),
            };
            p.set_initial_prompt(&prompt);
        }
        if let Some(flag) = opts.abort.clone() {
            p.set_abort_callback_safe(move || flag.load(Ordering::Relaxed));
        }

        let aborted = || opts.abort.as_ref().map(|f| f.load(Ordering::Relaxed)).unwrap_or(false);
        self.with_state(|state| {
            state.full(p, audio).map_err(|e| anyhow!("whisper inference failed: {e:?}"))?;
            if aborted() {
                return Err(anyhow!("aborted"));
            }
            let mut text = String::new();
            for seg in state.as_iter() {
                if seg.no_speech_probability() > 0.85 && seg.n_tokens() < 4 {
                    continue;
                }
                if let Ok(s) = seg.to_str_lossy() {
                    text.push_str(&s);
                }
            }
            Ok(text)
        })
    }

    /// Full transcription of arbitrarily long audio: chunked at quiet points, each chunk
    /// prompted with the end of the previous one. Returns (text, language).
    pub fn transcribe(&self, audio: &[f32], opts: &Decode) -> anyhow::Result<(String, &'static str)> {
        // Auto decodes as Tamil: with the code-switch prompt Whisper writes Tamil speech in
        // Tamil script and English speech/words in Latin, in one pass. (A separate
        // language-ID pass needs a full 30 s encode — seconds — and is unreliable on mixed speech.)
        let lang = match (self.kind, opts.lang) {
            (_, Lang::English) if self.kind == ModelKind::Multilingual => "en",
            _ => "ta",
        };
        let mut out = String::new();
        for (s, e) in dsp::split_points(audio, 18.0, 28.0) {
            let prev = if out.is_empty() { None } else { Some(out.as_str()) };
            let piece = self.transcribe_chunk(&audio[s..e], lang, &Decode { previous: prev, abort: opts.abort.clone(), ..*opts })?;
            let piece = piece.trim();
            if !piece.is_empty() {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(piece);
            }
        }
        let label = if opts.lang == Lang::Auto { script_language(&out) } else { lang };
        Ok((out, label))
    }
}

/// "ta" when at least a quarter of the letters are Tamil, else "en".
pub fn script_language(text: &str) -> &'static str {
    let (mut tamil, mut letters) = (0usize, 0usize);
    for c in text.chars().filter(|c| c.is_alphabetic()) {
        letters += 1;
        if ('\u{0B80}'..='\u{0BFF}').contains(&c) {
            tamil += 1;
        }
    }
    if letters > 0 && tamil * 4 < letters {
        "en"
    } else {
        "ta"
    }
}

fn tail_chars(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    chars[chars.len().saturating_sub(n)..].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::script_language;

    #[test]
    fn labels_by_script() {
        assert_eq!(script_language("நான் நாளைக்கு வருவேன்"), "ta");
        assert_eq!(script_language("இன்னைக்கு meeting-க்கு late ஆயிடுச்சு"), "ta");
        assert_eq!(script_language("Hi Priya, I will send the report tomorrow."), "en");
        assert_eq!(script_language(""), "ta");
    }
}
