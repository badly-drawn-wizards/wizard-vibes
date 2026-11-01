// Music: the build's `sound` entry point computes stereo samples as a pure
// function of time (see fut/music.fut), a chunk at a time; this schedules
// the chunks back to back on the Web Audio clock. Off by default; the ♪
// button (or M) turns it on.
//
// Time is kept here in double precision and wrapped to the music's loop
// before going to the GPU as f32: minutes in, an f32 time could no longer
// tell one sample from the next.

const FRAMES = 4096; // samples per channel per chunk (~85 ms at 48 kHz)
const MAX_CHUNKS = 16; // the most chunks asked for at once (~1.4 s)
const LEAD = 0.35; // seconds of audio kept requested ahead of the clock, at least

export interface ChunkRequest {
  t0: number; // start within the loop, in seconds
  at: number; // when it plays, on the audio clock
  frames: number; // samples per channel
}

export class Music {
  on = false;
  // (the most samples per channel one request asks for)
  readonly maxFrames = FRAMES * MAX_CHUNKS;
  // (for tests: requests played, ones that came too late, the seconds of
  // audio played, the loudest sample)
  readonly stats = { played: 0, late: 0, seconds: 0, peak: 0 };
  private ctx: AudioContext | null = null;
  private gain: GainNode | null = null;
  private songTime = 0; // music time of the next chunk to request
  private frameTime = 1 / 60; // seconds between frames, smoothed
  private lastAsked = 0; // when want() was last called, on the audio clock
  private queuedUntil = 0; // audio-clock time up to which chunks are requested
  private button: HTMLButtonElement;

  constructor(button: HTMLButtonElement) {
    this.button = button;
    button.onclick = () => this.toggle();
    this.render();
  }

  get rate(): number {
    return this.ctx?.sampleRate ?? 48000;
  }

  // (unavailable: e.g. the WebGL fallback has no compute to make samples)
  disable(reason: string): void {
    this.button.disabled = true;
    this.button.title = reason;
  }

  toggle(): void {
    if (this.button.disabled) return;
    this.on = !this.on;
    if (this.on) {
      // (created on the first click: browsers only let a gesture start audio)
      if (!this.ctx) {
        this.ctx = new AudioContext();
        this.gain = this.ctx.createGain();
        this.gain.connect(this.ctx.destination);
      }
      void this.ctx.resume();
      this.queuedUntil = this.ctx.currentTime + 0.1;
      this.lastAsked = 0;
      this.gain!.gain.setTargetAtTime(0.8, this.ctx.currentTime, 0.05);
    } else if (this.ctx) {
      // fade out, then stop the clock
      this.gain!.gain.setTargetAtTime(0, this.ctx.currentTime, 0.03);
      const ctx = this.ctx;
      setTimeout(() => !this.on && void ctx.suspend(), 200);
    }
    this.render();
  }

  // The next chunks to compute, if more audio is wanted now (and reserves
  // them): `period` is the music's loop length. It is asked once a frame,
  // so each request covers what the frames to come need: at a low frame
  // rate, more chunks, further ahead.
  want(period: number): ChunkRequest | null {
    const ctx = this.ctx;
    if (!this.on || !ctx || ctx.state !== "running" || period <= 0) return null;
    const now = ctx.currentTime;
    // (the time between frames, on the audio clock: the frame loop's own
    // step is capped)
    if (this.lastAsked > 0) this.frameTime += 0.2 * (Math.min(now - this.lastAsked, 0.5) - this.frameTime);
    this.lastAsked = now;
    const lead = Math.min(1.2, Math.max(LEAD, 2.5 * this.frameTime + 0.1));
    // fell behind (e.g. the tab was hidden): start again just ahead
    if (this.queuedUntil < now + 0.02) this.queuedUntil = now + 0.1;
    if (this.queuedUntil - now > lead) return null;
    const dur = FRAMES / ctx.sampleRate;
    const chunks = Math.min(MAX_CHUNKS, Math.max(1, Math.ceil((now + lead - this.queuedUntil) / dur)));
    const req = { t0: this.songTime % period, at: this.queuedUntil, frames: chunks * FRAMES };
    this.songTime += chunks * dur;
    this.queuedUntil += chunks * dur;
    return req;
  }

  // Plays a computed chunk (interleaved stereo) at its time.
  play(samples: Float32Array, req: ChunkRequest): void {
    const ctx = this.ctx;
    if (!ctx || !this.on) return;
    const n = req.frames;
    // (arrived late: play what is still to come)
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
    this.stats.played++;
    this.stats.seconds += n / ctx.sampleRate - Math.max(0, late);
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.connect(this.gain!);
    if (late > 0) src.start(ctx.currentTime, late);
    else src.start(req.at);
  }

  private render(): void {
    this.button.setAttribute("aria-pressed", String(this.on));
    if (!this.button.disabled) this.button.title = this.on ? "Music on (M)" : "Music off (M)";
    this.button.textContent = this.on ? "♪ on" : "♪ off";
  }
}
