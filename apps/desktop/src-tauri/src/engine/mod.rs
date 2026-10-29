//! Speech engine — microphone capture, whisper.cpp inference, model management,
//! Tamil → Tanglish transliteration and optional local-LLM polish.
//!
//! CONTRACT: the controller (src/controller.rs) depends only on the public items in
//! this file and in `models`, `translit`, `polish`. Keep these signatures stable;
//! add private submodules freely.
//!
//!   start()  ── opens the mic (≈10–40 ms) and, if needed, loads the model on a
//!               background thread; audio is buffered meanwhile.
//!   preview  ── every ~700 ms re-transcribes the current ≤25 s window (cheap pass).
//!   finish() ── stops the mic, aborts the preview, trims silence, normalises and runs
//!               the final pass on the whole recording.

mod capture;
mod clean;
mod dsp;
pub mod models;
pub mod polish;
mod resample;
pub mod translit;
mod whisper;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use parking_lot::{Condvar, Mutex};

use crate::types::{OutputMode, TranscriptResult};
use capture::Recorder;
use dsp::SAMPLE_RATE;
use whisper::{Decode, Lang, Model, Quality};

/// Callbacks from the audio / inference threads. Implementations must be cheap
/// and never block (they forward to Tauri events).
pub trait EngineSink: Send + Sync + 'static {
    /// Normalised input level 0.0–1.0 (perceptual, not raw RMS), ~30×/s while recording.
    fn level(&self, level: f32);
    /// Rolling transcript of everything said so far. Replaces the previous partial.
    fn partial(&self, text: &str);
}

#[derive(Debug, Clone)]
pub struct RecordOptions {
    /// Input device name from [`Engine::input_devices`]; None = system default.
    pub device: Option<String>,
    /// Model id from [`models::catalog`]. Empty = [`models::default_model_id`].
    pub model_id: String,
    pub mode: OutputMode,
    /// Emit `partial` updates while recording.
    pub live_preview: bool,
}

const PREVIEW_EVERY: Duration = Duration::from_millis(700);
const PREVIEW_WINDOW_S: usize = 25;
const MIN_SPEECH_MS: u64 = 300;
const MODEL_WAIT: Duration = Duration::from_secs(180);

/// A model that may still be loading.
type Slot = Arc<(Mutex<Option<Result<Arc<Model>, String>>>, Condvar)>;

fn wait_slot(slot: &Slot, timeout: Duration) -> anyhow::Result<Arc<Model>> {
    let (lock, cv) = &**slot;
    let mut guard = lock.lock();
    let deadline = Instant::now() + timeout;
    while guard.is_none() {
        if cv.wait_until(&mut guard, deadline).timed_out() {
            return Err(anyhow!("speech model is taking too long to load"));
        }
    }
    guard.as_ref().unwrap().clone().map_err(|e| anyhow!(e))
}

fn peek_slot(slot: &Slot) -> Option<Arc<Model>> {
    slot.0.lock().as_ref().and_then(|r| r.as_ref().ok().cloned())
}

struct Preview {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl Preview {
    fn shutdown(self) {
        self.stop.store(true, Ordering::SeqCst);
        self.handle.thread().unpark();
        let _ = self.handle.join();
    }
}

struct Session {
    recorder: Recorder,
    opts: RecordOptions,
    slot: Slot,
    preview: Option<Preview>,
}

struct Inner {
    models_dir: PathBuf,
    /// The loaded model (at most one in memory).
    model: Mutex<Option<Arc<Model>>>,
    /// Serialises model loads.
    loading: Mutex<()>,
    session: Mutex<Option<Session>>,
}

/// Cheap to clone; all clones share one recorder and one loaded model.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

impl Engine {
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(Inner {
                models_dir,
                model: Mutex::new(None),
                loading: Mutex::new(()),
                session: Mutex::new(None),
            }),
        }
    }

    pub fn models_dir(&self) -> PathBuf {
        self.inner.models_dir.clone()
    }

    /// Names of available input devices, default first.
    pub fn input_devices() -> Vec<String> {
        capture::input_devices()
    }

    /// Opens the mic and starts buffering 16 kHz mono audio. Must return within ~50 ms
    /// (the model may still be loading — buffer audio meanwhile). Errors if the mic is
    /// unavailable or the model file is missing.
    pub fn start(&self, opts: RecordOptions, sink: Arc<dyn EngineSink>) -> anyhow::Result<()> {
        self.cancel_now();
        let model_id = if opts.model_id.is_empty() { models::default_model_id().to_string() } else { opts.model_id.clone() };
        if models::resolve(&self.inner.models_dir, &model_id).is_none() {
            return Err(anyhow!("speech model {model_id} is not downloaded"));
        }
        let opts = RecordOptions { model_id: model_id.clone(), ..opts };

        let t = Instant::now();
        let recorder = Recorder::start(opts.device.as_deref(), sink.clone())?;
        log::debug!("mic open in {:?}", t.elapsed());

        let slot: Slot = Arc::new((Mutex::new(None), Condvar::new()));
        match self.loaded(&model_id) {
            Some(m) => *slot.0.lock() = Some(Ok(m)),
            None => {
                let me = self.clone();
                let s = slot.clone();
                thread::Builder::new()
                    .name("mozhi-load".into())
                    .spawn(move || {
                        let r = me.load(&model_id).map_err(|e| format!("{e:#}"));
                        *s.0.lock() = Some(r);
                        s.1.notify_all();
                    })
                    .map_err(|e| anyhow!("spawn model loader: {e}"))?;
            }
        }

        let preview = if opts.live_preview {
            Some(spawn_preview(recorder.shared(), slot.clone(), opts.mode, sink))
        } else {
            None
        };
        *self.inner.session.lock() = Some(Session { recorder, opts, slot, preview });
        Ok(())
    }

    /// Stops capture and runs the final transcription. Blocking — call from a worker thread.
    pub fn finish(&self) -> anyhow::Result<TranscriptResult> {
        let session = self.inner.session.lock().take().ok_or_else(|| anyhow!("not recording"))?;
        if let Some(p) = session.preview {
            p.shutdown();
        }
        let audio = session.recorder.stop();
        if dsp::is_digital_silence(&audio) && audio.len() > SAMPLE_RATE * 4 / 10 {
            return Err(anyhow!(
                "microphone permission denied — Mozhi received only silence. Allow it in System Settings → Privacy & Security → Microphone"
            ));
        }
        let slot = session.slot;
        run_final(audio, session.opts.mode, Quality::Final, || wait_slot(&slot, MODEL_WAIT))
    }

    /// Transcribes recorded audio (any sample rate, mono) exactly like a dictation would —
    /// trim, normalise, final pass, clean-up, mode conversion. Blocking. Used by
    /// `examples/transcribe.rs`; `preview` runs the cheaper live-preview pass instead.
    #[allow(dead_code)]
    pub fn transcribe_samples(&self, samples: &[f32], sample_rate: u32, mode: OutputMode, model_id: &str, preview: bool) -> anyhow::Result<TranscriptResult> {
        let mut audio = Vec::with_capacity(samples.len() * SAMPLE_RATE / sample_rate.max(1) as usize + 64);
        let mut r = resample::Resampler::new(sample_rate, SAMPLE_RATE as u32);
        r.process(samples, &mut audio);
        r.flush(&mut audio);
        let quality = if preview { Quality::Preview } else { Quality::Final };
        let id = if model_id.is_empty() { models::default_model_id() } else { model_id };
        run_final(audio, mode, quality, || self.load(id))
    }

    /// Stops capture and discards audio. Safe to call at any time.
    pub fn cancel(&self) {
        if let Some(session) = self.inner.session.lock().take() {
            // Joining the audio threads takes a few tens of ms; keep the caller snappy.
            thread::spawn(move || teardown(session));
        }
    }

    #[allow(dead_code)]
    pub fn is_recording(&self) -> bool {
        self.inner.session.lock().is_some()
    }

    /// Loads the model into memory so the first dictation starts instantly. Blocking.
    pub fn preload(&self, model_id: &str) -> anyhow::Result<()> {
        let id = if model_id.is_empty() { models::default_model_id() } else { model_id };
        self.load(id).map(|_| ())
    }

    // ── internals ──────────────────────────────────────────────────────────

    fn cancel_now(&self) {
        if let Some(session) = self.inner.session.lock().take() {
            teardown(session);
        }
    }

    fn loaded(&self, id: &str) -> Option<Arc<Model>> {
        self.inner.model.lock().as_ref().filter(|m| m.id == id).cloned()
    }

    fn load(&self, id: &str) -> anyhow::Result<Arc<Model>> {
        let _serial = self.inner.loading.lock();
        if let Some(m) = self.loaded(id) {
            return Ok(m);
        }
        let (path, kind) = models::resolve(&self.inner.models_dir, id).ok_or_else(|| anyhow!("speech model {id} is not downloaded"))?;
        // Free the previous model first — two large models may not fit in memory.
        *self.inner.model.lock() = None;
        let t = Instant::now();
        let model = Arc::new(Model::load(id, &path, kind)?);
        if let Err(e) = model.warm_up() {
            log::warn!("model warm-up: {e:#}");
        }
        log::info!("loaded {id} in {:?}", t.elapsed());
        *self.inner.model.lock() = Some(model.clone());
        Ok(model)
    }
}

/// Shared tail of every transcription: find speech, normalise, decode, clean, convert.
fn run_final(audio: Vec<f32>, mode: OutputMode, quality: Quality, model: impl FnOnce() -> anyhow::Result<Arc<Model>>) -> anyhow::Result<TranscriptResult> {
    let audio_ms = (audio.len() * 1000 / SAMPLE_RATE) as u64;
    let speech = dsp::find_speech(&audio);
    if speech.voiced_ms < MIN_SPEECH_MS {
        return Ok(TranscriptResult { text: String::new(), raw: String::new(), language: String::new(), audio_ms, inference_ms: 0 });
    }
    let mut clip = audio[speech.start..speech.end].to_vec();
    dsp::normalize(&mut clip);

    let model = model()?;
    let t = Instant::now();
    let (raw, lang) = model.transcribe(&clip, &Decode { lang: lang_for(mode), quality, previous: None, abort: None })?;
    let inference_ms = t.elapsed().as_millis() as u64;
    let cleaned = clean::clean(&raw, speech.voiced_ms);
    let text = finalize(&cleaned, mode);
    log::info!(
        "transcribed {:.1}s ({} ms voiced) in {inference_ms} ms with {} [{lang}]",
        audio_ms as f32 / 1000.0,
        speech.voiced_ms,
        model.id
    );
    Ok(TranscriptResult { text, raw: raw.trim().to_string(), language: lang.into(), audio_ms, inference_ms })
}

fn teardown(session: Session) {
    if let Some(p) = session.preview {
        p.shutdown();
    }
    drop(session.recorder.stop());
}

fn lang_for(mode: OutputMode) -> Lang {
    match mode {
        OutputMode::Tamil | OutputMode::Tanglish => Lang::Tamil,
        OutputMode::English => Lang::English,
        OutputMode::Auto => Lang::Auto,
    }
}

/// Whisper language code for a mode (see `Model::transcribe` for why Auto decodes as Tamil).
fn decode_lang(mode: OutputMode) -> &'static str {
    match mode {
        OutputMode::English => "en",
        _ => "ta",
    }
}

fn finalize(text: &str, mode: OutputMode) -> String {
    match mode {
        OutputMode::Tanglish => translit::tamil_to_tanglish(text),
        _ => text.to_string(),
    }
}

/// Live preview: transcribes the current window every ~700 ms. Long dictations are
/// handled in fixed 25 s windows; finished windows are frozen and prefixed.
fn spawn_preview(audio: Arc<parking_lot::RwLock<Vec<f32>>>, slot: Slot, mode: OutputMode, sink: Arc<dyn EngineSink>) -> Preview {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_t = stop.clone();
    let handle = thread::Builder::new()
        .name("mozhi-preview".into())
        .spawn(move || {
            let window = PREVIEW_WINDOW_S * SAMPLE_RATE;
            let mut frozen = String::new();
            let mut window_start = 0usize;
            let mut last_len = 0usize;
            let mut last_emitted = String::new();
            loop {
                thread::park_timeout(PREVIEW_EVERY);
                if stop_t.load(Ordering::SeqCst) {
                    return;
                }
                let Some(model) = peek_slot(&slot) else { continue };
                let len = audio.read().len();
                if len < SAMPLE_RATE * 6 / 10 || len == last_len {
                    continue;
                }
                last_len = len;

                // Close a full window: transcribe it once more and freeze the text.
                while len - window_start > window {
                    let chunk = audio.read()[window_start..window_start + window].to_vec();
                    let l = decode_lang(mode);
                    let opts = Decode { lang: lang_for(mode), quality: Quality::Preview, previous: Some(&frozen), abort: Some(stop_t.clone()) };
                    if let Ok(t) = model.transcribe_chunk(&chunk, l, &opts) {
                        let t = clean::clean(&t, 5_000);
                        if !t.is_empty() {
                            if !frozen.is_empty() {
                                frozen.push(' ');
                            }
                            frozen.push_str(&t);
                        }
                    }
                    window_start += window;
                    if stop_t.load(Ordering::SeqCst) {
                        return;
                    }
                }

                let mut chunk = audio.read()[window_start..].to_vec();
                let speech = dsp::find_speech(&chunk);
                if speech.voiced_ms < 200 {
                    continue;
                }
                dsp::normalize(&mut chunk);

                let l = decode_lang(mode);
                let opts = Decode { lang: lang_for(mode), quality: Quality::Preview, previous: if frozen.is_empty() { None } else { Some(&frozen) }, abort: Some(stop_t.clone()) };
                let Ok(text) = model.transcribe_chunk(&chunk, l, &opts) else { continue };
                if stop_t.load(Ordering::SeqCst) {
                    return;
                }
                let text = clean::clean(&text, speech.voiced_ms);
                let full = match (frozen.is_empty(), text.is_empty()) {
                    (true, _) => text,
                    (false, true) => frozen.clone(),
                    (false, false) => format!("{frozen} {text}"),
                };
                let shown = finalize(&full, mode);
                if shown != last_emitted {
                    sink.partial(&shown);
                    last_emitted = shown;
                }
            }
        })
        .expect("spawn preview thread");
    Preview { stop, handle }
}
