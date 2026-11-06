// The ray-marched wizard: the app, natively and in the browser. The shader
// crate (native/shader, rust-gpu) draws the scene through wgpu; egui draws
// the UI over it.

pub mod app;
pub mod audio;
pub mod game;
#[cfg(not(target_arch = "wasm32"))]
pub mod headless;
pub mod i18n;
pub mod nav;
pub mod params;
pub mod render;
pub mod scale;
pub mod source;
pub mod types;

// The repository (where params.json is): WIZARD_ROOT, or the working
// directory or one of its parents.
#[cfg(not(target_arch = "wasm32"))]
pub fn root() -> Result<std::path::PathBuf, String> {
    if let Ok(r) = std::env::var("WIZARD_ROOT") {
        return Ok(r.into());
    }
    let mut d = std::env::current_dir().map_err(|e| e.to_string())?;
    loop {
        if d.join("params.json").exists() {
            return Ok(d);
        }
        if !d.pop() {
            return Err("not in the repository (no params.json above here); set WIZARD_ROOT".into());
        }
    }
}

// The app in the browser, on the page's canvas #view.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    use wasm_bindgen::JsCast;
    console_error_panic_hook::set_once();
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document.get_element_by_id("view").expect("no canvas #view").dyn_into::<web_sys::HtmlCanvasElement>().unwrap();
        let options = eframe::WebOptions { wgpu_options: app::wgpu_options(), ..Default::default() };
        let r = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(|cc| {
                    let source = source::Source::new(cc.egui_ctx.clone());
                    Ok(Box::new(app::App::new(cc, source)))
                }),
            )
            .await;
        if let Err(e) = r {
            log::error!("the app failed to start: {e:?}");
        }
    });
}
