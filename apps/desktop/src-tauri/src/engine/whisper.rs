//! whisper.cpp inference via whisper-rs.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};

use anyhow::anyhow;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

use super::clean;
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

/// Whisper continues in the style of its prompt. The Tamil prompt shows both kinds of
/// code-switching — an English word inside Tamil, and a whole English sentence — so English
/// stays in Latin script instead of being transliterated (or hallucinated into another
/// language). Avoid short interjections (சரி, ஓகே): Whisper copies them and can loop on them.
/// Chosen on FLEURS Tamil + English test clips; see the engine notes in docs.
const PROMPT_TA: &str = "வணக்கம், இன்னைக்கு meeting-க்கு கொஞ்சம் late ஆயிடுச்சு. Sorry, I will call you back after lunch.";
/// Must not resemble anything a user is likely to dictate: Whisper skips speech that
/// repeats its prompt (an earlier "I'll send the report by tomorrow" prompt swallowed a
/// dictated "I will send the quarterly report by tomorrow evening").
const PROMPT_EN: &str = "Okay. Yes, that works, thanks!";

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
        Ok(Self { id: id.to_string(), kind, ctx })
    }

    /// Every decode gets a fresh state. Reusing a whisper.cpp state across calls with a
    /// different `audio_ctx` corrupts later results (measured: Tamil CER 13% → 48% after a
    /// few clips), and allocating one costs only tens of milliseconds.
    fn with_state<R>(&self, f: impl FnOnce(&mut WhisperState) -> anyhow::Result<R>) -> anyhow::Result<R> {
        let mut state = self.ctx.create_state().map_err(|e| anyhow!("speech model state: {e:?}"))?;
        f(&mut state)
    }

    /// Runs one tiny inference so Metal compiles its kernels now, not on the first dictation.
    pub fn warm_up(&self) -> anyhow::Result<()> {
        let noise: Vec<f32> = (0..SAMPLE_RATE).map(|i| ((i * 7919) % 97) as f32 / 97.0 * 0.002 - 0.001).collect();
        let lang = if self.kind == ModelKind::TamilOnly { "ta" } else { "en" };
        self.transcribe_chunk(&noise, lang, &Decode { lang: Lang::English, quality: Quality::Preview, previous: None, abort: None })
            .map(|_| ())
    }

    /// Tamil or English? Whisper's language ID restricted to those two. Needs a full 30 s
    /// encoder pass, so the engine runs it in the background while the user is still speaking.
    /// Leans to Tamil: Tanglish full of English words must still decode as Tamil.
    /// Returns (language, confident).
    pub fn identify(&self, audio: &[f32]) -> anyhow::Result<(&'static str, bool)> {
        if self.kind == ModelKind::TamilOnly {
            return Ok(("ta", true));
        }
        let n = audio.len().min(SAMPLE_RATE * 30);
        let probs = self.with_state(|state| {
            state.pcm_to_mel(&audio[..n], threads() as usize).map_err(|e| anyhow!("mel: {e:?}"))?;
            let (_, probs) = state.lang_detect(0, threads() as usize).map_err(|e| anyhow!("language detect: {e:?}"))?;
            Ok(probs)
        })?;
        let ta = whisper_rs::get_lang_id("ta").map(|i| probs[i as usize]).unwrap_or(0.0);
        let en = whisper_rs::get_lang_id("en").map(|i| probs[i as usize]).unwrap_or(0.0);
        log::debug!("language id: ta {ta:.3} · en {en:.3}");
        let lang = if en > 0.7 && en > ta * 4.0 { "en" } else { "ta" };
        Ok((lang, en > 0.85 || ta > 0.6))
    }

    /// Transcribes ≤30 s of 16 kHz mono audio.
    pub fn transcribe_chunk(&self, audio: &[f32], lang: &str, opts: &Decode) -> anyhow::Result<String> {
        if audio.len() < SAMPLE_RATE / 10 {
            return Ok(String::new());
        }
        let lang = if self.kind == ModelKind::TamilOnly { "ta" } else { lang };
        let prompt = match opts.previous {
            Some(prev) if !prev.trim().is_empty() => tail_chars(prev, 160),
            _ => (if lang == "en" { PROMPT_EN } else { PROMPT_TA }).to_string(),
        };

        // Plain greedy decoding. whisper.cpp's temperature fallback re-decodes up to five times
        // and fired on clean speech in testing (≈4× slower, no accuracy gain on Tamil FLEURS).
        let text = self.run(self.params(audio, lang, &prompt, opts, 0.0, 1), audio, opts)?;
        if opts.quality == Quality::Final && clean::has_loop(&text) {
            // Greedy got stuck repeating itself. One retry with a little sampling almost
            // always escapes the loop, and only costs time when it happens.
            log::info!("decoder loop detected, retrying with sampling");
            if let Ok(again) = self.run(self.params(audio, lang, &prompt, opts, 0.3, 3), audio, opts) {
                if !clean::has_loop(&again) {
                    return Ok(again);
                }
            }
        }
        Ok(text)
    }

    fn params<'a>(&self, audio: &[f32], lang: &'a str, prompt: &str, opts: &Decode, temperature: f32, best_of: i32) -> FullParams<'a, 'static> {
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of });
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
        p.set_temperature(temperature);
        p.set_temperature_inc(0.0);
        // A loop can otherwise emit hundreds of tokens (seconds of GPU time). Tamil runs at
        // roughly 10–20 tokens per second of speech; these caps leave ample headroom.
        let secs = audio.len() as f32 / SAMPLE_RATE as f32;
        let cap = match opts.quality {
            Quality::Final => secs * 40.0 + 64.0,
            Quality::Preview => secs * 30.0 + 32.0,
        };
        p.set_max_tokens(cap.min(440.0) as i32);
        if self.kind == ModelKind::Multilingual {
            // Encode only as much context as there is audio (+ padding) instead of a fixed 30 s:
            // ≈4× faster encoder for short dictation, identical Tamil CER on FLEURS. The Tamil
            // fine-tunes were trained on full windows and stop early when the context is cut,
            // so they always get the full 30 s.
            let pad = match opts.quality {
                Quality::Final => 2.0,
                Quality::Preview => 1.0,
            };
            let ctx = ((audio.len() as f32 / SAMPLE_RATE as f32 + pad) * 50.0).ceil() as i32;
            p.set_audio_ctx(ctx.clamp(256, 1500));
            p.set_initial_prompt(prompt);
        }
        if let Some(flag) = &opts.abort {
            // whisper-rs 0.16's `set_abort_callback_safe` casts its boxed `dyn FnMut` to the
            // concrete closure type, so the callback reads garbage and aborts encodes at random
            // (whisper error -6). Use the raw hook with a pointer to the flag instead; the Arc
            // in `opts` outlives the `full()` call.
            unsafe {
                p.set_abort_callback(Some(abort_when_set));
                p.set_abort_callback_user_data(Arc::as_ptr(flag) as *mut std::ffi::c_void);
            }
        }
        p
    }

    fn run(&self, p: FullParams, audio: &[f32], opts: &Decode) -> anyhow::Result<String> {
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

unsafe extern "C" fn abort_when_set(flag: *mut std::ffi::c_void) -> bool {
    !flag.is_null() && (*(flag as *const AtomicBool)).load(Ordering::Relaxed)
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
