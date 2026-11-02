// The audio context, shared by the music and the sound effects. Browsers
// only let a page start audio from a user gesture, so it is made on the
// first click or key press.

export class AudioHub {
  ctx: AudioContext | null = null;

  constructor() {
    const start = () => {
      this.ensure();
      void this.ctx?.resume();
    };
    window.addEventListener("pointerdown", start);
    window.addEventListener("keydown", start);
  }

  // The context (made now if need be: call from a gesture).
  ensure(): AudioContext {
    this.ctx ??= new AudioContext();
    return this.ctx;
  }

  get running(): boolean {
    return this.ctx?.state === "running";
  }

  get rate(): number {
    return this.ctx?.sampleRate ?? 48000;
  }
}
