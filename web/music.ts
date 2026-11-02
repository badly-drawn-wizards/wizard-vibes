// Music: the build's `sound` entry point computes stereo samples as a pure
// function of time (see fut/music.fut), a chunk at a time; this schedules
// the chunks back to back on the Web Audio clock. Off by default; the ♪
// button (or M) turns it on.
//
// Every chunk computed is also kept, at its place in the loop, and while it
// plays the rest of the loop is computed ahead, a chunk a frame. Once the
// whole loop is there it plays from that (a looping buffer), and the GPU
// makes no more; a new build starts it afresh (reset).
//
// Requests are made once a frame, so each covers what the frames to come
// need: at a low frame rate, more chunks, further ahead.

import type { AudioHub } from "./audio.ts";

const FRAMES = 4096; // samples per channel per chunk (~85 ms at 48 kHz)
const MAX_CHUNKS = 16; // the most chunks asked for at once (~1.4 s)
const LEAD = 0.35; // seconds of audio kept requested ahead of the clock, at least

export interface ChunkRequest {
  t0: number; // start within the loop, in seconds
  frame0: number; // the same, in samples
  frames: number; // samples per channel
  at: number; // when it plays, on the audio clock (if `play`)
  play: boolean; // false: only filling the cache
}

// The loop as computed so far.
interface Cache {
  frames: number;
  l: Float32Array<ArrayBuffer>;
  r: Float32Array<ArrayBuffer>;
  filled: Uint8Array; // per sample
  count: number; // samples filled
  cursor: number; // where to look for the next gap
}

export class Music {
  on = false;
  // (the most samples per channel one request asks for)
  readonly maxFrames = FRAMES * MAX_CHUNKS;
  // (for tests: requests played, ones that came too late, the seconds of
  // audio played from the GPU, the loudest sample)
  readonly stats = { played: 0, late: 0, seconds: 0, peak: 0 };
  private audio: AudioHub;
  private gain: GainNode | null = null;
  private button: HTMLButtonElement;
  private cache: Cache | null = null;
  private loop: { src: AudioBufferSourceNode; at: number; frame: number } | null = null;
  private songFrame = 0; // where in the song the next chunk to play starts
  private queuedUntil = 0; // audio-clock time up to which chunks are scheduled
  private frameTime = 1 / 60; // seconds between frames, smoothed
  private lastAsked = 0; // when want() was last called, on the audio clock

  constructor(button: HTMLButtonElement, audio: AudioHub) {
    this.button = button;
    this.audio = audio;
    button.onclick = () => this.toggle();
    this.render();
  }

  get rate(): number {
    return this.audio.rate;
  }

  // How much of the loop is cached (0..1).
  get cached(): number {
    return this.cache ? this.cache.count / this.cache.frames : 0;
  }

  // (unavailable: e.g. the WebGL fallback has no compute to make samples)
  disable(reason: string): void {
    this.button.disabled = true;
    this.button.title = reason;
  }

  // A new build: its music may differ, so forget the cached loop.
  reset(): void {
    this.stopLoop();
    this.cache = null;
  }

  toggle(): void {
    if (this.button.disabled) return;
    this.on = !this.on;
    const ctx = this.audio.ensure();
    if (!this.gain) {
      this.gain = ctx.createGain();
      this.gain.connect(ctx.destination);
    }
    if (this.on) {
      void ctx.resume();
      this.queuedUntil = ctx.currentTime + 0.1;
      this.lastAsked = 0;
      this.gain.gain.setTargetAtTime(0.8, ctx.currentTime, 0.05);
    } else {
      // fade out, then stop
      this.gain.gain.setTargetAtTime(0, ctx.currentTime, 0.03);
      setTimeout(() => !this.on && this.stopLoop(), 200);
    }
    this.render();
  }

  // What to compute next, if anything (and reserves it if it is to play):
  // the next stretch to play, unless it is cached (then it is scheduled
  // from the cache); else the next gap in the cache. `period` is the
  // music's loop length.
  want(period: number): ChunkRequest | null {
    const ctx = this.audio.ctx;
    if (!this.on || !ctx || ctx.state !== "running" || period <= 0 || this.loop) return null;
    const rate = ctx.sampleRate;
    const frames = Math.round(period * rate);
    if (this.cache?.frames !== frames) this.cache = newCache(frames);
    const cache = this.cache;
    const now = ctx.currentTime;
    if (cache.count === frames) {
      this.startLoop(Math.max(this.queuedUntil, now + 0.05));
      return null;
    }
    // (the time between frames, on the audio clock: the frame loop's own
    // step is capped)
    if (this.lastAsked > 0) this.frameTime += 0.2 * (Math.min(now - this.lastAsked, 0.5) - this.frameTime);
    this.lastAsked = now;
    const lead = Math.min(1.2, Math.max(LEAD, 2.5 * this.frameTime + 0.1));
    // fell behind (e.g. the tab was hidden): start again just ahead
    if (this.queuedUntil < now + 0.02) this.queuedUntil = now + 0.1;
    const dur = FRAMES / rate;
    while (this.queuedUntil - now <= lead) {
      const chunks = Math.min(MAX_CHUNKS, Math.max(1, Math.ceil((now + lead - this.queuedUntil) / dur)));
      const n = chunks * FRAMES;
      const frame0 = this.songFrame % frames;
      const req = { t0: frame0 / rate, frame0, frames: n, at: this.queuedUntil, play: true };
      this.songFrame += n;
      this.queuedUntil += n / rate;
      if (!isCached(cache, frame0, n)) return req;
      this.schedule(cacheSlice(cache, frame0, n), req);
    }
    // nothing to play yet: fill the next gap
    const gap = nextGap(cache);
    const n = Math.min(this.maxFrames, frames - gap);
    return { t0: gap / rate, frame0: gap, frames: n, at: 0, play: false };
  }

  // Takes computed samples (interleaved stereo): keeps them, and plays them
  // if they were asked for to play.
  play(samples: Float32Array, req: ChunkRequest): void {
    const cache = this.cache;
    if (cache) {
      for (let j = 0; j < req.frames; j++) {
        const k = (req.frame0 + j) % cache.frames;
        cache.l[k] = samples[2 * j];
        cache.r[k] = samples[2 * j + 1];
        if (!cache.filled[k]) {
          cache.filled[k] = 1;
          cache.count++;
        }
      }
    }
    if (req.play && this.on) {
      this.stats.played++;
      this.schedule(samples, req);
    }
  }

  // Schedules interleaved samples to play at req.at (what is still to come,
  // if late).
  private schedule(samples: Float32Array, req: ChunkRequest): void {
    const ctx = this.audio.ctx;
    if (!ctx || !this.gain) return;
    const n = req.frames;
    const late = ctx.currentTime - req.at;
    if (late > 0) this.stats.late++;
    if (late >= n / ctx.sampleRate) return;
    const buf = ctx.createBuffer(2, n, ctx.sampleRate);
    const l = buf.getChannelData(0), r = buf.getChannelData(1);
    for (let i = 0; i < n; i++) {
      l[i] = samples[2 * i];
      r[i] = samples[2 * i + 1];
      this.stats.peak = Math.max(this.stats.peak, Math.abs(l[i]), Math.abs(r[i]));
    }
    this.stats.seconds += n / ctx.sampleRate - Math.max(0, late);
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.connect(this.gain);
    if (late > 0) src.start(ctx.currentTime, late);
    else src.start(req.at);
  }

  // Plays the cached loop on from where the song has got to, from `at`.
  private startLoop(at: number): void {
    const ctx = this.audio.ctx!;
    const cache = this.cache!;
    const buf = ctx.createBuffer(2, cache.frames, ctx.sampleRate);
    buf.copyToChannel(cache.l, 0);
    buf.copyToChannel(cache.r, 1);
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.loop = true;
    src.connect(this.gain!);
    const frame = this.songFrame % cache.frames;
    src.start(at, frame / ctx.sampleRate);
    this.loop = { src, at, frame };
  }

  private stopLoop(): void {
    const ctx = this.audio.ctx;
    if (!this.loop || !ctx) return;
    // (the song carries on from where the loop had got to)
    const played = Math.max(0, ctx.currentTime - this.loop.at) * ctx.sampleRate;
    this.songFrame = Math.round(this.loop.frame + played);
    this.loop.src.stop();
    this.loop = null;
  }

  private render(): void {
    this.button.setAttribute("aria-pressed", String(this.on));
    if (!this.button.disabled) this.button.title = this.on ? "Music on (M)" : "Music off (M)";
    this.button.textContent = this.on ? "♪ on" : "♪ off";
  }
}

function newCache(frames: number): Cache {
  return { frames, l: new Float32Array(frames), r: new Float32Array(frames), filled: new Uint8Array(frames), count: 0, cursor: 0 };
}

function isCached(c: Cache, frame0: number, n: number): boolean {
  for (let j = 0; j < n; j++) if (!c.filled[(frame0 + j) % c.frames]) return false;
  return true;
}

// The cached samples from frame0, n of them, interleaved.
function cacheSlice(c: Cache, frame0: number, n: number): Float32Array {
  const out = new Float32Array(2 * n);
  for (let j = 0; j < n; j++) {
    const k = (frame0 + j) % c.frames;
    out[2 * j] = c.l[k];
    out[2 * j + 1] = c.r[k];
  }
  return out;
}

// The first sample not yet cached, from the cursor on (there is one).
function nextGap(c: Cache): number {
  while (c.filled[c.cursor]) c.cursor = (c.cursor + 1) % c.frames;
  return c.cursor;
}
