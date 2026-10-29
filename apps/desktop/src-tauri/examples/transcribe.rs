//! Transcribe WAV files with Mozhi's engine — the same trim / decode / clean-up path as a
//! real dictation. Handy for checking accuracy and speed of a model on your machine.
//!
//!   cargo run --release --example transcribe -- <file.wav>… [--model large-v3-turbo-q5]
//!       [--mode auto|tamil|tanglish|english] [--preview] [--models-dir DIR]
//!
//! Models are read from the app's models folder (download them in Mozhi → Models).
#![allow(dead_code)]

#[path = "../src/types.rs"]
mod types;
#[path = "../src/engine/mod.rs"]
mod engine;

use std::path::PathBuf;
use std::time::Instant;

use types::OutputMode;

fn default_models_dir() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default();
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/app.mozhi.desktop/models")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or(home).join("app.mozhi.desktop/models")
    } else {
        home.join(".local/share/app.mozhi.desktop/models")
    }
}

fn read_wav(path: &str) -> anyhow::Result<(Vec<f32>, u32)> {
    let mut r = hound::WavReader::open(path)?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect::<Result<_, _>>()?
        }
    };
    let mono = interleaved.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
    Ok((mono, spec.sample_rate))
}

/// `MOZHI_DEBUG=1` prints engine and whisper.cpp logs to stderr.
struct StderrLog;

impl log::Log for StderrLog {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        eprintln!("[{} {}] {}", r.level(), r.target(), r.args());
    }
    fn flush(&self) {}
}

static LOGGER: StderrLog = StderrLog;

struct Printer {
    levels: std::sync::atomic::AtomicUsize,
    peak: std::sync::Mutex<f32>,
}

impl engine::EngineSink for Printer {
    fn level(&self, level: f32) {
        self.levels.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut p = self.peak.lock().unwrap();
        *p = p.max(level);
    }
    fn partial(&self, text: &str) {
        println!("   … {text}");
    }
}

/// Live microphone run through the real start → preview → finish path.
fn record(engine: &engine::Engine, id: &str, mode: OutputMode, secs: f32) -> anyhow::Result<()> {
    let sink = std::sync::Arc::new(Printer { levels: Default::default(), peak: Default::default() });
    let opts = engine::RecordOptions { device: None, model_id: id.into(), mode, live_preview: true };
    let t = Instant::now();
    engine.start(opts, sink.clone())?;
    println!("start() returned in {:.1} ms — speak for {secs}s", t.elapsed().as_secs_f32() * 1000.0);
    std::thread::sleep(std::time::Duration::from_secs_f32(secs));
    let t = Instant::now();
    let r = engine.finish();
    println!("finish() in {} ms · {} level events · peak level {:.2}", t.elapsed().as_millis(), sink.levels.load(std::sync::atomic::Ordering::Relaxed), sink.peak.lock().unwrap());
    let r = r?;
    println!("   lang {} · audio {} ms · inference {} ms\n   text: {}", r.language, r.audio_ms, r.inference_ms, r.text);
    Ok(())
}

fn main() -> anyhow::Result<()> {
    if std::env::var_os("MOZHI_DEBUG").is_some() {
        let _ = log::set_logger(&LOGGER).map(|_| log::set_max_level(log::LevelFilter::Debug));
    }
    let mut files = Vec::new();
    let mut model = String::new();
    let mut mode = OutputMode::Auto;
    let mut preview = false;
    let mut mic_secs: Option<f32> = None;
    let mut dir = default_models_dir();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--model" => model = args.next().unwrap_or_default(),
            "--mode" => {
                mode = match args.next().as_deref() {
                    Some("tamil") => OutputMode::Tamil,
                    Some("tanglish") => OutputMode::Tanglish,
                    Some("english") => OutputMode::English,
                    _ => OutputMode::Auto,
                }
            }
            "--preview" => preview = true,
            "--mic" => mic_secs = args.next().and_then(|s| s.parse::<f32>().ok()),
            "--models-dir" => dir = PathBuf::from(args.next().unwrap_or_default()),
            _ => files.push(a),
        }
    }
    if files.is_empty() && mic_secs.is_none() {
        eprintln!("usage: transcribe <file.wav>… [--model ID] [--mode auto|tamil|tanglish|english] [--preview]");
        eprintln!("models in {}:", dir.display());
        for m in engine::models::catalog(&dir) {
            eprintln!("  {:<22} {:>5} MB  {}", m.id, m.size_mb, if m.installed { "installed" } else { "" });
        }
        std::process::exit(2);
    }

    let engine = engine::Engine::new(dir);
    let id = if model.is_empty() { engine::models::default_model_id().to_string() } else { model };
    let t = Instant::now();
    engine.preload(&id)?;
    println!("model {id}: loaded + warmed in {:.2}s", t.elapsed().as_secs_f32());

    if let Some(secs) = mic_secs {
        return record(&engine, &id, mode, secs);
    }

    for f in files {
        let (samples, rate) = read_wav(&f)?;
        let t = Instant::now();
        let r = engine.transcribe_samples(&samples, rate, mode, &id, preview)?;
        let wall = t.elapsed().as_millis();
        let audio_s = samples.len() as f32 / rate as f32;
        println!("\n── {f}");
        println!("   audio {audio_s:.1}s · lang {} · inference {} ms · total {wall} ms · RTF {:.3}", r.language, r.inference_ms, wall as f32 / 1000.0 / audio_s);
        println!("   raw : {}", r.raw);
        println!("   text: {}", r.text);
    }
    Ok(())
}
