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
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use parking_lot::RwLock;

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
    stream_thread: Option<JoinHandle<()>>,
    pump_thread: Option<JoinHandle<()>>,
    audio: Arc<RwLock<Vec<f32>>>,
    dropped: Arc<AtomicU64>,
}

impl Recorder {
    /// Opens the device and starts streaming. Returns once the stream is running.
    pub fn start(device: Option<&str>, sink: Arc<dyn EngineSink>) -> anyhow::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));
        let audio = Arc::new(RwLock::new(Vec::with_capacity(SAMPLE_RATE * 60)));
        let (ready_tx, ready_rx) = mpsc::sync_channel::<anyhow::Result<(u32, rtrb::Consumer<f32>)>>(1);

        let device_name = device.map(str::to_owned);
        let stop_s = stop.clone();
        let dropped_s = dropped.clone();
        let stream_thread = thread::Builder::new()
            .name("mozhi-mic".into())
            .spawn(move || match open_stream(device_name.as_deref(), dropped_s) {
                Ok((stream, rate, consumer)) => {
                    if let Err(e) = stream.play() {
                        let _ = ready_tx.send(Err(anyhow!("microphone device could not start: {e}")));
                        return;
                    }
                    let _ = ready_tx.send(Ok((rate, consumer)));
                    while !stop_s.load(Ordering::Acquire) {
                        thread::park_timeout(Duration::from_millis(50));
                    }
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .context("spawn microphone thread")?;

        let (rate, mut consumer) = match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                stop.store(true, Ordering::Release);
                return Err(e);
            }
            Err(_) => {
                stop.store(true, Ordering::Release);
                return Err(anyhow!("microphone device did not respond (permission prompt pending?)"));
            }
        };

        let stop_p = stop.clone();
        let audio_p = audio.clone();
        let pump_thread = thread::Builder::new()
            .name("mozhi-pump".into())
            .spawn(move || {
                let mut resampler = Resampler::new(rate, SAMPLE_RATE as u32);
                let mut meter = LevelMeter::new();
                let mut raw = Vec::with_capacity(rate as usize / 10);
                let mut out = Vec::with_capacity(SAMPLE_RATE / 10);
                loop {
                    let finished = stop_p.load(Ordering::Acquire);
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
                        audio_p.write().extend_from_slice(&out);
                    }
                    if finished {
                        sink.level(0.0);
                        return;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            })
            .context("spawn audio pump")?;

        Ok(Self { stop, stream_thread: Some(stream_thread), pump_thread: Some(pump_thread), audio, dropped })
    }

    /// The growing 16 kHz mono buffer, for the live preview.
    pub fn shared(&self) -> Arc<RwLock<Vec<f32>>> {
        self.audio.clone()
    }

    /// Stops the stream, drains the ring and returns everything recorded.
    pub fn stop(mut self) -> Vec<f32> {
        self.halt();
        let dropped = self.dropped.load(Ordering::Relaxed);
        if dropped > 0 {
            log::warn!("audio ring overflowed, dropped {dropped} samples");
        }
        std::mem::take(&mut *self.audio.write())
    }

    fn halt(&mut self) {
        // Let the last callback land before the pump does its final drain.
        thread::sleep(Duration::from_millis(30));
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.stream_thread.take() {
            t.thread().unpark();
            let _ = t.join();
        }
        if let Some(t) = self.pump_thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if self.stream_thread.is_some() || self.pump_thread.is_some() {
            self.stop.store(true, Ordering::Release);
            if let Some(t) = self.stream_thread.take() {
                t.thread().unpark();
                let _ = t.join();
            }
            if let Some(t) = self.pump_thread.take() {
                let _ = t.join();
            }
        }
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
    host.default_input_device().ok_or_else(|| anyhow!("no input device available — connect a microphone"))
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
