// One sound effect, rendered as the app renders them (sfx_samples): effect
// K (as sfx.rs numbers them) at 48 kHz, as raw interleaved stereo f32 on
// stdout.
//
// usage: cargo run --release --example sfx -- K > effect.f32

fn main() {
    let k: i32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).expect("an effect's number");
    let rate = 48000.0;
    let n = (wizard_shader::sfx::sfx_length(k) * rate) as usize;
    let out = wizard_shader::sfx::sfx_samples(k, rate, n);
    let bytes: Vec<u8> = out.iter().flat_map(|v| v.to_le_bytes()).collect();
    use std::io::Write;
    std::io::stdout().write_all(&bytes).unwrap();
}
