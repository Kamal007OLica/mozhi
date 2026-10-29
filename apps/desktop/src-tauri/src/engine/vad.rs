//! Voice activity detection with Silero VAD (via whisper.cpp). Feeding Whisper only the
//! stretches that contain speech is the single best defence against hallucinated text
//! ("Thank you.", subtitle credits) from room noise before, between and after words.

use std::path::{Path, PathBuf};

use whisper_rs::{WhisperVadContext, WhisperVadContextParams, WhisperVadParams};

use super::dsp::SAMPLE_RATE;

pub const FILE: &str = "ggml-silero-v6.2.0.bin";
pub const URL: &str = "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin";
pub const BYTES: u64 = 885_098;
pub const SHA256: &str = "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987";

/// Lead-in kept before each speech segment and tail kept after it.
const PAD_BEFORE: usize = SAMPLE_RATE / 5;
const PAD_AFTER: usize = SAMPLE_RATE * 3 / 10;
/// Pauses longer than this are shortened to it.
const MAX_GAP: usize = SAMPLE_RATE * 2 / 5;

pub fn path(models_dir: &Path) -> PathBuf {
    models_dir.join(FILE)
}

pub struct Vad(WhisperVadContext);

#[derive(Debug, Clone, PartialEq)]
pub struct SpeechMap {
    /// Speech segments as sample ranges, padded and merged.
    pub segments: Vec<(usize, usize)>,
    pub voiced_ms: u64,
}

impl Vad {
    /// None when the VAD model is not downloaded (callers fall back to energy trimming).
    pub fn load(models_dir: &Path) -> Option<Self> {
        let p = path(models_dir);
        if !p.is_file() {
            return None;
        }
        let mut params = WhisperVadContextParams::new();
        params.set_n_threads(2);
        params.set_use_gpu(false);
        match WhisperVadContext::new(&p.to_string_lossy(), params) {
            Ok(ctx) => Some(Self(ctx)),
            Err(e) => {
                log::warn!("VAD model failed to load: {e:?}");
                None
            }
        }
    }

    pub fn speech(&mut self, audio: &[f32]) -> Option<SpeechMap> {
        if audio.len() < SAMPLE_RATE / 4 {
            return Some(SpeechMap { segments: Vec::new(), voiced_ms: 0 });
        }
        let mut params = WhisperVadParams::new();
        params.set_threshold(0.5);
        params.set_min_speech_duration(200);
        params.set_min_silence_duration(250);
        params.set_speech_pad(30);
        let segs = self.0.segments_from_samples(params, audio).map_err(|e| log::warn!("VAD: {e:?}")).ok()?;
        let mut raw = Vec::new();
        for i in 0..segs.num_segments() {
            if let Some(s) = segs.get_segment(i) {
                // timestamps are in centiseconds
                let a = ((s.start * SAMPLE_RATE as f32 / 100.0) as usize).min(audio.len());
                let b = ((s.end * SAMPLE_RATE as f32 / 100.0) as usize).min(audio.len());
                if b > a {
                    raw.push((a, b));
                }
            }
        }
        Some(merge(&raw, audio.len()))
    }
}

fn merge(raw: &[(usize, usize)], len: usize) -> SpeechMap {
    let voiced_ms = raw.iter().map(|(a, b)| ((b - a) * 1000 / SAMPLE_RATE) as u64).sum();
    let mut segments: Vec<(usize, usize)> = Vec::new();
    for &(a, b) in raw {
        let (a, b) = (a.saturating_sub(PAD_BEFORE), (b + PAD_AFTER).min(len));
        match segments.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => segments.push((a, b)),
        }
    }
    SpeechMap { segments, voiced_ms }
}

/// Joins the speech segments, shortening long pauses so Whisper never sees seconds of
/// silence (where it likes to invent text).
pub fn splice(audio: &[f32], map: &SpeechMap) -> Vec<f32> {
    let mut out = Vec::with_capacity(audio.len());
    for (i, &(a, b)) in map.segments.iter().enumerate() {
        if i > 0 {
            let gap = a.saturating_sub(map.segments[i - 1].1).min(MAX_GAP);
            out.extend(std::iter::repeat(0.0).take(gap));
        }
        out.extend_from_slice(&audio[a..b]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_and_splices() {
        let s = SAMPLE_RATE;
        let raw = [(s, 2 * s), (2 * s + s / 10, 3 * s), (8 * s, 9 * s)];
        let map = merge(&raw, 10 * s);
        assert_eq!(map.segments.len(), 2, "{map:?}");
        assert_eq!(map.voiced_ms, 1000 + 900 + 1000);
        let audio = vec![0.1f32; 10 * s];
        let spliced = splice(&audio, &map);
        // two padded segments plus one shortened pause
        let expect = (map.segments[0].1 - map.segments[0].0) + (map.segments[1].1 - map.segments[1].0) + MAX_GAP;
        assert_eq!(spliced.len(), expect);
    }
}
