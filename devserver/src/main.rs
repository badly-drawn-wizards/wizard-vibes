// The dev server. It keeps the shader and the web app built and tells the
// app (native or in the browser) when there is something new:
//
// - The shader (native/shader, built by native/shader-builder into
//   build/shader): rebuilt when its sources, params.json or
//   config.local.json change. Quality presets asked for (POST /preset, or
//   by the native app writing build/want-preset) are built into
//   build/shader-<level> and kept up to date the same way. Builds run one at
//   a time; a change during one queues one more.
// - The web app (client, for wasm32, through wasm-bindgen into build/web):
//   rebuilt when client/ or the shader crate (which it runs on the CPU too)
//   changes, and the page reloaded. (Under
//   `dx serve --hotpatch`, Subsecond patches it instead: see README.md.)
//
// HTTP (port 8080, or --port N):
//   GET  /                 the page (client/web/index.html), /pkg/* its wasm
//   GET  /build/shader*/*  the shader builds
//   GET  /events           server-sent events: hello {id} (first; the id is
//                          new each run: build/devserver-id has it too),
//                          building, failed {message}, shader {preset},
//                          reload
//   POST /config           compile-time parameter changes (JSON object),
//                          saved to config.local.json (which rebuilds)
//   POST /preset           a quality level ("low", "med", "high") to build;
//                          answers once it is built
//
// With --static DIR it instead builds a self-contained site into DIR (the
// page, its wasm, the shader with the compile-time defaults and
// one build per quality level) and exits: no dev server behind it, so no
// hot reload or shader settings, and it can sit under any sub-path.
//
// usage: cargo run -p wizard-dev [-- --port N | --static DIR] (in `nix develop .#app`)

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::Router;
use notify::{RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, broadcast, mpsc, watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

#[derive(Clone, Debug)]
struct Note {
    name: &'static str,
    data: String,
}

struct Server {
    root: PathBuf,
    notes: broadcast::Sender<Note>,
    // what to build next: the shader (with the presets asked for), the web app
    jobs: mpsc::UnboundedSender<Job>,
    presets: Mutex<BTreeSet<String>>,
    // how many times each preset has been built (for /preset to wait on)
    built: watch::Sender<Vec<(String, u64)>>,
    // this server's own id, new each time it starts (see `events`)
    id: String,
}

// An id for this run of the server: random enough to tell runs apart.
fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    h.write_u32(std::process::id());
    format!("{:016x}", h.finish())
}

#[derive(Debug, PartialEq)]
enum Job {
    Shader,
    Preset(String),
    Web,
}

impl Server {
    fn note(&self, name: &'static str, data: serde_json::Value) {
        let _ = self.notes.send(Note { name, data: data.to_string() });
    }
}

fn log(msg: &str) {
    eprintln!("[dev] {msg}");
}

// Runs a command in the repository; Ok, or its output.
async fn run(root: &Path, cmd: &str, args: &[&str]) -> Result<(), String> {
    let out = tokio::process::Command::new(cmd).args(args).current_dir(root).output().await.map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
    }
}

async fn build_shader(s: &Server, preset: Option<&str>) {
    let t = Instant::now();
    s.note("building", serde_json::json!({ "preset": preset }));
    let out = match preset {
        None => "build/shader".to_string(),
        Some(p) => format!("build/shader-{p}"),
    };
    let mut args = vec!["run", "--release", "--quiet", "--manifest-path", "native/shader-builder/Cargo.toml", "--", "--out", &out];
    if let Some(p) = preset {
        args.extend(["--preset", p]);
    }
    match run(&s.root, "cargo", &args).await {
        Ok(()) => {
            log(&format!("shader{} built in {:.1} s", preset.map(|p| format!(" ({p})")).unwrap_or_default(), t.elapsed().as_secs_f32()));
            s.note("shader", serde_json::json!({ "preset": preset }));
            if let Some(p) = preset {
                s.built.send_modify(|b| match b.iter_mut().find(|(k, _)| k == p) {
                    Some(e) => e.1 += 1,
                    None => b.push((p.to_string(), 1)),
                });
            }
        }
        Err(e) => {
            // (the compiler's errors, without cargo's noise)
            let msg: String = e.lines().filter(|l| !l.trim_start().starts_with("Compiling") && !l.trim_start().starts_with("Finished")).collect::<Vec<_>>().join("\n");
            log(&format!("shader build failed:\n{msg}"));
            s.note("failed", serde_json::json!({ "message": msg, "preset": preset }));
        }
    }
}

async fn build_web(s: &Server) {
    let t = Instant::now();
    let r = async {
        run(&s.root, "cargo", &["build", "--release", "--quiet", "-p", "wizard", "--lib", "--target", "wasm32-unknown-unknown"]).await?;
        run(&s.root, "wasm-bindgen", &["--target", "web", "--no-typescript", "--out-dir", "build/web", "target/wasm32-unknown-unknown/release/wizard.wasm"]).await
    }
    .await;
    match r {
        Ok(()) => {
            log(&format!("web app built in {:.1} s", t.elapsed().as_secs_f32()));
            s.note("reload", serde_json::json!({}));
        }
        Err(e) => {
            log(&format!("web app build failed:\n{e}"));
            s.note("failed", serde_json::json!({ "message": format!("web app build failed:\n{e}") }));
        }
    }
}

// The builder: one job at a time, repeated jobs folded together.
async fn builder(s: Arc<Server>, mut jobs: mpsc::UnboundedReceiver<Job>) {
    while let Some(first) = jobs.recv().await {
        // (changes come in bursts: wait for them to settle)
        tokio::time::sleep(Duration::from_millis(150)).await;
        let mut todo = vec![first];
        while let Ok(j) = jobs.try_recv() {
            if !todo.contains(&j) {
                todo.push(j);
            }
        }
        for j in todo {
            match j {
                Job::Shader => {
                    let started = std::time::SystemTime::now();
                    build_shader(&s, None).await;
                    // (the presets in use follow the sources too)
                    let presets: Vec<String> = s.presets.lock().await.iter().cloned().collect();
                    for p in presets {
                        build_shader(&s, Some(&p)).await;
                    }
                    // (a source changed while it built, whose change was
                    // somehow missed: once seen, an edit left unbuilt; so
                    // again, if any is newer than the build)
                    if newest(&s.root.join("native/shader/src")).is_some_and(|m| m > started) {
                        let _ = s.jobs.send(Job::Shader);
                    }
                }
                Job::Preset(p) => build_shader(&s, Some(&p)).await,
                Job::Web => build_web(&s).await,
            }
        }
    }
}

// The latest modification time of the files under a directory.
fn newest(dir: &Path) -> Option<std::time::SystemTime> {
    let mut out = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let m = if e.path().is_dir() { newest(&e.path()) } else { e.metadata().ok().and_then(|m| m.modified().ok()) };
        if m > out {
            out = m;
        }
    }
    out
}

// Which jobs a changed file calls for. (The web app runs the shader crate
// on the CPU too: the sound effects, the collision probe.)
fn jobs_for(root: &Path, p: &Path) -> Vec<Job> {
    let Ok(rel) = p.strip_prefix(root) else { return vec![] };
    let s = rel.to_string_lossy();
    if s.contains("/target/") {
        return vec![];
    }
    // (the parameters too: the shader crate's params!() reads them)
    if s.starts_with("native/shader/src/") || s == "native/shader/Cargo.toml" || s.starts_with("native/params") || s.starts_with("native/shader-macros") || s == "params.json" || s == "config.local.json" {
        return vec![Job::Shader, Job::Web];
    }
    if s.starts_with("native/shader-builder/src/") {
        return vec![Job::Shader];
    }
    if s.starts_with("client/src/") || s == "client/Cargo.toml" {
        return vec![Job::Web];
    }
    vec![]
}

// Each connection starts with `hello` {id}: a client reconnecting (as
// EventSource does by itself) to a server with another id, a restarted one,
// reloads, as what it has may be stale.
async fn events(State(s): State<Arc<Server>>) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let hello = Event::default().event("hello").data(serde_json::json!({ "id": s.id }).to_string());
    let notes = BroadcastStream::new(s.notes.subscribe()).filter_map(|n| n.ok()).map(|n| Event::default().event(n.name).data(n.data));
    let stream = tokio_stream::once(hello).chain(notes).map(Ok);
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// (the body is parsed whatever its content type says)
async fn config(State(s): State<Arc<Server>>, body: String) -> Result<&'static str, (StatusCode, String)> {
    let changes: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&body).map_err(|e| (StatusCode::BAD_REQUEST, format!("not a JSON object: {e}")))?;
    let path = s.root.join("config.local.json");
    let mut saved: serde_json::Map<String, serde_json::Value> =
        std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    saved.extend(changes);
    std::fs::write(&path, serde_json::to_string_pretty(&saved).unwrap() + "\n").map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    // (the watcher sees the file and rebuilds)
    Ok("saved")
}

async fn preset(State(s): State<Arc<Server>>, level: String) -> Result<&'static str, (StatusCode, String)> {
    let level = level.trim().to_string();
    if !["low", "med", "high"].contains(&level.as_str()) {
        return Err((StatusCode::BAD_REQUEST, format!("no quality level {level}")));
    }
    let count = |b: &Vec<(String, u64)>| b.iter().find(|(k, _)| *k == level).map(|e| e.1).unwrap_or(0);
    let mut rx = s.built.subscribe();
    let before = count(&rx.borrow());
    let fresh = s.presets.lock().await.insert(level.clone());
    if fresh || !s.root.join(format!("build/shader-{level}/shader.json")).exists() {
        let _ = s.jobs.send(Job::Preset(level.clone()));
        // (answers once built; a failure is told over the events)
        let wait = rx.wait_for(|b| count(b) > before);
        let _ = tokio::time::timeout(Duration::from_secs(600), wait).await;
    }
    Ok("built")
}

// The static site (see the top).
async fn build_static(root: &Path, dir: &Path) -> Result<(), String> {
    let dir = if dir.is_absolute() { dir.to_path_buf() } else { std::env::current_dir().unwrap().join(dir) };
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pkg")).map_err(|e| e.to_string())?;
    let builder = ["run", "--release", "--quiet", "--manifest-path", "native/shader-builder/Cargo.toml", "--", "--defaults", "--out"];
    for level in ["low", "med", "high"] {
        let out = dir.join(format!("build/shader-{level}"));
        let mut args = builder.to_vec();
        args.extend([out.to_str().unwrap(), "--preset", level]);
        run(root, "cargo", &args).await?;
        log(&format!("shader ({level}) built"));
    }
    let out = dir.join("build/shader");
    let mut args = builder.to_vec();
    args.push(out.to_str().unwrap());
    run(root, "cargo", &args).await?;
    log("shader built");
    // (the web app's own copy of the shader crate at the defaults too)
    run(root, "cargo", &["build", "--release", "--quiet", "-p", "wizard", "--lib", "--target", "wasm32-unknown-unknown", "--config", "env.WIZARD_DEFAULTS=\"1\""]).await?;
    run(root, "wasm-bindgen", &["--target", "web", "--no-typescript", "--out-dir", dir.join("pkg").to_str().unwrap(), "target/wasm32-unknown-unknown/release/wizard.wasm"]).await?;
    log("web app built");
    for (from, to) in [("client/web/index.html", "index.html"), ("LICENSE", "LICENSE")] {
        std::fs::copy(root.join(from), dir.join(to)).map_err(|e| format!("{from}: {e}"))?;
    }
    // (the native SPIR-V is not needed in the browser)
    for d in ["build/shader", "build/shader-low", "build/shader-med", "build/shader-high"] {
        let _ = std::fs::remove_file(dir.join(d).join("shader.spv"));
    }
    log(&format!("wrote {}", dir.display()));
    Ok(())
}

#[tokio::main]
async fn main() {
    let mut port = 8080u16;
    let mut static_dir: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => port = args.next().and_then(|p| p.parse().ok()).expect("--port N"),
            "--static" => static_dir = Some(args.next().expect("--static DIR").into()),
            _ => panic!("unknown argument {a}"),
        }
    }
    let mut root = std::env::current_dir().unwrap();
    while !root.join("params.json").exists() {
        assert!(root.pop(), "not in the repository (no params.json above here)");
    }
    if let Some(dir) = static_dir {
        if let Err(e) = build_static(&root, &dir).await {
            log(&e);
            std::process::exit(1);
        }
        return;
    }
    let (jobs_tx, jobs_rx) = mpsc::unbounded_channel();
    let (notes, _) = broadcast::channel(64);
    let id = new_id();
    // (for the native app, which reads the builds' files rather than the
    // events: written once the server is up, below)
    let s = Arc::new(Server { root: root.clone(), notes, jobs: jobs_tx, presets: Mutex::new(BTreeSet::new()), built: watch::channel(vec![]).0, id: id.clone() });
    tokio::spawn(builder(s.clone(), jobs_rx));
    let _ = s.jobs.send(Job::Shader);
    if !root.join("build/web/wizard_bg.wasm").exists() {
        let _ = s.jobs.send(Job::Web);
    }

    // the watcher: changed files to jobs (params.json, compiled into the
    // shader and the app, rebuilds both); build/want-preset is the native app
    // asking for a level
    let (fs_tx, mut fs_rx) = mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |r: notify::Result<notify::Event>| {
        if let Ok(e) = r {
            if !e.kind.is_access() {
                for p in e.paths {
                    let _ = fs_tx.send(p);
                }
            }
        }
    })
    .expect("no file watcher");
    for (dir, mode) in [("native/shader", RecursiveMode::Recursive), ("native/shader-builder/src", RecursiveMode::Recursive), ("native/params/src", RecursiveMode::Recursive), ("native/shader-macros/src", RecursiveMode::Recursive), ("client", RecursiveMode::Recursive), (".", RecursiveMode::NonRecursive), ("build", RecursiveMode::NonRecursive)] {
        std::fs::create_dir_all(root.join(dir)).ok();
        watcher.watch(&root.join(dir), mode).unwrap_or_else(|e| panic!("watching {dir}: {e}"));
    }
    {
        let s = s.clone();
        tokio::spawn(async move {
            while let Some(p) = fs_rx.recv().await {
                if p == s.root.join("build/want-preset") {
                    if let Ok(level) = std::fs::read_to_string(&p) {
                        let level = level.trim().to_string();
                        if s.presets.lock().await.insert(level.clone()) {
                            let _ = s.jobs.send(Job::Preset(level));
                        }
                    }
                    continue;
                }
                for j in jobs_for(&s.root, &p) {
                    let _ = s.jobs.send(j);
                }
            }
        });
    }

    let no_store = tower_http::set_header::SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    let app = Router::new()
        .route("/events", get(events))
        .route("/config", post(config))
        .route("/preset", post(preset))
        .route_service("/", tower_http::services::ServeFile::new(root.join("client/web/index.html")))
        .nest_service("/pkg", tower_http::services::ServeDir::new(root.join("build/web")))
        .nest_service("/build", tower_http::services::ServeDir::new(root.join("build")))
        .layer(no_store)
        // (the page may come from elsewhere: dx serve)
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(s);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.unwrap_or_else(|e| panic!("port {port}: {e}"));
    log(&format!("http://localhost:{port} (server {id})"));
    let _ = std::fs::write(root.join("build/devserver-id"), &id);
    axum::serve(listener, app).await.unwrap();
    drop(watcher);
}
