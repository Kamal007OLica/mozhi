//! Microphone capture.
//!
//!   cpal callback ──(downmix to mono, lock-free SPSC ring)──▶ pump thread
//!   pump thread   ──(sinc resample → 16 kHz, level meter)──▶ shared buffer + EngineSink::level
//!
//! The cpal stream lives on its own thread (streams are not `Send` on every platform)
//! and is dropped there when recording stops.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use parking_lot::{Mutex, RwLock};

use super::dsp::{LevelMeter, SAMPLE_RATE};
use super::resample::Resampler;
use super::EngineSink;

pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let default = host.default_input_device().and_then(|d| d.name().ok());
    let mut names: Vec<String> = host
        .input_devices()
        .map(|it| it.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default();
    names.dedup();
    if let Some(def) = default {
        names.retain(|n| *n != def);
        names.insert(0, def);
    }
    names
}

pub struct Recorder {
    stop: Arc<AtomicBool>,
    /// Signalled by the mic thread once it has drained and released the device.
    done: Option<mpsc::Receiver<()>>,
    audio: Arc<RwLock<Vec<f32>>>,
    dropped: Arc<AtomicU64>,
    /// A device error that happened after `start` returned.
    error: Arc<Mutex<Option<String>>>,
}

/// Longest `start` blocks waiting for the device. Opening CoreAudio takes ~200 ms on a
/// MacBook, and a pending permission prompt blocks indefinitely; the overlay must not wait
/// for either. Fast failures (no device at all) still surface synchronously.
const OPEN_WAIT: Duration = Duration::from_millis(40);

impl Recorder {
    /// Starts opening the device on the `mozhi-mic` thread, which then streams, resamples and
    /// meters until stopped. Returns within ~40 ms.
    pub fn start(device: Option<&str>, sink: Arc<dyn EngineSink>) -> anyhow::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));
        let audio = Arc::new(RwLock::new(Vec::with_capacity(SAMPLE_RATE * 60)));
        let error = Arc::new(Mutex::new(None));
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);
        let (done_tx, done_rx) = mpsc::sync_channel::<()>(1);

        let device_name = device.map(str::to_owned);
        let (stop_t, dropped_t, audio_t, error_t) = (stop.clone(), dropped.clone(), audio.clone(), error.clone());
        thread::Builder::new()
            .name("mozhi-mic".into())
            .spawn(move || {
                let opened = open_stream(device_name.as_deref(), dropped_t).and_then(|(stream, rate, consumer)| {
                    stream.play().map_err(|e| anyhow!("microphone device could not start: {e}"))?;
                    Ok((stream, rate, consumer))
                });
                match opened {
                    Ok((stream, rate, consumer)) => {
                        let _ = ready_tx.try_send(Ok(()));
                        pump(rate, consumer, &stop_t, &audio_t, sink.as_ref());
                        drop(stream);
                    }
                    Err(e) => {
                        let msg = format!("{e:#}");
                        log::error!("{msg}");
                        *error_t.lock() = Some(msg.clone());
                        let _ = ready_tx.try_send(Err(msg));
                    }
                }
                let _ = done_tx.try_send(());
            })
            .context("spawn microphone thread")?;

        if let Ok(Err(e)) = ready_rx.recv_timeout(OPEN_WAIT) {
            stop.store(true, Ordering::Release);
            return Err(anyhow!(e));
        }
        Ok(Self { stop, done: Some(done_rx), audio, dropped, error })
    }

    /// The growing 16 kHz mono buffer, for the live preview.
    pub fn shared(&self) -> Arc<RwLock<Vec<f32>>> {
        self.audio.clone()
    }

    /// Stops the stream, drains the ring and returns everything recorded — or the device
    /// error if the microphone never opened.
    pub fn stop(mut self) -> anyhow::Result<Vec<f32>> {
        self.halt();
        let dropped = self.dropped.load(Ordering::Relaxed);
        if dropped > 0 {
            log::warn!("audio ring overflowed, dropped {dropped} samples");
        }
        let audio = std::mem::take(&mut *self.audio.write());
        match self.error.lock().take() {
            Some(e) if audio.is_empty() => Err(anyhow!(e)),
            _ => Ok(audio),
        }
    }

    fn halt(&mut self) {
        if let Some(done) = self.done.take() {
            // Let the last callback land before the final drain.
            thread::sleep(Duration::from_millis(30));
            self.stop.store(true, Ordering::Release);
            // A device stuck behind a permission prompt never finishes; don't hang on it.
            let _ = done.recv_timeout(Duration::from_secs(1));
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

/// Moves audio from the real-time ring to the shared 16 kHz buffer every 10 ms.
fn pump(rate: u32, mut consumer: rtrb::Consumer<f32>, stop: &AtomicBool, audio: &RwLock<Vec<f32>>, sink: &dyn EngineSink) {
    let mut resampler = Resampler::new(rate, SAMPLE_RATE as u32);
    let mut meter = LevelMeter::new();
    let mut raw = Vec::with_capacity(rate as usize / 10);
    let mut out = Vec::with_capacity(SAMPLE_RATE / 10);
    loop {
        let finished = stop.load(Ordering::Acquire);
        raw.clear();
        let n = consumer.slots();
        if n > 0 {
            if let Ok(chunk) = consumer.read_chunk(n) {
                let (a, b) = chunk.as_slices();
                raw.extend_from_slice(a);
                raw.extend_from_slice(b);
                chunk.commit_all();
            }
        }
        out.clear();
        resampler.process(&raw, &mut out);
        if finished {
            resampler.flush(&mut out);
        }
        if !out.is_empty() {
            meter.push(&out, |l| sink.level(l));
            audio.write().extend_from_slice(&out);
        }
        if finished {
            sink.level(0.0);
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn find_device(host: &cpal::Host, name: Option<&str>) -> anyhow::Result<cpal::Device> {
    if let Some(name) = name {
        if let Ok(mut devices) = host.input_devices() {
            if let Some(d) = devices.find(|d| d.name().map(|n| n == name).unwrap_or(false)) {
                return Ok(d);
            }
        }
        log::warn!("input device {name:?} not found, using the default");
    }
    host.default_input_device().ok_or_else(|| anyhow!("no input device available. Connect a microphone"))
}

fn open_stream(name: Option<&str>, dropped: Arc<AtomicU64>) -> anyhow::Result<(cpal::Stream, u32, rtrb::Consumer<f32>)> {
    let host = cpal::default_host();
    let device = find_device(&host, name)?;
    let config = device.default_input_config().map_err(|e| map_err("microphone device config", e))?;
    let rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    // Two seconds of headroom: the pump drains every 10 ms, so this only fills if the system stalls.
    let (producer, consumer) = rtrb::RingBuffer::<f32>::new(rate as usize * 2);
    let stream_config: cpal::StreamConfig = config.clone().into();

    let stream = match config.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, &stream_config, channels, producer, dropped),
        SampleFormat::I16 => build::<i16>(&device, &stream_config, channels, producer, dropped),
        SampleFormat::I32 => build::<i32>(&device, &stream_config, channels, producer, dropped),
        SampleFormat::U16 => build::<u16>(&device, &stream_config, channels, producer, dropped),
        SampleFormat::I8 => build::<i8>(&device, &stream_config, channels, producer, dropped),
        SampleFormat::U8 => build::<u8>(&device, &stream_config, channels, producer, dropped),
        SampleFormat::F64 => build::<f64>(&device, &stream_config, channels, producer, dropped),
        other => return Err(anyhow!("microphone device uses unsupported sample format {other:?}")),
    }?;
    log::info!("mic: {} @ {rate} Hz × {channels}", device.name().unwrap_or_default());
    Ok((stream, rate, consumer))
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    mut producer: rtrb::Producer<f32>,
    dropped: Arc<AtomicU64>,
) -> anyhow::Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = channels.max(1);
    let scale = 1.0 / channels as f32;
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                // Real-time thread: no allocation, no locks.
                let frames = data.len() / channels;
                let n = frames.min(producer.slots());
                if n < frames {
                    dropped.fetch_add((frames - n) as u64, Ordering::Relaxed);
                }
                if n == 0 {
                    return;
                }
                if let Ok(mut chunk) = producer.write_chunk(n) {
                    let (a, b) = chunk.as_mut_slices();
                    let mut frames_iter = data.chunks_exact(channels);
                    for slot in a.iter_mut().chain(b.iter_mut()) {
                        let frame = frames_iter.next().unwrap();
                        let mut acc = 0f32;
                        for &s in frame {
                            acc += s.to_sample::<f32>();
                        }
                        *slot = acc * scale;
                    }
                    chunk.commit_all();
                }
            },
            |e| log::error!("microphone stream error: {e}"),
            None,
        )
        .map_err(|e| map_err("microphone device stream", e))
}

fn map_err(what: &str, e: impl std::fmt::Display) -> anyhow::Error {
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    if lower.contains("permission") || lower.contains("not authorized") || lower.contains("denied") {
        anyhow!("microphone permission denied: {msg}")
    } else {
        anyhow!("{what} error: {msg}")
    }
}
