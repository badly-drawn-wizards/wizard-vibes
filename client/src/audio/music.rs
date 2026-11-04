// Music scheduling, apart from any device: the shader computes the music in
// chunks on the GPU (native/shader's music, soundboard and music_mix), and
// this asks for them and places them on the mixer's clock.
//
// The soundboard's state carries from one chunk to the next, so chunks are
// asked for strictly in order, one at a time, each from where the last
// ended (chunk numbers count them from 0; one asked again, its answer lost,
// starts from the same state). The song is the loop played twice: the first
// lap gets the soundboard ringing, as if it had been playing all along, and
// the second is kept; once the second lap is complete it plays round and
// round (its start already rings with its end's tail), and the GPU makes no
// more. A new build starts it afresh.
//
// The computing goes on in the background from the start, whether the
// music is on or not, a chunk (half a second) as soon as the last is back,
// so the loop is usually complete before anyone asks for music. Turned on
// before then, it plays the chunks as they come, from the next.
//
// Times: `now` is the mixer's clock in seconds; places on it are frames.

use std::sync::Arc;

use crate::types::MusicRequest;

const CHUNK: f64 = 0.5; // seconds a chunk
const START: f64 = 0.15; // seconds from turning on to the first chunk's place
const EXPIRE: f64 = 3.0; // seconds after which an unanswered request is asked again
const STOP_AFTER: f64 = 0.2; // seconds from turning off (the fade) to stopping
pub const GAIN: f32 = 0.8;

// What the mixer is to do with the music.
pub enum Cmd {
    // play interleaved stereo from a frame of the clock
    Chunk { at: u64, data: Arc<[f32]> },
    // play the whole loop round and round from a frame of the clock, from
    // its frame `offset`
    Loop { at: u64, offset: usize, data: Arc<[f32]> },
    // stop the loop and drop the chunks to come
    Clear,
    // glide the music's gain to `target` (time constant `tau` seconds)
    Gain { target: f32, tau: f32 },
}

#[derive(Default)]
pub struct Stats {
    // chunks placed, and those that came too late for their place
    pub played: u32,
    pub late: u32,
}

// Playing: the clock's frame at which song frame `from` plays (chunks from
// there on are placed by it), and once the loop plays, the clock's frame
// at which it began (its frame 0).
struct Play {
    start: u64,
    from: usize,
    loop_at: Option<u64>,
}

pub struct Music {
    pub on: bool,
    pub stats: Stats,
    lap: usize,          // frames in the loop (0: not known yet)
    rate: u32,           // the rate the song is computed at
    next: usize,         // song frames computed so far: the next request starts here
    chunks: u32,         // chunks computed so far
    pending: Option<(MusicRequest, f64)>, // the request out, and when it was made
    second: Vec<f32>,    // the second lap, as far as computed (interleaved)
    looped: Option<Arc<[f32]>>, // the second lap, complete
    play: Option<Play>,
    resume: usize,       // where in the loop to carry on, turned on again
    stop_at: Option<f64>,
}

impl Default for Music {
    fn default() -> Self {
        Self::new()
    }
}

fn frame_at(t: f64, rate: u32) -> u64 {
    (t.max(0.0) * rate as f64).round() as u64
}

impl Music {
    pub fn new() -> Music {
        Music {
            on: false,
            stats: Stats::default(),
            lap: 0,
            rate: 0,
            next: 0,
            chunks: 0,
            pending: None,
            second: Vec::new(),
            looped: None,
            play: None,
            resume: 0,
            stop_at: None,
        }
    }

    // How much of the kept (second) lap is there (0..1).
    pub fn cached(&self) -> f32 {
        if self.looped.is_some() {
            1.0
        } else if self.lap == 0 {
            0.0
        } else {
            self.next.saturating_sub(self.lap) as f32 / self.lap as f32
        }
    }

    #[cfg(test)]
    pub fn looping(&self) -> bool {
        self.looped.is_some()
    }

    // The song's computing from the start (and its playing from there, if on).
    fn restart(&mut self, now: f64, out: &mut Vec<Cmd>) {
        self.next = 0;
        self.chunks = 0;
        self.pending = None;
        self.second.clear();
        self.looped = None;
        self.resume = 0;
        if self.play.is_some() {
            out.push(Cmd::Clear);
            self.play = Some(Play { start: frame_at(now + START, self.rate), from: 0, loop_at: None });
        }
    }

    // A new build: its music may differ, and its soundboard has no state.
    pub fn reset(&mut self, now: f64, _rate: u32, out: &mut Vec<Cmd>) {
        self.restart(now, out);
    }

    pub fn set_on(&mut self, on: bool, now: f64, rate: u32, out: &mut Vec<Cmd>) {
        if on == self.on {
            return;
        }
        self.on = on;
        if !on {
            // fade out, then stop
            out.push(Cmd::Gain { target: 0.0, tau: 0.03 });
            self.stop_at = Some(now + STOP_AFTER);
            return;
        }
        out.push(Cmd::Gain { target: GAIN, tau: 0.05 });
        // (still fading out: carry on with what is queued)
        if self.stop_at.take().is_some() && self.play.is_some() {
            return;
        }
        if let Some(data) = &self.looped {
            let at = frame_at(now + 0.05, rate);
            out.push(Cmd::Loop { at, offset: self.resume, data: data.clone() });
            self.play = Some(Play { start: 0, from: 0, loop_at: Some(at - self.resume as u64) });
        } else {
            // (from the next chunk to come)
            self.play = Some(Play { start: frame_at(now + START, rate), from: self.next, loop_at: None });
        }
    }

    // Call each frame: stops the music once faded out.
    pub fn tick(&mut self, now: f64, rate: u32, out: &mut Vec<Cmd>) {
        if self.stop_at.is_some_and(|t| now >= t) && !self.on {
            self.stop_at = None;
            out.push(Cmd::Clear);
            // (where the loop got to, to carry on from there)
            if let Some(Play { loop_at: Some(at), .. }) = self.play {
                if self.lap > 0 {
                    self.resume = (frame_at(now, rate).saturating_sub(at) % self.lap as u64) as usize;
                }
            }
            self.play = None;
        }
    }

    // What to compute next, if anything: the chunk after the last, as soon
    // as the last is back, until the loop is complete; or the one still
    // unanswered, again, after a while. `period` is the loop's length in
    // seconds, `rate` the output's.
    pub fn want(&mut self, now: f64, rate: u32, period: f32, out: &mut Vec<Cmd>) -> Option<MusicRequest> {
        if period <= 0.0 || rate == 0 {
            return None;
        }
        let lap = (period as f64 * rate as f64).round() as usize;
        if lap != self.lap || rate != self.rate {
            self.lap = lap;
            self.rate = rate;
            self.restart(now, out);
        }
        if self.looped.is_some() {
            return None;
        }
        if let Some((req, asked)) = &mut self.pending {
            if now - *asked < EXPIRE {
                return None;
            }
            *asked = now;
            return Some(*req);
        }
        let frames = ((CHUNK * rate as f64) as usize).min(2 * lap - self.next);
        let t0 = ((self.next as f64 / rate as f64) % period as f64) as f32;
        let req = MusicRequest { t0, rate: rate as f32, frames: frames as u32, chunk: self.chunks };
        self.pending = Some((req, now));
        Some(req)
    }

    // A chunk computed: kept if it is of the second lap, and, while
    // playing, placed (if it came late, the song moves on to make room: a
    // gap rather than an overlap); with the second lap complete, that loops
    // from where the song ends.
    pub fn take(&mut self, req: MusicRequest, samples: &[f32], now: f64, out: &mut Vec<Cmd>) {
        let Some((want, _)) = self.pending else { return };
        // (an answer to a request from before a restart: not ours)
        if want != req {
            return;
        }
        self.pending = None;
        let frames = req.frames as usize;
        let s = self.next;
        let lap = self.lap;
        let rate = self.rate;
        if let Some(p) = &mut self.play {
            if p.loop_at.is_none() && s >= p.from {
                let soonest = frame_at(now + 0.02, rate);
                if p.start + ((s - p.from) as u64) < soonest {
                    self.stats.late += 1;
                    p.start = soonest - (s - p.from) as u64;
                }
                out.push(Cmd::Chunk { at: p.start + (s - p.from) as u64, data: samples[..2 * frames].into() });
                self.stats.played += 1;
            }
        }
        if s + frames > lap {
            let from = lap.saturating_sub(s);
            self.second.extend_from_slice(&samples[2 * from..2 * frames]);
        }
        self.next += frames;
        self.chunks += 1;
        if self.next >= 2 * lap {
            let data: Arc<[f32]> = std::mem::take(&mut self.second).into();
            if let Some(p) = &mut self.play {
                if p.loop_at.is_none() {
                    let at = p.start + (2 * lap - p.from) as u64;
                    out.push(Cmd::Loop { at, offset: 0, data: data.clone() });
                    p.loop_at = Some(at);
                }
            }
            self.looped = Some(data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48000;
    // (a short loop, so whole songs are quick: 2.5 chunks a lap)
    const PERIOD: f32 = 1.25;
    const LAP: usize = 60000;

    // A chunk "computed": each frame holds its song frame (both sides).
    fn compute(req: MusicRequest, from: usize) -> Vec<f32> {
        (0..req.frames as usize).flat_map(|i| [(from + i) as f32, (from + i) as f32]).collect()
    }

    // Runs at 60 frames a second for `secs`, answering each request a frame
    // later (dropping the first answer if asked); returns the commands.
    fn session(m: &mut Music, t: &mut f64, secs: f64, drop_one: bool, mut on_at: Option<f64>) -> Vec<Cmd> {
        let mut out = vec![];
        let mut held: Option<MusicRequest> = None;
        let mut dropped = !drop_one;
        let mut computed = 0usize;
        while *t < secs {
            if on_at.is_some_and(|a| *t >= a) {
                m.set_on(true, *t, RATE, &mut out);
                on_at = None;
            }
            if let Some(r) = held.take() {
                if !dropped {
                    dropped = true;
                } else {
                    m.take(r, &compute(r, computed), *t, &mut out);
                    computed += r.frames as usize;
                }
            }
            m.tick(*t, RATE, &mut out);
            held = m.want(*t, RATE, PERIOD, &mut out);
            *t += 1.0 / 60.0;
        }
        out
    }

    fn chunks(out: &[Cmd]) -> Vec<(u64, Arc<[f32]>)> {
        out.iter().filter_map(|c| if let Cmd::Chunk { at, data } = c { Some((*at, data.clone())) } else { None }).collect()
    }

    #[test]
    fn computes_in_the_background_while_off() {
        let mut m = Music::new();
        let mut t = 0.0;
        let out = session(&mut m, &mut t, 1.0, false, None);
        assert!(m.looping(), "the loop is computed with the music off");
        assert!(chunks(&out).is_empty(), "and nothing plays");
        // turned on: the loop plays at once, from its start
        let mut out = vec![];
        m.set_on(true, t, RATE, &mut out);
        assert!(out.iter().any(|c| matches!(c, Cmd::Loop { offset: 0, .. })));
        assert!(m.want(t, RATE, PERIOD, &mut out).is_none());
    }

    #[test]
    fn requests_are_half_seconds_numbered_in_order() {
        let mut m = Music::new();
        let mut out = vec![];
        let a = m.want(0.0, RATE, PERIOD, &mut out).unwrap();
        assert_eq!((a.chunk, a.frames), (0, RATE / 2));
        // (unanswered: nothing more, until it is asked again)
        assert!(m.want(0.1, RATE, PERIOD, &mut out).is_none());
        let again = m.want(EXPIRE + 0.1, RATE, PERIOD, &mut out).unwrap();
        assert_eq!(again, a);
        m.take(a, &compute(a, 0), EXPIRE + 0.1, &mut out);
        let b = m.want(EXPIRE + 0.2, RATE, PERIOD, &mut out).unwrap();
        assert_eq!((b.chunk, b.t0), (1, 0.5));
    }

    #[test]
    fn on_before_the_loop_is_done_plays_the_chunks_to_come_back_to_back_then_loops() {
        let mut m = Music::new();
        let mut t = 0.0;
        // (on at once: before anything is computed)
        let out = session(&mut m, &mut t, 6.0, false, Some(0.0));
        assert!(m.looping());
        let cs = chunks(&out);
        let start = cs[0].0;
        let mut s = 0usize;
        for (at, d) in &cs {
            assert_eq!(*at, start + s as u64);
            assert_eq!(d[0], s as f32);
            s += d.len() / 2;
        }
        assert_eq!(s, 2 * LAP);
        let (at, offset, data) = out.iter().find_map(|c| if let Cmd::Loop { at, offset, data } = c { Some((*at, *offset, data.clone())) } else { None }).unwrap();
        assert_eq!((at, offset), (start + 2 * LAP as u64, 0));
        assert_eq!(data.len() / 2, LAP);
        assert_eq!((data[0], data[2 * LAP - 2]), (LAP as f32, (2 * LAP - 1) as f32));
        assert_eq!(m.stats.late, 0);
    }

    #[test]
    fn a_lost_answer_is_asked_again_and_the_song_carries_on() {
        let mut m = Music::new();
        let mut t = 0.0;
        let out = session(&mut m, &mut t, 8.0, true, Some(0.0));
        assert!(m.looping());
        let mut s = 0usize;
        for (_, d) in chunks(&out) {
            assert_eq!(d[0], s as f32);
            s += d.len() / 2;
        }
        assert_eq!(s, 2 * LAP);
    }

    #[test]
    fn a_new_build_starts_from_chunk_0() {
        let mut m = Music::new();
        let mut t = 0.0;
        session(&mut m, &mut t, 0.1, false, Some(0.0));
        let mut out = vec![];
        m.reset(t, RATE, &mut out);
        assert!(matches!(out.last(), Some(Cmd::Clear)));
        let r = m.want(t, RATE, PERIOD, &mut out).unwrap();
        assert_eq!(r.chunk, 0);
        assert_eq!(m.cached(), 0.0);
    }

    #[test]
    fn off_and_on_again_carries_on_the_loop() {
        let mut m = Music::new();
        let mut t = 0.0;
        session(&mut m, &mut t, 6.0, false, Some(0.0));
        assert!(m.looping());
        let mut out = vec![];
        m.set_on(false, t, RATE, &mut out);
        m.tick(t + 1.0, RATE, &mut out);
        out.clear();
        m.set_on(true, t + 2.0, RATE, &mut out);
        assert!(out.iter().any(|c| matches!(c, Cmd::Loop { .. })));
    }
}
