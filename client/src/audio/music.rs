// Music scheduling (web/music.ts), apart from any device: the build's music
// is stereo samples as a pure function of loop time (native/shader's
// music, fut/music.fut), computed by the GPU a chunk at a time; this asks
// for the chunks and places them back to back on the mixer's clock.
//
// Every chunk computed is also kept, at its place in the loop, and while it
// plays the rest of the loop is computed ahead, a chunk a frame. Once the
// whole loop is there it plays from that (a looping buffer), and the GPU
// makes no more; a new build starts it afresh (reset).
//
// Requests are made once a frame, so each covers what the frames to come
// need: at a low frame rate, more chunks, further ahead. Unlike in the
// browser, where a request was read back before the next was made, a
// request may be answered a frame or more later, with others outstanding:
// what is asked for is marked so as not to be asked for again, and the
// cache is filled by one request at a time.
//
// Times: `now` is the mixer's clock in seconds; places on it are frames.

use std::sync::Arc;

use crate::types::MusicRequest;

pub const FRAMES: usize = 4096; // samples per channel per chunk (~85 ms at 48 kHz)
pub const MAX_CHUNKS: usize = 16; // the most chunks asked for at once (~1.4 s)
pub const MAX_FRAMES: usize = FRAMES * MAX_CHUNKS; // the most frames one request asks for
const LEAD: f64 = 0.35; // seconds of audio kept requested ahead of the clock, at least
const MAX_LEAD: f64 = 1.2;
const EXPIRE: f64 = 3.0; // seconds after which an unanswered request is given up
const STOP_AFTER: f64 = 0.2; // seconds from turning off (the fade) to stopping
pub const GAIN: f32 = 0.8;

// What the mixer is to do with the music.
pub enum Cmd {
    // play interleaved stereo from a frame of the clock
    Chunk { at: u64, data: Arc<[f32]> },
    // play the whole loop round and round from a frame of the clock, from
    // its frame `offset`
    Loop { at: u64, offset: usize, data: Arc<[f32]> },
    StopLoop,
    // stop the loop and drop the chunks to come
    Clear,
    // glide the music's gain to `target` (time constant `tau` seconds)
    Gain { target: f32, tau: f32 },
}

const EMPTY: u8 = 0;
const ASKED: u8 = 1;
const FILLED: u8 = 2;

// The loop as computed so far.
struct Cache {
    frames: usize,
    data: Vec<f32>, // interleaved stereo
    state: Vec<u8>, // per frame: EMPTY, ASKED or FILLED
    count: usize,   // frames filled
    cursor: usize,  // where to look for the next gap
}

impl Cache {
    fn new(frames: usize) -> Cache {
        Cache { frames, data: vec![0.0; 2 * frames], state: vec![EMPTY; frames], count: 0, cursor: 0 }
    }

    fn filled(&self, frame0: usize, n: usize) -> bool {
        (0..n).all(|j| self.state[(frame0 + j) % self.frames] == FILLED)
    }

    // Marks what is asked for (or, `undo`, no longer), where not filled.
    fn ask(&mut self, frame0: usize, n: usize, undo: bool) {
        let (from, to) = if undo { (ASKED, EMPTY) } else { (EMPTY, ASKED) };
        for j in 0..n.min(self.frames) {
            let s = &mut self.state[(frame0 + j) % self.frames];
            if *s == from {
                *s = to;
            }
        }
    }

    fn put(&mut self, frame0: usize, samples: &[f32]) {
        for (j, s) in samples.chunks_exact(2).enumerate() {
            let k = (frame0 + j) % self.frames;
            self.data[2 * k] = s[0];
            self.data[2 * k + 1] = s[1];
            if self.state[k] != FILLED {
                self.state[k] = FILLED;
                self.count += 1;
            }
        }
    }

    // The cached samples from frame0, n of them, interleaved.
    fn slice(&self, frame0: usize, n: usize) -> Arc<[f32]> {
        let mut out = Vec::with_capacity(2 * n);
        for j in 0..n {
            let k = (frame0 + j) % self.frames;
            out.extend_from_slice(&self.data[2 * k..2 * k + 2]);
        }
        out.into()
    }

    // The next stretch neither filled nor asked for (from the cursor on, up
    // to the loop's end and at most `max` frames), if any.
    fn next_gap(&mut self, max: usize) -> Option<(usize, usize)> {
        if self.count == self.frames {
            return None;
        }
        while self.state[self.cursor] == FILLED {
            self.cursor = (self.cursor + 1) % self.frames;
        }
        let gap = (0..self.frames).map(|j| (self.cursor + j) % self.frames).find(|&k| self.state[k] == EMPTY)?;
        let n = (gap..self.frames.min(gap + max)).take_while(|&k| self.state[k] == EMPTY).count();
        Some((gap, n))
    }
}

// A request made and not yet answered.
struct Pending {
    req: MusicRequest,
    frame0: usize,
    frames: usize,
    // where it plays on the clock (None: only filling the cache)
    at: Option<u64>,
    // the build (see reset) and when it was asked
    build: u32,
    asked: f64,
}

// The cached loop playing: from frame `at` of the clock, from the loop's
// frame `frame`.
struct Looping {
    at: u64,
    frame: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    // requests played, chunks (of them or from the cache) that came late
    pub played: u32,
    pub late: u32,
}

pub struct Music {
    pub on: bool,
    pub stats: Stats,
    cache: Option<Cache>,
    looping: Option<Looping>,
    song_frame: u64, // where in the song the next chunk to play starts
    queued: u64,     // the frame of the clock up to which chunks are scheduled
    frame_time: f64, // seconds between frames, smoothed
    last_asked: Option<f64>,
    pending: Vec<Pending>,
    build: u32,
    // turned off: when to stop (after the fade)
    stop_at: Option<f64>,
}

impl Default for Music {
    fn default() -> Self {
        Self::new()
    }
}

impl Music {
    pub fn new() -> Music {
        Music {
            on: false,
            stats: Stats::default(),
            cache: None,
            looping: None,
            song_frame: 0,
            queued: 0,
            frame_time: 1.0 / 60.0,
            last_asked: None,
            pending: Vec::new(),
            build: 0,
            stop_at: None,
        }
    }

    // How much of the loop is cached (0..1).
    pub fn cached(&self) -> f32 {
        self.cache.as_ref().map_or(0.0, |c| c.count as f32 / c.frames as f32)
    }

    #[cfg(test)]
    pub fn looping(&self) -> bool {
        self.looping.is_some()
    }

    // A new build: its music may differ, so forget the cached loop (chunks
    // still to come from the old one play but are not kept).
    pub fn reset(&mut self, now: f64, rate: u32, out: &mut Vec<Cmd>) {
        if self.stop_loop(now, rate) {
            out.push(Cmd::StopLoop);
        }
        self.cache = None;
        self.build += 1;
    }

    pub fn set_on(&mut self, on: bool, now: f64, rate: u32, out: &mut Vec<Cmd>) {
        if on == self.on {
            return;
        }
        self.on = on;
        if on {
            // (still fading out: carry on with what is queued)
            if self.stop_at.take().is_none() {
                self.queued = frame_at(now + 0.1, rate);
            }
            self.last_asked = None;
            out.push(Cmd::Gain { target: GAIN, tau: 0.05 });
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
            self.stop_loop(now, rate);
            out.push(Cmd::Clear);
            // (what is still to come is no longer to play)
            for p in &mut self.pending {
                p.at = None;
            }
        }
    }

    // What to compute next, if anything (and reserves it if it is to play):
    // the next stretch to play, unless it is cached (then it is scheduled
    // from the cache); else the next gap in the cache. `period` is the
    // music's loop length in seconds, `rate` the output's.
    pub fn want(&mut self, now: f64, rate: u32, period: f32, out: &mut Vec<Cmd>) -> Option<MusicRequest> {
        self.expire(now);
        if !self.on || period <= 0.0 || rate == 0 || self.looping.is_some() {
            return None;
        }
        let r = rate as f64;
        let frames = (period as f64 * r).round() as usize;
        if self.cache.as_ref().is_none_or(|c| c.frames != frames) {
            self.cache = Some(Cache::new(frames));
            self.build += 1;
        }
        if self.cache.as_ref().is_some_and(|c| c.count == frames) {
            let at = self.queued.max(frame_at(now + 0.05, rate));
            self.start_loop(at, out);
            return None;
        }
        // (the time between frames, on the audio clock: capped)
        if let Some(t) = self.last_asked {
            self.frame_time += 0.2 * ((now - t).min(0.5) - self.frame_time);
        }
        self.last_asked = Some(now);
        let lead: f64 = (2.5 * self.frame_time + 0.1).clamp(LEAD, MAX_LEAD);
        let nowf = now * r;
        // fell behind (e.g. the app was hidden): start again just ahead
        if (self.queued as f64) < nowf + 0.02 * r {
            self.queued = frame_at(now + 0.1, rate);
        }
        while self.queued as f64 - nowf <= lead * r {
            let need = (nowf + lead * r - self.queued as f64) / FRAMES as f64;
            let chunks = (need.ceil() as usize).clamp(1, MAX_CHUNKS);
            let n = chunks * FRAMES;
            let frame0 = (self.song_frame % frames as u64) as usize;
            let at = self.queued;
            self.song_frame += n as u64;
            self.queued += n as u64;
            let cache = self.cache.as_mut().unwrap();
            if !cache.filled(frame0, n) {
                cache.ask(frame0, n, false);
                return Some(self.issue(frame0, n, Some(at), now, rate));
            }
            out.push(Cmd::Chunk { at, data: cache.slice(frame0, n) });
        }
        // nothing to play yet: fill the next gap (one request at a time)
        if self.pending.iter().any(|p| p.at.is_none() && p.build == self.build) {
            return None;
        }
        let cache = self.cache.as_mut().unwrap();
        let (gap, n) = cache.next_gap(MAX_FRAMES)?;
        cache.ask(gap, n, false);
        Some(self.issue(gap, n, None, now, rate))
    }

    // Takes computed samples (interleaved stereo) for a request: keeps
    // them, and plays them if they were asked for to play.
    pub fn take(&mut self, req: MusicRequest, samples: &[f32], now: f64, out: &mut Vec<Cmd>) {
        let Some(i) = self.pending.iter().position(|p| p.req == req) else {
            log::debug!("music: a chunk not asked for (or given up): {req:?}");
            return;
        };
        let p = self.pending.remove(i);
        let n = p.frames.min(samples.len() / 2);
        let samples = &samples[..2 * n];
        if p.build == self.build {
            if let Some(cache) = &mut self.cache {
                cache.put(p.frame0, samples);
                cache.ask(p.frame0, p.frames, true);
            }
        }
        if let (Some(at), true) = (p.at, self.on) {
            self.stats.played += 1;
            if (at as f64) < now * req.rate as f64 {
                self.stats.late += 1;
            }
            out.push(Cmd::Chunk { at, data: samples.into() });
        }
    }

    fn issue(&mut self, frame0: usize, frames: usize, at: Option<u64>, now: f64, rate: u32) -> MusicRequest {
        let req = MusicRequest { t0: (frame0 as f64 / rate as f64) as f32, rate: rate as f32, frames: frames as u32 };
        self.pending.push(Pending { req, frame0, frames, at, build: self.build, asked: now });
        req
    }

    // Gives up requests long unanswered (so their stretch is asked again).
    fn expire(&mut self, now: f64) {
        let (build, cache) = (self.build, &mut self.cache);
        self.pending.retain(|p| {
            let keep = now - p.asked < EXPIRE;
            if !keep && p.build == build {
                if let Some(c) = cache.as_mut() {
                    c.ask(p.frame0, p.frames, true);
                }
            }
            keep
        });
    }

    // Plays the cached loop on from where the song has got to, from `at`.
    fn start_loop(&mut self, at: u64, out: &mut Vec<Cmd>) {
        let cache = self.cache.as_ref().unwrap();
        let frame = (self.song_frame % cache.frames as u64) as usize;
        out.push(Cmd::Loop { at, offset: frame, data: cache.data.clone().into() });
        self.looping = Some(Looping { at, frame });
    }

    // Forgets the loop playing (the song carries on from where it had got
    // to); whether there was one.
    fn stop_loop(&mut self, now: f64, rate: u32) -> bool {
        let Some(l) = self.looping.take() else { return false };
        let played = (now * rate as f64 - l.at as f64).max(0.0);
        self.song_frame = l.frame as u64 + played.round() as u64;
        true
    }
}

fn frame_at(t: f64, rate: u32) -> u64 {
    (t * rate as f64).round().max(0.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48000;

    // The music as the GPU would make it: frame k of the loop is k (left)
    // and -k (right).
    fn compute(req: MusicRequest, frames: usize) -> Vec<f32> {
        let f0 = (req.t0 as f64 * req.rate as f64).round() as usize;
        (0..req.frames as usize).flat_map(|j| {
            let k = ((f0 + j) % frames) as f32;
            [k, -k]
        })
        .collect()
    }

    fn chunks(out: &[Cmd]) -> Vec<(u64, usize, f32)> {
        out.iter()
            .filter_map(|c| match c {
                Cmd::Chunk { at, data } => Some((*at, data.len() / 2, data[0])),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn off_asks_nothing() {
        let mut m = Music::new();
        let mut out = vec![];
        assert_eq!(m.want(0.0, RATE, 10.0, &mut out), None);
    }

    #[test]
    fn first_requests_cover_the_lead_back_to_back() {
        let mut m = Music::new();
        let mut out = vec![];
        m.set_on(true, 1.0, RATE, &mut out);
        let req = m.want(1.0, RATE, 10.0, &mut out).unwrap();
        // from 0.1 s ahead to at least LEAD ahead, in whole chunks
        assert_eq!(req.t0, 0.0);
        assert_eq!(req.rate, RATE as f32);
        let need = ((LEAD - 0.1) * RATE as f64 / FRAMES as f64).ceil() as u32;
        assert_eq!(req.frames, need * FRAMES as u32);
        // answered: placed at 1.1 s
        m.take(req, &compute(req, 480000), 1.0, &mut out);
        let c = chunks(&out);
        assert_eq!(c, vec![(frame_at(1.1, RATE), req.frames as usize, 0.0)]);
        // the next frame: nothing to play yet, so the cache's next gap
        // (at the same time: no more to play yet)
        let req2 = m.want(1.0, RATE, 10.0, &mut out).unwrap();
        assert_eq!(req2.t0, req.frames as f32 / RATE as f32);
        assert_eq!(req2.frames as usize, MAX_FRAMES);
        // (one cache fill at a time)
        assert_eq!(m.want(1.0, RATE, 10.0, &mut out), None);
        // as time goes on, the next chunk to play follows on from the first
        let t = 1.1 + req.frames as f64 / RATE as f64 - LEAD + 0.01;
        let req3 = m.want(t, RATE, 10.0, &mut out).unwrap();
        // (the gap fill covers it, but is not yet answered: asked again, to play)
        assert_eq!(req3.t0, req2.t0);
        assert_eq!(req3.frames as usize, FRAMES);
        out.clear();
        m.take(req3, &compute(req3, 480000), t, &mut out);
        assert_eq!(chunks(&out), vec![(frame_at(1.1, RATE) + req.frames as u64, FRAMES, req.frames as f32)]);
    }

    #[test]
    fn low_frame_rate_asks_further_ahead() {
        let mut m = Music::new();
        let mut out = vec![];
        m.set_on(true, 0.0, RATE, &mut out);
        let mut t = 0.0;
        let mut ahead: f64 = 0.0;
        for _ in 0..20 {
            if let Some(req) = m.want(t, RATE, 100.0, &mut out) {
                m.take(req, &compute(req, 480000), t, &mut out);
            }
            ahead = m.queued as f64 / RATE as f64 - t;
            t += 0.25; // 4 frames a second
        }
        // lead = 2.5 * 0.25 + 0.1
        assert!(ahead > 0.7, "{ahead}");
        assert_eq!(m.stats.late, 0);
    }

    // Plays a session at 60 frames a second, answering each request a
    // frame later; returns the chunks scheduled.
    fn session(m: &mut Music, t: &mut f64, secs: f64, period: f32, out: &mut Vec<Cmd>) {
        let frames = (period as f64 * RATE as f64).round() as usize;
        let mut waiting: Option<MusicRequest> = None;
        let end = *t + secs;
        while *t < end {
            if let Some(req) = waiting.take() {
                m.take(req, &compute(req, frames), *t, out);
            }
            m.tick(*t, RATE, out);
            waiting = m.want(*t, RATE, period, out);
            *t += 1.0 / 60.0;
        }
    }

    #[test]
    fn chunks_are_gapless_and_in_loop_order_and_the_loop_gets_cached() {
        let period = 3.0;
        let frames = (period as f64 * RATE as f64) as u64;
        let mut m = Music::new();
        let mut out = vec![];
        let mut t = 0.5;
        m.set_on(true, t, RATE, &mut out);
        session(&mut m, &mut t, 4.0, period, &mut out);
        // the whole loop is there: playing from the cache
        assert!(m.looping());
        assert_eq!(m.cached(), 1.0);
        assert_eq!(m.stats.late, 0);
        let mut c = chunks(&out);
        c.sort_by_key(|c| c.0);
        // back to back, each where the song had got to (wrapping round)
        for w in c.windows(2) {
            assert_eq!(w[0].0 + w[0].1 as u64, w[1].0, "{w:?}");
            assert_eq!((w[0].2 as u64 + w[0].1 as u64) % frames, w[1].2 as u64);
        }
        let (last_at, last_n, last_v) = *c.last().unwrap();
        let Some(Cmd::Loop { at, offset, data }) = out.iter().find(|c| matches!(c, Cmd::Loop { .. })) else {
            panic!("no loop")
        };
        assert_eq!(*at, last_at + last_n as u64);
        assert_eq!(*offset as u64, (last_v as u64 + last_n as u64) % frames);
        // the cache holds the loop, sample for sample
        assert!(data.chunks(2).enumerate().all(|(k, s)| s[0] == k as f32 && s[1] == -(k as f32)));
        // and no more requests
        out.clear();
        assert_eq!(m.want(t, RATE, period, &mut out), None);
        assert!(out.is_empty());
    }

    #[test]
    fn wraps_round_the_loops_end() {
        // a loop shorter than the lead: requests run past its end and are
        // kept modulo it
        let period = 0.2;
        let frames = (period as f64 * RATE as f64) as usize;
        let mut m = Music::new();
        let mut out = vec![];
        m.set_on(true, 0.0, RATE, &mut out);
        let req = m.want(0.0, RATE, period, &mut out).unwrap();
        assert!(req.frames as usize > frames);
        m.take(req, &compute(req, frames), 0.0, &mut out);
        assert_eq!(m.cached(), 1.0);
        let req2 = m.want(0.3, RATE, period, &mut out);
        assert_eq!(req2, None);
        let Some(Cmd::Loop { offset, .. }) = out.last() else { panic!() };
        assert_eq!(*offset, req.frames as usize % frames);
    }

    #[test]
    fn plays_from_the_cache_when_cached() {
        let period = 10.0;
        let frames = 10 * RATE as usize;
        let mut m = Music::new();
        let mut out = vec![];
        m.set_on(true, 0.0, RATE, &mut out);
        // fill the cache from 0 .. 0.5 s only by hand: the first request
        let req = m.want(0.0, RATE, period, &mut out).unwrap();
        m.take(req, &compute(req, frames), 0.0, &mut out);
        let gap = m.want(0.0, RATE, period, &mut out).unwrap();
        assert_eq!(gap.t0, req.frames as f32 / RATE as f32);
        m.take(gap, &compute(gap, frames), 0.0, &mut out);
        // turn off and on: the song carries on, now from the cache
        m.set_on(false, 0.02, RATE, &mut out);
        m.tick(0.3, RATE, &mut out);
        assert!(matches!(out.last(), Some(Cmd::Clear)));
        m.set_on(true, 0.3, RATE, &mut out);
        out.clear();
        let next = m.want(0.3, RATE, period, &mut out);
        // (from where the song had got to, cached by the gap fill)
        assert_eq!(chunks(&out), vec![(frame_at(0.4, RATE), req.frames as usize, req.frames as f32)]);
        // then the next gap
        let next = next.unwrap();
        assert_eq!(next.t0, ((req.frames + gap.frames) as f64 / RATE as f64) as f32);
    }

    #[test]
    fn reset_starts_afresh() {
        let period = 0.5;
        let mut m = Music::new();
        let mut out = vec![];
        let mut t = 0.0;
        m.set_on(true, t, RATE, &mut out);
        session(&mut m, &mut t, 2.0, period, &mut out);
        assert!(m.looping());
        out.clear();
        m.reset(t, RATE, &mut out);
        assert!(matches!(out[0], Cmd::StopLoop));
        assert_eq!(m.cached(), 0.0);
        assert!(m.want(t, RATE, period, &mut out).is_some());
    }

    #[test]
    fn unanswered_requests_are_asked_again() {
        let mut m = Music::new();
        let mut out = vec![];
        m.set_on(true, 0.0, RATE, &mut out);
        let a = m.want(0.0, RATE, 10.0, &mut out).unwrap();
        m.take(a, &compute(a, 480000), 0.0, &mut out);
        let gap = m.want(0.0, RATE, 10.0, &mut out).unwrap();
        assert_eq!(m.want(0.0, RATE, 10.0, &mut out), None);
        // never answered: given up, and asked again
        let again = m.want(EXPIRE + 0.01, RATE, 10.0, &mut out);
        assert!(again.is_some());
        assert!(m.pending.iter().all(|p| p.req != gap));
        let mut t = EXPIRE + 0.01;
        let mut asked = vec![];
        while let Some(r) = m.want(t, RATE, 10.0, &mut out) {
            asked.push(r);
            t += 0.001;
        }
        assert!(asked.iter().chain(&again).any(|r| r.t0 == gap.t0), "{asked:?}");
    }
}
