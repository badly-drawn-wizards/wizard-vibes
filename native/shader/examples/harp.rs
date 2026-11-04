// One harpsichord note, for comparing with recordings: the left channel of
// music::harp_note at frequency F, held H seconds, for S seconds at 48 kHz,
// as raw f32 on stdout. With `arp S`: the music's harpsichord part alone
// for S seconds, stereo (interleaved). With `mix S [T0]`: the whole music
// from T0 (as the app hears it: through the soundboard, chunk by chunk).
//
// usage: cargo run --release --example harp -- F [H] [S] > note.f32
//        cargo run --release --example harp -- arp S > arp.f32
//        cargo run --release --example harp -- mix S [T0] > mix.f32
// (SONG=1 in the environment: the mix of song 1)
//        cargo run --release --example harp -- part NAME S [T0] > part.f32
// (`part`: one voice alone, mono: kick, clap, hats, bass, keys, strings,
// tune; of the cracktro: chip-arp, chip-bass, chip-drums, chip-tune; Starscape's: stars-celesta, stars-harp, stars-strings, stars-choir, and its dry mix: stars)

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("mix") {
        let secs: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(32.0);
        let t0: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let rate = 48000.0;
        let n = 24000usize;
        let mut state = wizard_shader::music::State::new();
        let mut out = Vec::new();
        let mut c = 0u32;
        while (c as usize * n) < (secs * rate) as usize {
            let song: i32 = std::env::var("SONG").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
            let chunk = wizard_shader::music::render_chunk(song, t0 + (c as usize * n) as f32 / rate, rate, n, c, &mut state);
            out.extend(chunk.iter().flat_map(|v| v.to_le_bytes()));
            c += 1;
        }
        use std::io::Write;
        std::io::stdout().write_all(&out).unwrap();
        return;
    }
    if args.first().map(String::as_str) == Some("part") {
        use wizard_shader::music::*;
        let name = args[1].clone();
        let secs: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(16.0);
        let t0: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let rate = 48000.0;
        let mut out = Vec::new();
        for i in 0..(secs * rate) as usize {
            let t = wrap(t0 + i as f32 / rate);
            let mono = |(l, r): (f32, f32)| 0.5 * (l + r);
            let v = match name.as_str() {
                "kick" => kick(rate, t),
                "clap" => clap(rate, t),
                "hats" => mono(hats(rate, t)),
                "bass" => bass(t),
                "keys" => mono(keys(t)),
                "strings" => mono(strings(t)),
                "tune" => mono(tune(t)),
                "chip-arp" => wizard_shader::chip::arpeggio(rate, t),
                "chip-bass" => wizard_shader::chip::bass(rate, t),
                "chip-drums" => wizard_shader::chip::drums(rate, t),
                "chip-tune" => mono(wizard_shader::chip::tune(rate, t)),
                "stars-celesta" => wizard_shader::stars::celesta(t),
                "stars-harp" => wizard_shader::stars::harp(t),
                "stars-strings" => mono(wizard_shader::stars::strings(t)),
                "stars-choir" => mono(wizard_shader::stars::choir(t)),
                "stars" => {
                    let (l, r, _) = wizard_shader::stars::parts(rate, t);
                    0.5 * (l + r)
                }
                _ => panic!("no part {name}"),
            };
            out.extend_from_slice(&v.to_le_bytes());
        }
        use std::io::Write;
        std::io::stdout().write_all(&out).unwrap();
        return;
    }
    if args.first().map(String::as_str) == Some("arp") {
        let secs: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(16.0);
        let rate = 48000.0;
        let mut out = Vec::new();
        for i in 0..(secs * rate) as usize {
            let (l, r) = wizard_shader::music::harpsichord(rate, i as f32 / rate);
            out.extend_from_slice(&l.to_le_bytes());
            out.extend_from_slice(&r.to_le_bytes());
        }
        use std::io::Write;
        std::io::stdout().write_all(&out).unwrap();
        return;
    }
    let a: Vec<f32> = args.iter().map(|s| s.parse().expect("a number")).collect();
    let (f, held, secs) = (a[0], *a.get(1).unwrap_or(&10.0), *a.get(2).unwrap_or(&3.0));
    let rate = 48000.0;
    let mut out = Vec::with_capacity((secs * rate) as usize * 4);
    for i in 0..(secs * rate) as usize {
        let t = i as f32 / rate;
        out.extend_from_slice(&wizard_shader::music::harp_note(rate, t, f, t, held).0.to_le_bytes());
    }
    use std::io::Write;
    std::io::stdout().write_all(&out).unwrap();
}
