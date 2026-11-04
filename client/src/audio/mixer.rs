// The mixer the output callback pulls from (no device here: see output.rs).
// Its clock is the frames it has rendered; music chunks and the cached loop
// are placed on that clock (as music.ts places AudioBufferSourceNodes on the
// Web Audio clock), and the sound effects play from when they are started,
// each panned as a StereoPannerNode pans stereo input.

use std::sync::Arc;

use super::music::Cmd;

// The sound effects' level (sfx.ts's VOLUME), and the music's when on.
pub const SFX_VOLUME: f32 = 0.7;
// The most effects sounding at once (the oldest make way).
const MAX_VOICES: usize = 64;

// Interleaved stereo samples placed at a frame of the clock.
struct Chunk {
    at: u64,
    data: Arc<[f32]>,
}

// The cached loop: from frame `at` on, the loop from its frame `offset`,
// round and round.
struct Looping {
    at: u64,
    offset: usize,
    data: Arc<[f32]>,
}

struct Voice {
    data: Arc<[f32]>,
    // the next frame
    i: usize,
    // [l from l, l from r, r from l, r from r]
    m: [f32; 4],
}

pub struct Mixer {
    pub rate: u32,
    // frames rendered so far (the clock)
    pub pos: u64,
    chunks: Vec<Chunk>,
    looping: Option<Looping>,
    // the music's gain, gliding to its target (as setTargetAtTime)
    gain: f32,
    target: f32,
    coef: f32,
    voices: Vec<Voice>,
    // the loudest music sample played (for tests)
    pub peak: f32,
}

impl Mixer {
    pub fn new(rate: u32) -> Mixer {
        Mixer {
            rate,
            pos: 0,
            chunks: Vec::new(),
            looping: None,
            gain: 0.0,
            target: 0.0,
            coef: 1.0,
            voices: Vec::new(),
            peak: 0.0,
        }
    }

    pub fn apply(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Chunk { at, data } => {
                // (wholly too late: dropped)
                if at + (data.len() / 2) as u64 > self.pos {
                    self.chunks.push(Chunk { at, data });
                }
            }
            Cmd::Loop { at, offset, data } => {
                if !data.is_empty() {
                    self.looping = Some(Looping { at, offset, data });
                }
            }
            Cmd::Clear => {
                self.looping = None;
                self.chunks.clear();
            }
            Cmd::Gain { target, tau } => {
                self.target = target;
                self.coef = 1.0 - (-1.0 / (tau.max(1e-4) * self.rate as f32)).exp();
            }
        }
    }

    // Starts an effect (interleaved stereo at the mixer's rate), panned -1
    // (left) .. 1 (right), at a gain.
    pub fn play(&mut self, data: Arc<[f32]>, pan: f32, gain: f32) {
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        self.voices.push(Voice { data, i: 0, m: pan_matrix(pan, gain * SFX_VOLUME) });
    }

    // Whether anything is sounding or to sound.
    #[cfg(test)]
    pub fn busy(&self) -> bool {
        !self.chunks.is_empty() || self.looping.is_some() || !self.voices.is_empty()
    }

    // Renders the next out.len() / 2 frames (interleaved stereo) and moves
    // the clock on.
    pub fn render(&mut self, out: &mut [f32]) {
        let n = out.len() / 2;
        let (p0, p1) = (self.pos, self.pos + n as u64);
        out.fill(0.0);
        // the music
        for c in &self.chunks {
            let len = (c.data.len() / 2) as u64;
            let (a, b) = (p0.max(c.at), p1.min(c.at + len));
            for p in a..b {
                let (o, i) = (2 * (p - p0) as usize, 2 * (p - c.at) as usize);
                out[o] += c.data[i];
                out[o + 1] += c.data[i + 1];
            }
        }
        if let Some(l) = &self.looping {
            let len = l.data.len() / 2;
            if l.at < p1 {
                let a = p0.max(l.at);
                let mut k = (l.offset + (a - l.at) as usize) % len;
                for p in a..p1 {
                    let o = 2 * (p - p0) as usize;
                    out[o] += l.data[2 * k];
                    out[o + 1] += l.data[2 * k + 1];
                    k += 1;
                    if k == len {
                        k = 0;
                    }
                }
            }
        }
        for f in out.chunks_exact_mut(2) {
            self.gain += (self.target - self.gain) * self.coef;
            self.peak = self.peak.max(f[0].abs()).max(f[1].abs());
            f[0] *= self.gain;
            f[1] *= self.gain;
        }
        // the effects
        for v in &mut self.voices {
            let left = v.data.len() / 2 - v.i;
            for (f, s) in out.chunks_exact_mut(2).zip(v.data[2 * v.i..].chunks_exact(2)) {
                f[0] += v.m[0] * s[0] + v.m[1] * s[1];
                f[1] += v.m[2] * s[0] + v.m[3] * s[1];
            }
            v.i += left.min(n);
        }
        self.voices.retain(|v| 2 * v.i < v.data.len());
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
        self.pos = p1;
        self.chunks.retain(|c| c.at + (c.data.len() / 2) as u64 > p1);
    }
}

// How a StereoPannerNode pans stereo input (equal power), times a gain.
pub fn pan_matrix(pan: f32, gain: f32) -> [f32; 4] {
    let p = pan.clamp(-1.0, 1.0);
    let h = std::f32::consts::FRAC_PI_2;
    let m = if p <= 0.0 {
        let x = (p + 1.0) * h;
        [1.0, x.cos(), 0.0, x.sin()]
    } else {
        let x = p * h;
        [x.cos(), 0.0, x.sin(), 1.0]
    };
    m.map(|v| v * gain)
}

// Interleaved stereo resampled from one rate to another (linearly).
pub fn resample(src: &[f32], from: f32, to: f32) -> Vec<f32> {
    let n = src.len() / 2;
    if n == 0 || from <= 0.0 || to <= 0.0 || (from - to).abs() < 0.5 {
        return src[..2 * n].to_vec();
    }
    let m = ((n as f64) * to as f64 / from as f64).round() as usize;
    let step = from as f64 / to as f64;
    let mut out = Vec::with_capacity(2 * m);
    for j in 0..m {
        let x = j as f64 * step;
        let i = (x.floor() as usize).min(n - 1);
        let t = (x - i as f64) as f32;
        let k = (i + 1).min(n - 1);
        for c in 0..2 {
            let (a, b) = (src[2 * i + c], src[2 * k + c]);
            out.push(a + (b - a) * t);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(at: u64, frames: usize, v: f32) -> Cmd {
        Cmd::Chunk { at, data: vec![v; 2 * frames].into() }
    }

    #[test]
    fn chunks_play_at_their_frames_back_to_back() {
        let mut m = Mixer::new(1000);
        m.apply(Cmd::Gain { target: 1.0, tau: 1e-6 });
        m.apply(chunk(10, 10, 0.5));
        m.apply(chunk(20, 10, 0.25));
        let mut out = vec![0.0; 2 * 40];
        m.render(&mut out);
        let l: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert!(l[..10].iter().all(|&v| v == 0.0));
        assert!(l[10..20].iter().all(|&v| (v - 0.5).abs() < 1e-6));
        assert!(l[20..30].iter().all(|&v| (v - 0.25).abs() < 1e-6));
        assert!(l[30..].iter().all(|&v| v == 0.0));
        assert!(!m.busy());
        // too late: dropped
        m.apply(chunk(5, 10, 1.0));
        assert!(!m.busy());
    }

    #[test]
    fn loop_wraps() {
        let mut m = Mixer::new(1000);
        m.apply(Cmd::Gain { target: 1.0, tau: 1e-6 });
        let data: Vec<f32> = (0..4).flat_map(|i| [i as f32 / 10.0; 2]).collect();
        m.apply(Cmd::Loop { at: 2, offset: 3, data: data.into() });
        let mut out = vec![0.0; 2 * 8];
        m.render(&mut out);
        let l: Vec<f32> = out.chunks(2).map(|f| (f[0] * 10.0).round()).collect();
        assert_eq!(l, vec![0.0, 0.0, 3.0, 0.0, 1.0, 2.0, 3.0, 0.0]);
    }

    #[test]
    fn effects_mix_and_pan() {
        let mut m = Mixer::new(1000);
        let d: Arc<[f32]> = vec![0.25; 2 * 4].into();
        m.play(d.clone(), -1.0, 1.0);
        m.play(d, 0.0, 1.0);
        let mut out = vec![0.0; 2 * 6];
        m.render(&mut out);
        // hard left: both channels to the left; centre: as is
        let v = SFX_VOLUME * 0.25;
        assert!((out[0] - 3.0 * v).abs() < 1e-5, "{out:?}");
        assert!((out[1] - v).abs() < 1e-5);
        assert_eq!(&out[8..], &[0.0; 4]);
        assert!(!m.busy());
    }

    #[test]
    fn resamples() {
        let src: Vec<f32> = (0..100).flat_map(|i| [i as f32, -(i as f32)]).collect();
        let out = resample(&src, 100.0, 200.0);
        assert_eq!(out.len(), 400);
        assert!((out[2 * 3] - 1.5).abs() < 1e-5);
        assert!((out[2 * 3 + 1] + 1.5).abs() < 1e-5);
    }
}
