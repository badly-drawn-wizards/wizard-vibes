// One harpsichord note, for comparing with recordings: the left channel of
// music::harp_note at frequency F, held H seconds, for S seconds at 48 kHz,
// as raw f32 on stdout. With `arp S`: the music's harpsichord part alone
// for S seconds, stereo (interleaved).
//
// usage: cargo run --release --example harp -- F [H] [S] > note.f32
//        cargo run --release --example harp -- arp S > arp.f32

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
