//! Small audio helpers: the level meter that drives the ஃ dots, energy-based
//! speech trimming, loudness normalisation and split points for long dictation.

pub const SAMPLE_RATE: usize = 16_000;
const FRAME: usize = SAMPLE_RATE / 50; // 20 ms

/// Perceptual input level for the overlay: dBFS mapped −55…−12 dB → 0…1,
/// fast attack, slower release, emitted ~30×/s.
pub struct LevelMeter {
    window: usize,
    acc: f64,
    n: usize,
    smoothed: f32,
}

impl LevelMeter {
    pub fn new() -> Self {
        Self { window: SAMPLE_RATE / 30, acc: 0.0, n: 0, smoothed: 0.0 }
    }

    /// Feeds 16 kHz samples; calls `emit` once per completed ~33 ms window.
    pub fn push(&mut self, samples: &[f32], mut emit: impl FnMut(f32)) {
        for &s in samples {
            self.acc += (s as f64) * (s as f64);
            self.n += 1;
            if self.n == self.window {
                let rms = (self.acc / self.n as f64).sqrt();
                let db = 20.0 * (rms + 1e-9).log10();
                let target = (((db + 55.0) / 43.0) as f32).clamp(0.0, 1.0);
                let k = if target > self.smoothed { 0.65 } else { 0.18 };
                self.smoothed += (target - self.smoothed) * k;
                emit(self.smoothed);
                self.acc = 0.0;
                self.n = 0;
            }
        }
    }
}

fn frame_db(frame: &[f32]) -> f32 {
    let e = frame.iter().map(|v| v * v).sum::<f32>() / frame.len().max(1) as f32;
    10.0 * (e + 1e-12).log10()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Speech {
    pub start: usize,
    pub end: usize,
    /// Total voiced time inside [start, end).
    pub voiced_ms: u64,
}

/// Finds where speech starts and ends using an adaptive energy threshold
/// (noise floor + 12 dB, never below −52 dBFS) and pads 250 ms on each side.
pub fn find_speech(audio: &[f32]) -> Speech {
    let frames: Vec<f32> = audio.chunks(FRAME).map(frame_db).collect();
    if frames.is_empty() {
        return Speech { start: 0, end: 0, voiced_ms: 0 };
    }
    let mut sorted = frames.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let floor = sorted[sorted.len() / 10];
    let threshold = (floor + 12.0).max(-52.0);
    let voiced: Vec<bool> = frames.iter().map(|&d| d > threshold).collect();

    // Ignore isolated clicks: a frame counts only if one of its neighbours is voiced too.
    let solid = |i: usize| voiced[i] && ((i > 0 && voiced[i - 1]) || (i + 1 < voiced.len() && voiced[i + 1]));
    let first = (0..voiced.len()).find(|&i| solid(i));
    let last = (0..voiced.len()).rev().find(|&i| solid(i));
    match (first, last) {
        (Some(f), Some(l)) => {
            let pad = 250 / 20;
            let start = f.saturating_sub(pad) * FRAME;
            let end = ((l + 1 + pad) * FRAME).min(audio.len());
            let voiced_ms = (f..=l).filter(|&i| voiced[i]).count() as u64 * 20;
            Speech { start, end, voiced_ms }
        }
        _ => Speech { start: 0, end: 0, voiced_ms: 0 },
    }
}

/// Brings quiet microphones up to a healthy level (speech RMS ≈ −20 dBFS),
/// at most +20 dB, and never clips.
pub fn normalize(audio: &mut [f32]) {
    if audio.is_empty() {
        return;
    }
    let loud: Vec<f32> = audio.chunks(FRAME).map(frame_db).filter(|d| *d > -50.0).collect();
    if loud.is_empty() {
        return;
    }
    let speech_db = loud.iter().sum::<f32>() / loud.len() as f32;
    let peak = audio.iter().fold(0f32, |m, v| m.max(v.abs())).max(1e-6);
    let want = 10f32.powf((-20.0 - speech_db) / 20.0).min(10.0);
    let gain = want.min(0.95 / peak);
    if gain > 1.05 || gain < 0.95 {
        for v in audio.iter_mut() {
            *v *= gain;
        }
    }
}

/// Splits long audio into ≤`max_s` chunks, cutting at the quietest 100 ms
/// between `min_s` and `max_s` so words are not chopped in half.
pub fn split_points(audio: &[f32], min_s: f32, max_s: f32) -> Vec<(usize, usize)> {
    let max = (max_s * SAMPLE_RATE as f32) as usize;
    let min = (min_s * SAMPLE_RATE as f32) as usize;
    let win = SAMPLE_RATE / 10;
    let mut out = Vec::new();
    let mut start = 0;
    while audio.len() - start > max {
        let lo = start + min;
        let hi = start + max - win;
        let mut best = hi;
        let mut best_db = f32::MAX;
        let mut i = lo;
        while i <= hi {
            let d = frame_db(&audio[i..i + win]);
            if d < best_db {
                best_db = d;
                best = i;
            }
            i += win / 2;
        }
        let cut = best + win / 2;
        out.push((start, cut));
        start = cut;
    }
    out.push((start, audio.len()));
    out
}

/// True when every sample is exactly zero — macOS delivers digital silence when
/// microphone permission has been denied.
pub fn is_digital_silence(audio: &[f32]) -> bool {
    !audio.is_empty() && audio.iter().all(|v| *v == 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(secs: f32, amp: f32) -> Vec<f32> {
        (0..(secs * SAMPLE_RATE as f32) as usize).map(|i| (i as f32 * 0.07).sin() * amp).collect()
    }

    #[test]
    fn trims_silence() {
        let mut a = vec![0.0005f32; SAMPLE_RATE];
        a.extend(tone(1.0, 0.3));
        a.extend(vec![0.0005f32; SAMPLE_RATE]);
        let s = find_speech(&a);
        assert!(s.start > SAMPLE_RATE / 2 && s.start < SAMPLE_RATE, "{s:?}");
        assert!(s.end > 2 * SAMPLE_RATE && s.end < 2 * SAMPLE_RATE + SAMPLE_RATE / 2, "{s:?}");
        assert!((900..=1100).contains(&s.voiced_ms), "{s:?}");
    }

    #[test]
    fn silence_has_no_speech() {
        let a = vec![0.0001f32; SAMPLE_RATE * 2];
        assert_eq!(find_speech(&a).voiced_ms, 0);
    }

    #[test]
    fn splits_long_audio_at_quiet_points() {
        let mut a = Vec::new();
        for _ in 0..7 {
            a.extend(tone(9.0, 0.3));
            a.extend(vec![0.0f32; SAMPLE_RATE / 2]);
        }
        let parts = split_points(&a, 18.0, 28.0);
        assert!(parts.len() >= 3);
        for (s, e) in &parts {
            assert!(e - s <= 28 * SAMPLE_RATE);
        }
        assert_eq!(parts.last().unwrap().1, a.len());
    }

    #[test]
    fn normalize_boosts_quiet_speech() {
        let mut a = tone(1.0, 0.01);
        normalize(&mut a);
        let peak = a.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.08 && peak <= 0.96, "{peak}");
    }
}
