// The audio (web/audio.ts, music.ts and the playing side of sfx.ts): one
// output, shared by the music and the sound effects.
//
// Browsers only let a page start audio after a user gesture, so in the
// browser the output is opened on the first click or key press (seen in
// egui's input; the page's activation then lets it start); natively it is
// opened at once. No device is not an error: the buttons are disabled,
// saying why.
//
// Music (music.rs): off by default; the music button (or M, via toggle_music)
// turns it on. Its samples come from the GPU in chunks the app is asked for
// once a frame (music_request, answered by music_chunk).
//
// Sound effects: each build hands over its effects' samples (set_build);
// play() plays one, panned and at a gain. The "fx" button turns them off
// and on (remembered).

mod mixer;
mod music;
mod output;

use std::sync::Arc;

use crate::types::{BuildInfo, MusicRequest, SfxEvent};
use music::{Cmd, Music};
use output::Output;

const STORE_SFX: &str = "wizard-sfx";
const STORE_SONG: &str = "wizard-song";

pub struct Audio {
    out: Option<Output>,
    // why there is no audio, if there is none
    failed: Option<String>,
    music: Music,
    sfx_on: bool,
    // the song playing (see wizard_shader::music::songs), and the build's
    // effects (interleaved stereo at sfx_rate, and at the output's rate)
    song: u32,
    sfx_rate: f32,
    sfx_src: Vec<Vec<f32>>,
    sfx: Vec<Arc<[f32]>>,
    cmds: Vec<Cmd>,
}

impl Audio {
    pub fn new(storage: Option<&dyn eframe::Storage>) -> Audio {
        let sfx_on = storage.and_then(|s| s.get_string(STORE_SFX)).is_none_or(|v| v != "0");
        let song = storage.and_then(|s| s.get_string(STORE_SONG)).and_then(|v| v.parse().ok()).filter(|&k: &u32| (k as i32) < wizard_shader::music::songs).unwrap_or(0);
        #[allow(unused_mut)]
        let mut audio = Audio {
            out: None,
            failed: None,
            music: Music::new(),
            sfx_on,
            song,
            sfx_rate: 0.0,
            sfx_src: Vec::new(),
            sfx: Vec::new(),
            cmds: Vec::new(),
        };
        #[cfg(not(target_arch = "wasm32"))]
        audio.start();
        audio
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string(STORE_SFX, if self.sfx_on { "1" } else { "0" }.into());
        storage.set_string(STORE_SONG, self.song.to_string());
    }

    // Call each frame: starts the output on the first user input, and stops
    // the music once it has faded out.
    pub fn frame(&mut self, ctx: &egui::Context) {
        let gesture = ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::PointerButton { pressed: true, .. }
                        | egui::Event::Key { pressed: true, .. }
                        | egui::Event::Touch { phase: egui::TouchPhase::Start, .. }
                )
            })
        });
        if gesture {
            self.start();
        }
        if let Some(out) = &mut self.out {
            let now = out.now();
            self.music.tick(now, out.rate, &mut self.cmds);
            self.flush();
        }
    }

    // A new build: its music and effects.
    pub fn set_build(&mut self, info: &BuildInfo, sfx: Vec<Vec<f32>>) {
        self.sfx_rate = info.sfx_rate;
        self.sfx_src = sfx;
        self.sfx.clear();
        if let Some(out) = &mut self.out {
            let now = out.now();
            self.music.reset(now, out.rate, &mut self.cmds);
            self.flush();
        } else {
            self.music.reset(0.0, 0, &mut self.cmds);
            self.cmds.clear();
        }
        self.resample_sfx();
    }

    // The music to compute this frame, if any: `frames` stereo frames from
    // `t0` into the loop at `rate`, to hand back to music_chunk.
    pub fn music_request(&mut self) -> Option<MusicRequest> {
        let out = self.out.as_mut()?;
        let now = out.now();
        let req = self.music.want(now, out.rate, self.song, wizard_shader::music::song_period(self.song as i32), &mut self.cmds);
        self.flush();
        req
    }

    // The samples (interleaved stereo) for a request.
    pub fn music_chunk(&mut self, req: MusicRequest, samples: &[f32]) {
        let Some(out) = &mut self.out else { return };
        let now = out.now();
        self.music.take(req, samples, now, &mut self.cmds);
        self.flush();
    }

    pub fn play(&mut self, ev: SfxEvent) {
        let (Some(out), true) = (&self.out, self.sfx_on) else { return };
        if ev.gain < 0.02 {
            return;
        }
        if let Some(data) = self.sfx.get(ev.kind) {
            out.with(|m| m.play(data.clone(), ev.pan, ev.gain));
        }
    }

    pub fn toggle_music(&mut self) {
        // (always from a user gesture)
        self.start();
        let Some(out) = &mut self.out else { return };
        let now = out.now();
        self.music.set_on(!self.music.on, now, out.rate, &mut self.cmds);
        self.flush();
    }

    pub fn music_on(&self) -> bool {
        self.music.on
    }

    // How much of the music's loop is cached (0..1).
    pub fn music_cached(&self) -> f32 {
        self.music.cached()
    }

    // The music and fx buttons, for the top bar.
    pub fn buttons(&mut self, ui: &mut egui::Ui) {
        let off = self.failed.as_ref().map(|e| format!("No sound: {e}"));
        let on = self.music.on;
        let r = ui.add_enabled(off.is_none(), egui::Button::new(if on { "music on" } else { "music off" }).selected(on));
        let r = match &off {
            Some(e) => r.on_disabled_hover_text(e),
            None => r.on_hover_text(if on { "Music on (M)" } else { "Music off (M)" }),
        };
        if r.clicked() {
            self.toggle_music();
        }
        // (the songs: a switch computes the new one from its start, unless
        // its loop is kept from before)
        let mut song = self.song;
        egui::ComboBox::from_id_salt("song").selected_text(wizard_shader::music::song_names[song as usize]).show_ui(ui, |ui| {
            for (k, name) in wizard_shader::music::song_names.iter().enumerate() {
                ui.selectable_value(&mut song, k as u32, *name);
            }
        });
        self.song = song;
        let on = self.sfx_on && off.is_none();
        let r = ui.add_enabled(off.is_none(), egui::Button::new(if on { "fx on" } else { "fx off" }).selected(on));
        let r = match &off {
            Some(e) => r.on_disabled_hover_text(e),
            None => r.on_hover_text(if on { "Sound effects on" } else { "Sound effects off" }),
        };
        if r.clicked() {
            self.start();
            self.sfx_on = !self.sfx_on;
        }
    }

    // Opens the output if need be (or resumes it).
    fn start(&mut self) {
        if let Some(out) = &self.out {
            #[cfg(target_arch = "wasm32")]
            out.resume();
            let _ = out;
            return;
        }
        if self.failed.is_some() {
            return;
        }
        match Output::open() {
            Ok(out) => {
                self.out = Some(out);
                self.resample_sfx();
            }
            Err(e) => {
                log::warn!("audio: {e} (no music or sound effects)");
                self.failed = Some(e);
            }
        }
    }

    // The effects at the output's rate.
    fn resample_sfx(&mut self) {
        let Some(out) = &self.out else { return };
        let rate = out.rate as f32;
        self.sfx = self.sfx_src.iter().map(|s| mixer::resample(s, self.sfx_rate, rate).into()).collect();
    }

    fn flush(&mut self) {
        if self.cmds.is_empty() {
            return;
        }
        if let Some(out) = &self.out {
            out.with(|m| self.cmds.drain(..).for_each(|c| m.apply(c)));
        }
        self.cmds.clear();
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod smoke {
    use super::*;

    // Opens the default output and plays a tone and a chunk of music
    // through the mixer (skipped if there is no device).
    #[test]
    fn device_smoke() {
        let mut out = match Output::open() {
            Ok(o) => o,
            Err(e) => {
                eprintln!("no audio device, skipped: {e}");
                return;
            }
        };
        let rate = out.rate;
        let tone: Vec<f32> = (0..rate as usize / 5)
            .flat_map(|i| {
                let v = 0.1 * (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin();
                [v, v]
            })
            .collect();
        out.with(|m| m.play(tone.clone().into(), 0.3, 1.0));
        let at = (out.now() * rate as f64) as u64 + rate as u64 / 10;
        out.with(|m| {
            m.apply(Cmd::Gain { target: 0.8, tau: 0.05 });
            m.apply(Cmd::Chunk { at, data: tone.into() })
        });
        let t0 = out.now();
        std::thread::sleep(std::time::Duration::from_millis(600));
        let t1 = out.now();
        eprintln!("audio clock went {t0:.3} -> {t1:.3} s at {rate} Hz");
        assert!(t1 > t0, "the audio clock does not run");
        assert!(out.with(|m| m.peak) > 0.0 || t1 - t0 < 0.3);
    }
}
