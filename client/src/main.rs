// The app natively: a window (eframe on winit, wgpu on Vulkan, Metal or
// DX12), reading the shader builds from the repository (run from it, or set
// WIZARD_ROOT). Or headless, with the same shader and passes (headless.rs):
//
// usage: wizard
//        wizard --shot FILE.ppm [--size WxH] [--t0 T] [--wgsl] [--set PARAM=VALUE ...]
//        wizard --bench [FRAMES] [--size WxH] [--t0 T] [--wgsl] [--set PARAM=VALUE ...]
//
// --wgsl runs naga's WGSL of the shader even where the SPIR-V could run as
// it is (to see what the browser gets).

#[cfg(not(target_arch = "wasm32"))]
fn die(msg: &str) -> ! {
    eprintln!("wizard: {msg}");
    std::process::exit(2)
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn")).init();
    let root = wizard::root().unwrap_or_else(|e| die(&e));
    // (under `dx serve --hotpatch`: take its patches to the app's code)
    dioxus_devtools::connect_subsecond();
    let mut o = wizard::headless::Options { shot: None, bench: None, size: (1920, 1080), t0: 0.0, sets: vec![], wgsl: false };
    let mut it = std::env::args().skip(1).peekable();
    while let Some(arg) = it.next() {
        let mut value = || it.next().unwrap_or_else(|| die(&format!("{arg} needs a value")));
        match arg.as_str() {
            "--shot" => o.shot = Some(value().into()),
            "--t0" => o.t0 = value().parse().unwrap_or_else(|_| die("--t0: a number")),
            "--size" => {
                let v = value();
                let (w, h) = v.split_once('x').unwrap_or_else(|| die("--size: WxH"));
                o.size = (w.parse().unwrap_or_else(|_| die("--size: WxH")), h.parse().unwrap_or_else(|_| die("--size: WxH")));
            }
            "--set" => {
                let v = value();
                let (k, x) = v.split_once('=').unwrap_or_else(|| die("--set: key=value"));
                o.sets.push((k.to_string(), x.parse().unwrap_or_else(|_| die("--set: a number"))));
            }
            "--wgsl" => o.wgsl = true,
            "--bench" => o.bench = Some(if it.peek().is_some_and(|n| n.parse::<u32>().is_ok()) { it.next().unwrap().parse().unwrap() } else { 60 }),
            _ => die(&format!("unknown argument {arg}")),
        }
    }
    if o.shot.is_some() || o.bench.is_some() {
        wizard::headless::run(&root, &o).unwrap_or_else(|e| die(&e));
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("wizard").with_inner_size([1600.0, 900.0]),
        wgpu_options: wizard::app::wgpu_options(),
        ..Default::default()
    };
    eframe::run_native(
        "wizard",
        options,
        Box::new(move |cc| {
            let source = wizard::source::Source::new(root, true, cc.egui_ctx.clone());
            Ok(Box::new(wizard::app::App::new(cc, source)))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {}
