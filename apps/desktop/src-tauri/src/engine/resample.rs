//! Streaming windowed-sinc resampler (Kaiser window, 256-phase polyphase table).
//! Converts whatever the microphone delivers (44.1/48/96 kHz…) to Whisper's 16 kHz
//! with a proper anti-aliasing low-pass; cheap enough to run on a pump thread.

const PHASES: usize = 256;
const ZERO_CROSSINGS: f64 = 16.0;
const KAISER_BETA: f64 = 8.6;

pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    half: usize,
    table: Vec<f32>,
    taps: usize,
    hist: Vec<f32>,
    /// Position of the next output, in input samples relative to `hist[0]`.
    t: f64,
    passthrough: bool,
}

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    for k in 1..50 {
        term *= q / (k * k) as f64;
        sum += term;
        if term < 1e-12 * sum {
            break;
        }
    }
    sum
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        let step = in_rate as f64 / out_rate as f64;
        if in_rate == out_rate {
            return Self { step, half: 0, table: Vec::new(), taps: 0, hist: Vec::new(), t: 0.0, passthrough: true };
        }
        // Cut-off as a fraction of the input Nyquist; leave a little transition band.
        let cutoff = (1.0 / step).min(1.0) * 0.94;
        let half = (ZERO_CROSSINGS / cutoff).ceil() as usize;
        let taps = 2 * half;
        let i0b = bessel_i0(KAISER_BETA);
        let mut table = vec![0f32; (PHASES + 1) * taps];
        for p in 0..=PHASES {
            let frac = p as f64 / PHASES as f64;
            let row = &mut table[p * taps..(p + 1) * taps];
            let mut sum = 0.0;
            let mut vals = vec![0f64; taps];
            for (j, v) in vals.iter_mut().enumerate() {
                // distance from the output instant to input tap j
                let d = frac + half as f64 - 1.0 - j as f64;
                let x = cutoff * d;
                let sinc = if x.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
                let r = d / half as f64;
                let w = if r.abs() >= 1.0 { 0.0 } else { bessel_i0(KAISER_BETA * (1.0 - r * r).sqrt()) / i0b };
                *v = sinc * w;
                sum += *v;
            }
            for (o, v) in row.iter_mut().zip(vals) {
                *o = (v / sum) as f32;
            }
        }
        Self { step, half, table, taps, hist: vec![0.0; half], t: half as f64, passthrough: false }
    }

    /// Appends resampled output for `input` to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        self.hist.extend_from_slice(input);
        loop {
            let base = self.t.floor() as usize;
            if base + self.half >= self.hist.len() {
                break;
            }
            let frac = self.t - base as f64;
            let mut p = (frac * PHASES as f64).round() as usize;
            let mut b = base;
            if p == PHASES {
                p = 0;
                b += 1;
                if b + self.half >= self.hist.len() {
                    break;
                }
            }
            let row = &self.table[p * self.taps..(p + 1) * self.taps];
            let start = b + 1 - self.half;
            let xs = &self.hist[start..start + self.taps];
            let mut acc = 0f32;
            for (x, h) in xs.iter().zip(row) {
                acc += x * h;
            }
            out.push(acc);
            self.t += self.step;
        }
        // Drop history we will never need again.
        let keep_from = (self.t.floor() as usize + 1).saturating_sub(self.half);
        if keep_from > 4096 {
            self.hist.drain(..keep_from);
            self.t -= keep_from as f64;
        }
    }

    /// Pushes the filter tail out (call once at the end).
    pub fn flush(&mut self, out: &mut Vec<f32>) {
        if !self.passthrough {
            let zeros = vec![0f32; self.half + 2];
            self.process(&zeros, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f64, secs: f64) -> Vec<f32> {
        (0..(rate as f64 * secs) as usize).map(|i| (2.0 * std::f64::consts::PI * hz * i as f64 / rate as f64).sin() as f32 * 0.5).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn length_and_passband() {
        for rate in [44_100u32, 48_000, 96_000, 22_050, 8_000] {
            let input = sine(rate, 440.0, 1.0);
            let mut r = Resampler::new(rate, 16_000);
            let mut out = Vec::new();
            for chunk in input.chunks(480) {
                r.process(chunk, &mut out);
            }
            r.flush(&mut out);
            assert!((out.len() as i64 - 16_000).abs() < 40, "{rate}: {}", out.len());
            let mid = &out[2000..14000];
            assert!((rms(mid) - 0.5 / 2f32.sqrt()).abs() < 0.01, "{rate}: rms {}", rms(mid));
        }
    }

    #[test]
    fn rejects_aliases() {
        // 12 kHz is above the 8 kHz output Nyquist and must be filtered out, not folded to 4 kHz.
        let input = sine(48_000, 12_000.0, 1.0);
        let mut r = Resampler::new(48_000, 16_000);
        let mut out = Vec::new();
        r.process(&input, &mut out);
        assert!(rms(&out[2000..14000]) < 0.005, "alias leak {}", rms(&out[2000..14000]));
    }
}
