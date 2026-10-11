// muguet - config.rs
// Constantin Clerc (c22dev) - 0.1

use std::path::{Path, PathBuf};

#[cfg_attr(target_os = "ios", allow(dead_code))]
pub struct Startup {
    pub wasm_path: PathBuf,
    pub cache_dir: PathBuf,

    pub engine_args: Vec<String>,
}

const DEFAULT_CONFIG: &str = r#"{
  "scale": 0.5,
  "mouse_sensitivity": 2.0,
  "fullscreen": true,
  "fps_limit": 60,
  "start_mode": 2,
  "mute": false,
  "multiplayer": false,
  "host": false,
  "server": "",
  "player_name": "",
  "extra_args": []
}
"#;

fn support_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    if cfg!(target_os = "ios") {
        return PathBuf::from(home).join("Documents");
    }
    PathBuf::from(home).join("Library/Application Support/Muguet")
}

pub fn game_dir() -> PathBuf {
    support_dir().join("game")
}

pub fn game_ready() -> bool {
    let dir = game_dir();
    dir.join("data/manifest.json").exists() && build_dir(&dir).is_some_and(|b| !cfg!(feature = "jit") || crate::code_embedded() || b.join("game.wasm").exists())
}

pub fn bundled_resources() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let res = exe.parent()?.parent()?.join("Resources");
    (exe.parent()?.ends_with("Contents/MacOS") && res.is_dir()).then_some(res)
}

fn build_dir(root: &Path) -> Option<PathBuf> {
    std::fs::read_dir(root.join("b")).ok()?.flatten().map(|e| e.path()).find(|p| p.join("shaders/index.json").exists())
}

fn set_default(key: &str, value: impl AsRef<std::ffi::OsStr>) {
    if std::env::var_os(key).is_none() {
        std::env::set_var(key, value);
    }
}

fn redirect_logs_if_detached() {
    extern "C" {
        fn isatty(fd: i32) -> i32;
        fn dup2(old: i32, new: i32) -> i32;
    }

    if cfg!(target_os = "ios") {
        return;
    }
    let in_bundle = std::env::current_exe().map(|p| p.to_string_lossy().contains(".app/Contents/MacOS/")).unwrap_or(false);
    if !in_bundle || unsafe { isatty(2) } != 0 {
        return;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let path = PathBuf::from(home).join("Library/Logs/Muguet.log");
    if let Ok(f) = std::fs::File::create(&path) {
        use std::os::fd::IntoRawFd;
        let fd = f.into_raw_fd();
        unsafe {
            dup2(fd, 2);
            dup2(fd, 1);
        }
    }
}

pub fn setup() -> anyhow::Result<Startup> {
    redirect_logs_if_detached();
    let support = support_dir();
    std::fs::create_dir_all(support.join("saves"))?;

    let cfg_path = support.join("config.json");
    if !cfg_path.exists() {
        std::fs::write(&cfg_path, DEFAULT_CONFIG)?;
    }
    let engine_args = settings(&cfg_path);

    set_default("MUGUET_FSROOT", support.join("saves"));
    let build = locate();
    #[cfg(not(target_os = "ios"))]
    let build = build?;
    #[cfg(target_os = "ios")]
    let build = build.unwrap_or_default();
    Ok(Startup { wasm_path: build.join("game.wasm"), cache_dir: support, engine_args })
}

fn locate() -> anyhow::Result<PathBuf> {
    let root = game_dir();
    let build = build_dir(&root).filter(|_| game_ready()).ok_or_else(|| anyhow::anyhow!("no game data: import your game .zip from the launcher"))?;
    std::env::set_var("MUGUET_DATA", root.join("data"));
    std::env::set_var("MUGUET_SHADERS", build.join("shaders"));
    eprintln!("[config] game {}", root.display());
    Ok(build)
}

const SETTING_VARS: &[&str] = &["MUGUET_SCALE", "MUGUET_MOUSE_SENS", "MUGUET_TOUCH_SENS", "MUGUET_RES", "MUGUET_FULLSCREEN", "MUGUET_MUTE", "MUGUET_HOST", "MUGUET_SERVER", "MUGUET_NAME"];

#[cfg(target_os = "ios")]
pub fn reload() -> Vec<String> {
    for v in SETTING_VARS { std::env::remove_var(v); }
    let args = settings(&support_dir().join("config.json"));
    if let Err(e) = locate() {
        eprintln!("[config] {e:#}");
    }
    eprintln!("[config] launcher settings applied: {}", args.join(" "));
    args
}

fn settings(cfg_path: &Path) -> Vec<String> {
    let _ = SETTING_VARS;
    let cfg: serde_json::Value = std::fs::read_to_string(&cfg_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).map_err(|e| eprintln!("[config] {}: {e}", cfg_path.display())).ok())
        .unwrap_or_else(|| serde_json::from_str(DEFAULT_CONFIG).unwrap());
    let num = |k: &str| cfg.get(k).and_then(|v| v.as_f64());
    let flag = |k: &str| cfg.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    if let Some(v) = num("scale") { set_default("MUGUET_SCALE", v.to_string()); }
    if let Some(v) = num("mouse_sensitivity") { set_default("MUGUET_MOUSE_SENS", v.to_string()); }
    if let Some(v) = num("touch_sensitivity") { set_default("MUGUET_TOUCH_SENS", v.to_string()); }
    if let Some(r) = cfg.get("resolution").and_then(|v| v.as_str()) { set_default("MUGUET_RES", r); }
    if flag("fullscreen") { set_default("MUGUET_FULLSCREEN", "1"); }
    if flag("mute") { set_default("MUGUET_MUTE", "1"); }

    let text = |k: &str| cfg.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty());
    if flag("multiplayer") {
        if flag("host") { set_default("MUGUET_HOST", "1"); }
        else if let Some(v) = text("server") { set_default("MUGUET_SERVER", v); }
    }
    if let Some(v) = text("player_name") { set_default("MUGUET_NAME", v); }

    let mut engine_args = Vec::new();

    let fps = std::env::var("MUGUET_FPS").ok().and_then(|v| v.parse::<f64>().ok()).or_else(|| num("fps_limit")).unwrap_or(60.0);
    engine_args.push(format!("-frameLimit={}", if fps <= 0.0 { 0 } else if fps <= 30.0 { 2 } else { 1 }));

    let start_mode = std::env::var("MUGUET_START").ok().and_then(|v| v.parse::<f64>().ok()).or_else(|| num("start_mode")).unwrap_or(2.0) as i64;
    let start_mode = if (1..=2).contains(&start_mode) { start_mode } else { 2 };
    if !std::env::args().any(|a| a.starts_with("-wasmstartchoice")) {
        engine_args.push(format!("-wasmstartchoice={start_mode}"));
    }
    if let Some(extra) = cfg.get("extra_args").and_then(|v| v.as_array()) {
        engine_args.extend(extra.iter().filter_map(|a| a.as_str().map(String::from)));
    }
    if let Some(q) = num("texture_quality") {
        set_texture_quality(q as i64);
    }
    engine_args
}

fn set_texture_quality(q: i64) {
    let path = support_dir().join("saves/userdata/Documents/Rockstar Games/GTA V/settings.xml");
    let Ok(xml) = std::fs::read_to_string(&path) else { return };
    let re = regex::Regex::new(r#"<TextureQuality value="\d+" />"#).unwrap();
    let new = re.replace(&xml, format!(r#"<TextureQuality value="{}" />"#, q.clamp(0, 2)).as_str());
    if new != xml {
        let _ = std::fs::write(&path, new.as_bytes());
        eprintln!("[config] texture quality set to {q}");
    }
}
