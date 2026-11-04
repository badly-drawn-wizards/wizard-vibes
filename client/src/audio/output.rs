// The output device (cpal: ALSA etc. natively, Web Audio in the browser),
// pulling from the mixer. Natively the callback runs on an audio thread; in
// the browser cpal calls it on the main thread (the Stream is not Send and
// stays there): either way the mixer sits behind a mutex, held briefly.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use web_time::Instant;

use super::mixer::Mixer;

pub struct Shared {
    pub mixer: Mixer,
    // when the callback last ran, and how many frames it rendered then
    stamp: Option<(Instant, u32)>,
}

pub struct Output {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    stream: cpal::Stream,
    shared: Arc<Mutex<Shared>>,
    pub rate: u32,
    // (the clock as last read, so that it never goes back)
    last_now: f64,
}

impl Output {
    // Opens the default output device and starts it.
    pub fn open() -> Result<Output, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no audio output device")?;
        let supported = device.default_output_config().map_err(|e| format!("no output configuration: {e}"))?;
        let format = supported.sample_format();
        let config = supported.config();
        let rate = config.sample_rate;
        let channels = config.channels as usize;
        if channels == 0 || rate == 0 {
            return Err("unusable output configuration".into());
        }
        let shared = Arc::new(Mutex::new(Shared { mixer: Mixer::new(rate), stamp: None }));
        use cpal::SampleFormat as F;
        let stream = match format {
            F::F32 => build::<f32>(&device, config, &shared),
            F::F64 => build::<f64>(&device, config, &shared),
            F::I16 => build::<i16>(&device, config, &shared),
            F::I32 => build::<i32>(&device, config, &shared),
            F::U16 => build::<u16>(&device, config, &shared),
            F::U8 => build::<u8>(&device, config, &shared),
            F::I8 => build::<i8>(&device, config, &shared),
            f => return Err(format!("unsupported sample format {f}")),
        }
        .map_err(|e| format!("could not open the audio output: {e}"))?;
        stream.play().map_err(|e| format!("could not start the audio output: {e}"))?;
        log::info!("audio: {} Hz, {channels} channels, {format}", rate);
        Ok(Output { stream, shared, rate, last_now: 0.0 })
    }

    // (resumes it, e.g. after a gesture in the browser)
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn resume(&self) {
        if let Err(e) = self.stream.play() {
            log::warn!("audio: {e}");
        }
    }

    // The mixer's clock in seconds: the frames rendered, plus the time since
    // (up to the callback's buffer), never going back.
    pub fn now(&mut self) -> f64 {
        let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let mut t = s.mixer.pos as f64 / self.rate as f64;
        if let Some((at, n)) = s.stamp {
            t += at.elapsed().as_secs_f64().min(n as f64 / self.rate as f64);
        }
        drop(s);
        self.last_now = self.last_now.max(t);
        self.last_now
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut Mixer) -> R) -> R {
        f(&mut self.shared.lock().unwrap_or_else(|e| e.into_inner()).mixer)
    }
}

fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, shared: &Arc<Mutex<Shared>>) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    let shared = shared.clone();
    let mut buf: Vec<f32> = Vec::new();
    device.build_output_stream::<T, _, _>(
        config,
        move |data: &mut [T], _| {
            let frames = data.len() / channels;
            buf.resize(2 * frames, 0.0);
            {
                let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                s.mixer.render(&mut buf);
                s.stamp = Some((Instant::now(), frames as u32));
            }
            for (f, s) in data.chunks_exact_mut(channels).zip(buf.chunks_exact(2)) {
                if channels == 1 {
                    f[0] = T::from_sample(0.5 * (s[0] + s[1]));
                } else {
                    f[0] = T::from_sample(s[0]);
                    f[1] = T::from_sample(s[1]);
                    for c in &mut f[2..] {
                        *c = T::from_sample(0.0);
                    }
                }
            }
        },
        |e| log::warn!("audio: {e}"),
        None,
    )
}
