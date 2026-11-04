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
// Times: `now` is the mixer's clock in seconds; places on it are frames.

use std::sync::Arc;

use crate::types::MusicRequest;

pub const FRAMES: usize = 4096; // frames a chunk (~85 ms at 48 kHz)
const AHEAD: f64 = 4.0; // seconds computed ahead of the clock, at most
const START: f64 = 0.15; // seconds from starting to the first chunk's place
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

pub struct Music {
    pub on: bool,
    pub stats: Stats,
    lap: usize,          // frames in the loop (0: not known yet)
    next: usize,         // song frames computed so far: the next request starts here
    start: Option<u64>,  // the clock's frame that song frame 0 plays at
    pending: Option<(MusicRequest, f64)>, // the request out, and when it was made
    second: Vec<f32>,    // the second lap, as far as computed (interleaved)
    looped: Option<(u64, Arc<[f32]>)>, // the second lap complete: from which clock frame it loops
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
        Music { on: false, stats: Stats::default(), lap: 0, next: 0, start: None, pending: None, second: Vec::new(), looped: None, resume: 0, stop_at: None }
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

    // From the start of the song: nothing computed, nothing placed.
    fn restart(&mut self) {
        self.next = 0;
        self.start = None;
        self.pending = None;
        self.second.clear();
        self.looped = None;
        self.resume = 0;
    }

    // A new build: its music may differ, and its soundboard has no state.
    pub fn reset(&mut self, _now: f64, _rate: u32, out: &mut Vec<Cmd>) {
        out.push(Cmd::Clear);
        self.restart();
    }

    pub fn set_on(&mut self, on: bool, now: f64, rate: u32, out: &mut Vec<Cmd>) {
        if on == self.on {
            return;
        }
        self.on = on;
        if on {
            out.push(Cmd::Gain { target: GAIN, tau: 0.05 });
            // (still fading out: carry on with what is queued)
            if self.stop_at.take().is_some() {
                return;
            }
            if let Some((_, data)) = &self.looped {
                let at = frame_at(now + 0.05, rate);
                out.push(Cmd::Loop { at, offset: self.resume, data: data.clone() });
                self.looped = Some((at - self.resume as u64, data.clone()));
            } else {
                self.restart();
            }
        } else {
            // fade out, then stop
            out.push(Cmd::Gain { target: 0.0, tau: 0.03 });
            self.stop_at = Some(now + STOP_AFTER);
        }
    }

    // Call each frame: stops the music once faded out.
    pub fn tick(&mut self, now: f64, rate: u32, out: &mut Vec<Cmd>) {
        if self.stop_at.is_some_and(|t| now >= t) && !self.on {
            self.stop_at = None;
            out.push(Cmd::Clear);
            match &self.looped {
                // (where the loop got to, to carry on from there)
                Some((from, _)) if self.lap > 0 => self.resume = (frame_at(now, rate).saturating_sub(*from) % self.lap as u64) as usize,
                Some(_) => {}
                // (half computed: it starts afresh next time)
                None => self.restart(),
            }
        }
    }

    // What to compute next, if anything: the chunk after the last, once the
    // clock is near enough; or the one still unanswered, again, after a
    // while. `period` is the loop's length in seconds, `rate` the output's.
    pub fn want(&mut self, now: f64, rate: u32, period: f32, _out: &mut Vec<Cmd>) -> Option<MusicRequest> {
        if !self.on || self.stop_at.is_some() || period <= 0.0 || rate == 0 {
            return None;
        }
        let lap = (period as f64 * rate as f64).round() as usize;
        if lap != self.lap {
            self.lap = lap;
            self.restart();
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
        let start = *self.start.get_or_insert(frame_at(now + START, rate));
        if start + self.next as u64 > frame_at(now + AHEAD, rate) {
            return None;
        }
        let frames = FRAMES.min(2 * lap - self.next);
        let t0 = ((self.next as f64 / rate as f64) % period as f64) as f32;
        let req = MusicRequest { t0, rate: rate as f32, frames: frames as u32, chunk: (self.next / FRAMES) as u32 };
        self.pending = Some((req, now));
        Some(req)
    }

    // A chunk computed: placed after the last (if it came late, the song
    // moves on to make room: a gap rather than an overlap), and kept if it
    // is of the second lap; with the second lap complete, that loops from
    // where the song ends.
    pub fn take(&mut self, req: MusicRequest, samples: &[f32], now: f64, out: &mut Vec<Cmd>) {
        let Some((want, _)) = self.pending else { return };
        // (an answer to a request from before a restart: not ours)
        if want != req {
            return;
        }
        self.pending = None;
        let rate = req.rate.round() as u32;
        let frames = req.frames as usize;
        let s = self.next;
        let mut start = self.start.unwrap_or(frame_at(now + START, rate));
        let soonest = frame_at(now + 0.02, rate);
        if start + (s as u64) < soonest {
            self.stats.late += 1;
            start = soonest - s as u64;
        }
        self.start = Some(start);
        let data: Arc<[f32]> = samples[..2 * frames].into();
        out.push(Cmd::Chunk { at: start + s as u64, data });
        self.stats.played += 1;
        let lap = self.lap;
        if s + frames > lap {
            let from = lap.saturating_sub(s);
            self.second.extend_from_slice(&samples[2 * from..2 * frames]);
        }
        self.next += frames;
        if self.next >= 2 * lap {
            let data: Arc<[f32]> = std::mem::take(&mut self.second).into();
            let at = start + 2 * lap as u64;
            out.push(Cmd::Loop { at, offset: 0, data: data.clone() });
            self.looped = Some((at, data));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48000;
    // (a short loop, so whole songs are quick)
    const PERIOD: f32 = 0.5;
    const LAP: usize = 24000;

    // A chunk "computed": each frame holds its song frame (both sides).
    fn compute(req: MusicRequest, from: usize) -> Vec<f32> {
        (0..req.frames as usize).flat_map(|i| [(from + i) as f32, (from + i) as f32]).collect()
    }

    // Runs the music at 60 frames a second for `secs`, answering each request
    // a frame later; returns the commands.
    fn session(m: &mut Music, t: &mut f64, secs: f64, drop_one: bool) -> Vec<Cmd> {
        let mut out = vec![];
        let mut held: Option<MusicRequest> = None;
        let mut dropped = !drop_one;
        let mut computed = 0usize;
        while *t < secs {
            if let Some(r) = held.take() {
                if !dropped {
                    dropped = true;
                } else {
                    let from = r.chunk as usize * FRAMES;
                    m.take(r, &compute(r, from), *t, &mut out);
                    computed = computed.max(from + r.frames as usize);
                }
            }
            m.tick(*t, RATE, &mut out);
            held = m.want(*t, RATE, PERIOD, &mut out);
            *t += 1.0 / 60.0;
        }
        out
    }

    #[test]
    fn off_asks_nothing() {
        let mut m = Music::new();
        assert!(m.want(0.0, RATE, PERIOD, &mut vec![]).is_none());
    }

    #[test]
    fn chunks_in_order_back_to_back_then_the_second_lap_loops() {
        let mut m = Music::new();
        let mut out = vec![];
        m.set_on(true, 0.0, RATE, &mut out);
        let mut t = 0.0;
        out.extend(session(&mut m, &mut t, 5.0, false));
        assert!(m.looping());
        let chunks: Vec<(u64, Arc<[f32]>)> = out.iter().filter_map(|c| if let Cmd::Chunk { at, data } = c { Some((*at, data.clone())) } else { None }).collect();
        // in order, from song frame 0, each where the last ended
        let start = chunks[0].0;
        let mut s = 0usize;
        for (at, d) in &chunks {
            assert_eq!(*at, start + s as u64);
            assert_eq!(d[0], s as f32);
            s += d.len() / 2;
        }
        assert_eq!(s, 2 * LAP);
        // the loop: the second lap, from where the song ends
        let (at, offset, data) = out.iter().find_map(|c| if let Cmd::Loop { at, offset, data } = c { Some((*at, *offset, data.clone())) } else { None }).unwrap();
        assert_eq!((at, offset), (start + 2 * LAP as u64, 0));
        assert_eq!(data.len() / 2, LAP);
        assert_eq!((data[0], data[2 * LAP - 2]), (LAP as f32, (2 * LAP - 1) as f32));
        assert_eq!(m.stats.late, 0);
    }

    #[test]
    fn requests_number_their_chunks_and_never_run_ahead_of_the_last() {
        let mut m = Music::new();
        let mut out = vec![];
        m.set_on(true, 0.0, RATE, &mut out);
        let a = m.want(0.0, RATE, PERIOD, &mut out).unwrap();
        assert_eq!((a.chunk, a.frames as usize), (0, FRAMES));
        // (unanswered: nothing more, until it is asked again)
        assert!(m.want(0.1, RATE, PERIOD, &mut out).is_none());
        let again = m.want(EXPIRE + 0.1, RATE, PERIOD, &mut out).unwrap();
        assert_eq!(again, a);
        m.take(a, &compute(a, 0), EXPIRE + 0.1, &mut out);
        let b = m.want(EXPIRE + 0.2, RATE, PERIOD, &mut out).unwrap();
        assert_eq!(b.chunk, 1);
    }

    #[test]
    fn a_lost_answer_is_asked_again_and_the_song_carries_on() {
        let mut m = Music::new();
        let mut out = vec![];
        m.set_on(true, 0.0, RATE, &mut out);
        let mut t = 0.0;
        out.extend(session(&mut m, &mut t, 10.0, true));
        assert!(m.looping());
        let mut s = 0usize;
        for c in &out {
            if let Cmd::Chunk { data, .. } = c {
                assert_eq!(data[0], s as f32);
                s += data.len() / 2;
            }
        }
        assert_eq!(s, 2 * LAP);
    }

    #[test]
    fn a_new_build_starts_from_chunk_0() {
        let mut m = Music::new();
        let mut out = vec![];
        m.set_on(true, 0.0, RATE, &mut out);
        let mut t = 0.0;
        session(&mut m, &mut t, 0.3, false);
        m.reset(t, RATE, &mut out);
        assert!(matches!(out.last(), Some(Cmd::Clear)));
        let r = m.want(t, RATE, PERIOD, &mut out).unwrap();
        assert_eq!(r.chunk, 0);
        assert_eq!(m.cached(), 0.0);
    }

    #[test]
    fn off_and_on_again_carries_on_the_loop() {
        let mut m = Music::new();
        let mut out = vec![];
        m.set_on(true, 0.0, RATE, &mut out);
        let mut t = 0.0;
        session(&mut m, &mut t, 5.0, false);
        assert!(m.looping());
        m.set_on(false, t, RATE, &mut out);
        m.tick(t + 1.0, RATE, &mut out);
        out.clear();
        m.set_on(true, t + 2.0, RATE, &mut out);
        assert!(out.iter().any(|c| matches!(c, Cmd::Loop { .. })));
        assert!(m.want(t + 2.0, RATE, PERIOD, &mut out).is_none());
    }
}
