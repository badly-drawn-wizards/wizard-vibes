// Where the shader and the parameters' schema come from, and how changes
// arrive (hot reload). The shader builder (native/shader-builder) writes
// build/shader/{shader.spv, shader.wgsl, shader.json}; the dev server
// (devserver/) rebuilds it whenever the shader's sources, params.json or
// config.local.json change.
//
// Natively, the files are read from the repository, watched by polling
// shader.json's modification time; compile-time parameter changes are saved
// to config.local.json (which the dev server, if running, picks up).
//
// In the browser they are fetched from the dev server, whose event stream
// (/events) says when a build starts, fails or is done; compile-time changes
// are posted to it (/config).

use crate::params::Schema;
use crate::types::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub from: From,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum From {
    Param { id: String, index: Option<usize> },
    Drive { id: String },
    Arg { id: String },
}

// An entry point: its workgroup size and the bindings it uses.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Entry {
    pub name: String,
    pub workgroup: [u32; 3],
    pub bindings: Vec<Binding>,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Binding {
    pub group: u32,
    pub binding: u32,
    // "uniform", "storage" or "storage-read"
    pub kind: String,
}

// shader.json
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Meta {
    pub fields: Vec<Field>,
    pub workgroup: u32,
    pub entries: Vec<Entry>,
    pub preset: Option<String>,
    // the compile-time values it was built with
    pub compile: HashMap<String, Value>,
}

pub struct ShaderFiles {
    // what identifies this build (its shader.json's text hashed)
    pub version: String,
    pub meta: Meta,
    pub wgsl: String,
    pub spv: Option<Vec<u8>>,
}

pub enum Event {
    Schema(Schema),
    Shader(ShaderFiles),
    Building,
    Error(String),
}

fn version_of(text: &str) -> String {
    // (FNV-1a: only to tell builds apart)
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn parse(meta_text: &str, wgsl: String, spv: Option<Vec<u8>>) -> Result<ShaderFiles, String> {
    let meta: Meta = serde_json::from_str(meta_text).map_err(|e| format!("shader.json: {e}"))?;
    Ok(ShaderFiles { version: version_of(meta_text), meta, wgsl, spv })
}

// The build directory for a quality preset (None: the drawer's settings).
pub fn build_dir(preset: Option<&str>) -> String {
    match preset {
        None => "build/shader".into(),
        Some(p) => format!("build/shader-{p}"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::Source;
#[cfg(target_arch = "wasm32")]
pub use web::Source;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime};

    pub struct Source {
        pub root: PathBuf,
        events: mpsc::Receiver<Event>,
        // which build directory the watcher follows
        follow: std::sync::Arc<std::sync::Mutex<String>>,
    }

    fn mtime(p: &Path) -> Option<SystemTime> {
        std::fs::metadata(p).and_then(|m| m.modified()).ok()
    }

    fn load(root: &Path, dir: &str, want_spv: bool) -> Result<ShaderFiles, String> {
        let d = root.join(dir);
        let read = |f: &str| std::fs::read_to_string(d.join(f)).map_err(|e| format!("{}: {e}", d.join(f).display()));
        let meta = read("shader.json")?;
        let wgsl = read("shader.wgsl")?;
        let spv = if want_spv { std::fs::read(d.join("shader.spv")).ok() } else { None };
        parse(&meta, wgsl, spv)
    }

    impl Source {
        // `root`: the repository (where params.json is). `want_spv`: also
        // read the SPIR-V (to run it as it is, where the device can).
        pub fn new(root: PathBuf, want_spv: bool, ctx: egui::Context) -> Source {
            let (tx, rx) = mpsc::channel();
            let follow = std::sync::Arc::new(std::sync::Mutex::new(build_dir(None)));
            let (r, f) = (root.clone(), follow.clone());
            std::thread::spawn(move || {
                let mut seen_schema = None;
                let mut seen: Option<(String, Option<SystemTime>)> = None;
                loop {
                    let schema_path = r.join("params.json");
                    let t = mtime(&schema_path);
                    if t != seen_schema {
                        seen_schema = t;
                        match std::fs::read_to_string(&schema_path).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())) {
                            Ok(s) => {
                                let _ = tx.send(Event::Schema(s));
                            }
                            Err(e) => {
                                let _ = tx.send(Event::Error(format!("params.json: {e}")));
                            }
                        }
                        ctx.request_repaint();
                    }
                    let dir = f.lock().unwrap().clone();
                    let t = mtime(&r.join(&dir).join("shader.json"));
                    if seen.as_ref() != Some(&(dir.clone(), t)) && t.is_some() {
                        seen = Some((dir.clone(), t));
                        let ev = match load(&r, &dir, want_spv) {
                            Ok(s) => Event::Shader(s),
                            Err(e) => Event::Error(e),
                        };
                        if tx.send(ev).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            });
            Source { root, events: rx, follow }
        }

        pub fn poll(&mut self) -> Option<Event> {
            self.events.try_recv().ok()
        }

        // Saves compile-time changes to config.local.json, where the dev
        // server (if running) sees them and rebuilds.
        pub fn send_config(&mut self, changes: HashMap<String, Value>) {
            let path = self.root.join("config.local.json");
            let mut saved: serde_json::Map<String, serde_json::Value> =
                std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
            for (k, v) in changes {
                saved.insert(k, serde_json::to_value(v).unwrap());
            }
            if let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(&saved).unwrap() + "\n") {
                log::error!("{}: {e}", path.display());
            }
        }

        // Follows the build for a quality preset (None: the drawer's).
        // Natively the dev server builds every preset it is asked for
        // through its build directories; here only which is watched changes.
        pub fn follow(&mut self, preset: Option<&str>) {
            let dir = build_dir(preset);
            if let Some(p) = preset {
                // (asks the dev server for it: a request file it watches)
                let _ = std::fs::create_dir_all(self.root.join("build"));
                let _ = std::fs::write(self.root.join("build/want-preset"), p);
            }
            *self.follow.lock().unwrap() = dir;
        }

        pub fn has_server(&self) -> bool {
            true
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;

    pub struct Source {
        base: String,
        queue: Rc<RefCell<VecDeque<Event>>>,
        ctx: egui::Context,
        preset: Rc<RefCell<Option<String>>>,
        _events: Option<web_sys::EventSource>,
        server: Rc<std::cell::Cell<bool>>,
    }

    async fn fetch_text(url: &str) -> Result<String, String> {
        let window = web_sys::window().unwrap();
        let resp = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url)).await.map_err(|e| format!("{url}: {e:?}"))?;
        let resp: web_sys::Response = resp.dyn_into().unwrap();
        if !resp.ok() {
            return Err(format!("{url}: HTTP {}", resp.status()));
        }
        let text = wasm_bindgen_futures::JsFuture::from(resp.text().unwrap()).await.map_err(|e| format!("{url}: {e:?}"))?;
        Ok(text.as_string().unwrap_or_default())
    }

    async fn post(url: &str, body: String) -> Result<String, String> {
        let window = web_sys::window().unwrap();
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
        init.set_body(&JsValue::from_str(&body));
        let headers = web_sys::Headers::new().map_err(|e| format!("{e:?}"))?;
        let _ = headers.set("Content-Type", "application/json");
        init.set_headers(&headers);
        let req = web_sys::Request::new_with_str_and_init(url, &init).map_err(|e| format!("{e:?}"))?;
        let resp = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&req)).await.map_err(|e| format!("{url}: {e:?}"))?;
        let resp: web_sys::Response = resp.dyn_into().unwrap();
        let text = wasm_bindgen_futures::JsFuture::from(resp.text().unwrap()).await.map_err(|e| format!("{e:?}"))?;
        let text = text.as_string().unwrap_or_default();
        if resp.ok() { Ok(text) } else { Err(text) }
    }

    fn load(base: String, dir: String, queue: Rc<RefCell<VecDeque<Event>>>, ctx: egui::Context) {
        wasm_bindgen_futures::spawn_local(async move {
            let r = async {
                let meta = fetch_text(&format!("{base}/{dir}/shader.json")).await?;
                let wgsl = fetch_text(&format!("{base}/{dir}/shader.wgsl")).await?;
                parse(&meta, wgsl, None)
            }
            .await;
            queue.borrow_mut().push_back(match r {
                Ok(s) => Event::Shader(s),
                Err(e) => Event::Error(e),
            });
            ctx.request_repaint();
        });
    }

    impl Source {
        // The dev server: ?server=URL, else the page's own origin.
        pub fn new(ctx: egui::Context) -> Source {
            let window = web_sys::window().unwrap();
            let location = window.location();
            let search = location.search().unwrap_or_default();
            let base = search
                .trim_start_matches('?')
                .split('&')
                .find_map(|kv| kv.strip_prefix("server=").map(|s| js_sys::decode_uri_component(s).ok().and_then(|s| s.as_string()).unwrap_or_default()))
                // (the page's own directory: a static site may sit under a sub-path)
                .unwrap_or_else(|| {
                    let href = location.href().unwrap_or_default();
                    let href = href.split(['?', '#']).next().unwrap_or_default();
                    href[..href.rfind('/').map_or(href.len(), |i| i + 1)].to_string()
                });
            let base = base.trim_end_matches('/').to_string();
            let queue = Rc::new(RefCell::new(VecDeque::new()));
            let preset: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
            {
                let (q, c, b) = (queue.clone(), ctx.clone(), base.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    let ev = match fetch_text(&format!("{b}/params.json")).await.and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())) {
                        Ok(s) => Event::Schema(s),
                        Err(e) => Event::Error(format!("params.json: {e}")),
                    };
                    q.borrow_mut().push_back(ev);
                    c.request_repaint();
                });
            }
            load(base.clone(), build_dir(None), queue.clone(), ctx.clone());
            // the dev server's events (a static site has none)
            let events = web_sys::EventSource::new(&format!("{base}/events")).ok();
            if let Some(es) = &events {
                let add = |name: &str, f: Box<dyn FnMut(web_sys::MessageEvent)>| {
                    let cb = Closure::wrap(f);
                    es.add_event_listener_with_callback(name, cb.as_ref().unchecked_ref()).unwrap();
                    cb.forget();
                };
                let (q, c) = (queue.clone(), ctx.clone());
                add(
                    "building",
                    Box::new(move |_| {
                        q.borrow_mut().push_back(Event::Building);
                        c.request_repaint();
                    }),
                );
                let (q, c) = (queue.clone(), ctx.clone());
                add(
                    "failed",
                    Box::new(move |e: web_sys::MessageEvent| {
                        let msg = e.data().as_string().unwrap_or_default();
                        let msg = serde_json::from_str::<serde_json::Value>(&msg).ok().and_then(|v| v["message"].as_str().map(String::from)).unwrap_or(msg);
                        q.borrow_mut().push_back(Event::Error(msg));
                        c.request_repaint();
                    }),
                );
                let (q, c, b, p) = (queue.clone(), ctx.clone(), base.clone(), preset.clone());
                add(
                    "shader",
                    Box::new(move |e: web_sys::MessageEvent| {
                        // (only the build this page follows)
                        let data = e.data().as_string().unwrap_or_default();
                        let built: Option<String> = serde_json::from_str::<serde_json::Value>(&data).ok().and_then(|v| v["preset"].as_str().map(String::from));
                        log::debug!("shader built ({built:?}; following {:?})", p.borrow());
                        if built == *p.borrow() {
                            load(b.clone(), build_dir(built.as_deref()), q.clone(), c.clone());
                        }
                    }),
                );
                let (q, c, b) = (queue.clone(), ctx.clone(), base.clone());
                add(
                    "params",
                    Box::new(move |_| {
                        let (q, c, b) = (q.clone(), c.clone(), b.clone());
                        wasm_bindgen_futures::spawn_local(async move {
                            if let Ok(s) = fetch_text(&format!("{b}/params.json")).await.and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())) {
                                q.borrow_mut().push_back(Event::Schema(s));
                                c.request_repaint();
                            }
                        });
                    }),
                );
            }
            // (a dev server is there once its events connect: a static site
            // has none)
            let server = Rc::new(std::cell::Cell::new(false));
            if let Some(es) = &events {
                let (sv, c) = (server.clone(), ctx.clone());
                let cb = Closure::wrap(Box::new(move |_: web_sys::Event| {
                    sv.set(true);
                    c.request_repaint();
                }) as Box<dyn FnMut(web_sys::Event)>);
                es.set_onopen(Some(cb.as_ref().unchecked_ref()));
                cb.forget();
                // (and a static site's missing events are not retried)
                let es2 = es.clone();
                let sv = server.clone();
                let cb = Closure::wrap(Box::new(move |_: web_sys::Event| {
                    if !sv.get() {
                        es2.close();
                    }
                }) as Box<dyn FnMut(web_sys::Event)>);
                es.set_onerror(Some(cb.as_ref().unchecked_ref()));
                cb.forget();
            }
            Source { base, queue, ctx, preset, _events: events, server }
        }

        pub fn poll(&mut self) -> Option<Event> {
            self.queue.borrow_mut().pop_front()
        }

        pub fn send_config(&mut self, changes: HashMap<String, Value>) {
            let (url, body, q, c) = (format!("{}/config", self.base), serde_json::to_string(&changes).unwrap(), self.queue.clone(), self.ctx.clone());
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(e) = post(&url, body).await {
                    q.borrow_mut().push_back(Event::Error(format!("saving the shader settings: {e}")));
                    c.request_repaint();
                }
            });
        }

        // Follows the build for a quality preset (None: the drawer's): asks
        // the dev server for it (built if need be), then loads it.
        pub fn follow(&mut self, preset: Option<&str>) {
            *self.preset.borrow_mut() = preset.map(String::from);
            match preset {
                None => load(self.base.clone(), build_dir(None), self.queue.clone(), self.ctx.clone()),
                Some(p) => {
                    let (url, q, c, b, p) = (format!("{}/preset", self.base), self.queue.clone(), self.ctx.clone(), self.base.clone(), p.to_string());
                    let server = self.server.get();
                    wasm_bindgen_futures::spawn_local(async move {
                        match post(&url, p.clone()).await {
                            Ok(_) => load(b, build_dir(Some(&p)), q, c),
                            // (a static site has each level built already)
                            Err(_) if !server => load(b, build_dir(Some(&p)), q, c),
                            Err(e) => {
                                q.borrow_mut().push_back(Event::Error(format!("quality {p}: {e}")));
                                c.request_repaint();
                            }
                        }
                    });
                }
            }
        }

        pub fn has_server(&self) -> bool {
            self.server.get()
        }
    }
}
