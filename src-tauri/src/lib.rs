//! 拾忆 desktop backend: the shiyi-core SQLite store (shared with the `shiyi`
//! CLI that AI agents use), a watcher that tells the window when an agent
//! changed something, settings, the AI proxy, import/export dialogs, CLI and
//! skill installers, and the global quick-capture shortcut.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shiyi_core::Core;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

static QUITTING: AtomicBool = AtomicBool::new(false);
const KEEP_BACKUPS: usize = 14;

struct Store {
    core: Mutex<Option<Core>>,
    error: Option<String>,
    path: PathBuf,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
struct Settings {
    api_format: String,
    api_base: String,
    api_key: String,
    model: String,
    quick_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api_format: "anthropic".into(),
            api_base: String::new(),
            api_key: String::new(),
            model: "claude-sonnet-5-5".into(),
            quick_model: "claude-haiku-5-5".into(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicSettings {
    api_format: String,
    api_base: String,
    model: String,
    quick_model: String,
    has_key: bool,
    data_path: String,
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn read_settings(app: &AppHandle) -> Settings {
    data_dir(app)
        .ok()
        .and_then(|d| fs::read_to_string(d.join("settings.json")).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn home() -> PathBuf {
    shiyi_core::install::home_dir().unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "C:\\" } else { "/" }))
}

/// Runs `f` with the database, mapping errors to `{code, message, hint}`.
fn with_core<T>(store: &Store, f: impl FnOnce(&mut Core) -> shiyi_core::Result<T>) -> Result<T, Value> {
    let mut guard = store
        .core
        .lock()
        .map_err(|_| json!({ "code": "busy", "message": "数据库正忙，请稍后再试" }))?;
    match guard.as_mut() {
        Some(core) => f(core).map_err(|e| e.to_json()["error"].clone()),
        None => Err(json!({
            "code": "db_unavailable",
            "message": format!("打不开数据库：{}", store.error.clone().unwrap_or_default()),
            "hint": store.path.display().to_string()
        })),
    }
}

// ---------- data ----------

#[tauri::command]
fn core_query(store: State<'_, Store>, q: Value) -> Result<Value, Value> {
    with_core(&store, |c| c.query(q))
}

#[tauri::command]
fn core_apply(store: State<'_, Store>, req: Value) -> Result<Value, Value> {
    with_core(&store, |c| c.apply(req, "user"))
}

/// True exactly once, on the very first launch with an empty database.
#[tauri::command]
fn is_first_run(store: State<'_, Store>) -> Result<bool, Value> {
    with_core(&store, |c| {
        let seen = c.meta_get("first_run_at")?.is_some() || c.meta_get("legacy_imported")?.is_some();
        if !seen {
            c.set_meta("first_run_at", &c.clock().now_iso())?;
        }
        Ok(!seen && c.is_empty()?)
    })
}

#[tauri::command]
async fn core_export(app: AppHandle, file_name: String) -> Result<bool, Value> {
    let snap = {
        let store = app.state::<Store>();
        with_core(&store, |c| c.query(json!({ "q": "snapshot" })))?
    };
    let picked = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .set_file_name(file_name)
        .blocking_save_file();
    let Some(fp) = picked else { return Ok(false) };
    let path = fp.into_path().map_err(|e| json!({ "code": "io_error", "message": e.to_string() }))?;
    let bytes = serde_json::to_vec_pretty(&snap).map_err(|e| json!({ "code": "io_error", "message": e.to_string() }))?;
    fs::write(&path, bytes).map_err(|e| json!({ "code": "io_error", "message": e.to_string() }))?;
    Ok(true)
}

#[tauri::command]
async fn core_import(app: AppHandle) -> Result<Option<Value>, Value> {
    let picked = app.dialog().file().add_filter("JSON", &["json"]).blocking_pick_file();
    let Some(fp) = picked else { return Ok(None) };
    let path = fp.into_path().map_err(|e| json!({ "code": "io_error", "message": e.to_string() }))?;
    let text = fs::read_to_string(&path).map_err(|e| json!({ "code": "io_error", "message": e.to_string() }))?;
    let data: Value = serde_json::from_str(&text).map_err(|e| json!({ "code": "invalid_json", "message": format!("不是有效的 JSON：{e}") }))?;
    let store = app.state::<Store>();
    with_core(&store, |c| c.import(data, "user")).map(Some)
}

#[tauri::command]
fn reveal_data_dir(store: State<'_, Store>) -> Result<(), String> {
    let dir = store.path.parent().map(Path::to_path_buf).unwrap_or_else(home);
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener).arg(&dir).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- AI access: CLI + skill ----------

use shiyi_core::install::{dir_on_path, is_our_cli, wrapper_for};

#[cfg(windows)]
fn cli_targets() -> Vec<PathBuf> {
    shiyi_core::install::windows_targets(&home())
}

#[cfg(not(windows))]
fn cli_targets() -> Vec<PathBuf> {
    vec![home().join(".local/bin/shiyi"), PathBuf::from("/usr/local/bin/shiyi"), PathBuf::from("/opt/homebrew/bin/shiyi")]
}

fn skill_path() -> PathBuf {
    home().join(".claude/skills/shiyi/SKILL.md")
}

#[tauri::command]
fn cli_status(store: State<'_, Store>) -> Value {
    let installed: Vec<String> = cli_targets()
        .into_iter()
        .filter(|p| fs::read_to_string(p).map(|t| is_our_cli(&t)).unwrap_or(false))
        .map(|p| p.display().to_string())
        .collect();
    let skill = skill_path();
    let skill_current = fs::read_to_string(&skill).map(|t| t == shiyi_core::SKILL_MD).unwrap_or(false);
    json!({
        "installed": installed,
        "exe": std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
        "skill_path": skill.display().to_string(),
        "skill_installed": skill.exists(),
        "skill_current": skill_current,
        "db": store.path.display().to_string(),
        "version": shiyi_core::VERSION
    })
}

#[tauri::command]
fn install_cli() -> Result<Value, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_s = exe.display().to_string();
    if cfg!(target_os = "macos") && (exe_s.contains("/AppTranslocation/") || exe_s.contains("/Volumes/")) {
        return Err("先把拾忆拖进「应用程序」文件夹并从那里打开，再安装命令行工具。".into());
    }
    let mut written = vec![];
    let mut errors = vec![];
    for (i, target) in cli_targets().into_iter().enumerate() {
        let dir = match target.parent() {
            Some(d) => d.to_path_buf(),
            None => continue,
        };
        if i == 0 {
            if let Err(e) = fs::create_dir_all(&dir) {
                errors.push(format!("{}: {e}", dir.display()));
                continue;
            }
        } else if !dir.is_dir() {
            continue;
        }
        // never overwrite somebody else's `shiyi`
        if target.exists() && !fs::read_to_string(&target).map(|t| is_our_cli(&t)).unwrap_or(false) {
            continue;
        }
        match fs::write(&target, wrapper_for(&target, &exe_s)) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&target, fs::Permissions::from_mode(0o755));
                }
                written.push(target.display().to_string());
            }
            Err(e) if i == 0 => errors.push(format!("{}: {e}", target.display())),
            Err(_) => {}
        }
    }
    if written.is_empty() {
        return Err(format!("没能写入命令行工具：{}", errors.join("；")));
    }
    let (on_default_path, path_hint) = path_advice(&written);
    Ok(json!({
        "paths": written,
        "on_default_path": on_default_path,
        "path_hint": path_hint
    }))
}

/// Whether the new command can already be found, and the line to run if not.
#[cfg(windows)]
fn path_advice(written: &[String]) -> (bool, String) {
    let dir = PathBuf::from(&written[0]).parent().map(Path::to_path_buf).unwrap_or_else(home);
    let on_path = std::env::var_os("PATH").map(|p| dir_on_path(&dir, &p)).unwrap_or(false);
    (on_path, shiyi_core::install::windows_path_hint(&dir))
}

/// Apps started from Finder get a short PATH, so only the system-wide
/// locations count as "already on the path".
#[cfg(not(windows))]
fn path_advice(written: &[String]) -> (bool, String) {
    let home = home().display().to_string();
    let _ = dir_on_path;
    (
        written.iter().any(|p| !p.starts_with(&home)),
        "echo 'export PATH=\"$HOME/.local/bin:$PATH\"' >> ~/.zshrc".into(),
    )
}

#[tauri::command]
fn install_skill(dir: Option<String>) -> Result<String, String> {
    let file = match dir.filter(|d| !d.trim().is_empty()) {
        Some(d) => PathBuf::from(d.trim()).join("shiyi").join("SKILL.md"),
        None => skill_path(),
    };
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&file, shiyi_core::SKILL_MD).map_err(|e| e.to_string())?;
    Ok(file.display().to_string())
}

#[tauri::command]
fn skill_text() -> String {
    shiyi_core::SKILL_MD.to_string()
}

// ---------- settings + AI proxy ----------

#[tauri::command]
fn get_settings(app: AppHandle, store: State<'_, Store>) -> Result<PublicSettings, String> {
    let s = read_settings(&app);
    Ok(PublicSettings {
        api_format: s.api_format,
        api_base: s.api_base,
        model: s.model,
        quick_model: s.quick_model,
        has_key: !s.api_key.trim().is_empty(),
        data_path: store.path.display().to_string(),
    })
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings, keep_key: bool) -> Result<(), String> {
    let mut s = settings;
    if keep_key {
        s.api_key = read_settings(&app).api_key;
    }
    let path = data_dir(&app)?.join("settings.json");
    let bytes = serde_json::to_vec_pretty(&s).map_err(|e| e.to_string())?;
    write_atomic(&path, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn net_err(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "请求超时，请稍后再试".into()
    } else if e.is_connect() {
        format!("连不上接口地址（检查网络、代理或接口地址）：{e}")
    } else {
        e.to_string()
    }
}

async fn read_json(res: reqwest::Response) -> Result<(reqwest::StatusCode, Value), String> {
    let status = res.status();
    let text = res.text().await.map_err(|e| e.to_string())?;
    let value = serde_json::from_str::<Value>(&text)
        .unwrap_or_else(|_| json!({ "raw": text.chars().take(300).collect::<String>() }));
    Ok((status, value))
}

fn api_err(status: reqwest::StatusCode, v: &Value) -> String {
    let msg = v["error"]["message"]
        .as_str()
        .or_else(|| v["error"].as_str())
        .or_else(|| v["message"].as_str())
        .or_else(|| v["raw"].as_str())
        .unwrap_or("");
    format!("HTTP {}：{}", status.as_u16(), msg)
}

#[tauri::command]
async fn ai_complete(app: AppHandle, prompt: String, quick: bool) -> Result<String, String> {
    let s = read_settings(&app);
    let key = s.api_key.trim().to_string();
    if key.is_empty() {
        return Err("NO_KEY".into());
    }
    let model = if quick && !s.quick_model.trim().is_empty() {
        s.quick_model.trim().to_string()
    } else {
        s.model.trim().to_string()
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    let base = s.api_base.trim().trim_end_matches('/').to_string();

    if s.api_format == "openai" {
        let base = if base.is_empty() { "https://api.openai.com/v1".to_string() } else { base };
        let body = json!({
            "model": model,
            "max_tokens": 4096,
            "messages": [{ "role": "user", "content": prompt }]
        });
        let res = client
            .post(format!("{base}/chat/completions"))
            .bearer_auth(&key)
            .json(&body)
            .send()
            .await
            .map_err(net_err)?;
        let (status, v) = read_json(res).await?;
        if !status.is_success() {
            return Err(api_err(status, &v));
        }
        v["choices"][0]["message"]["content"]
            .as_str()
            .map(|t| t.to_string())
            .ok_or_else(|| "接口返回里没有文本内容".to_string())
    } else {
        let base = if base.is_empty() { "https://api.anthropic.com".to_string() } else { base };
        let url = if base.ends_with("/v1") {
            format!("{base}/messages")
        } else {
            format!("{base}/v1/messages")
        };
        let body = json!({
            "model": model,
            "max_tokens": 4096,
            "messages": [{ "role": "user", "content": prompt }]
        });
        let res = client
            .post(url)
            .header("x-api-key", &key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(net_err)?;
        let (status, v) = read_json(res).await?;
        if !status.is_success() {
            return Err(api_err(status, &v));
        }
        let text: String = v["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        if text.is_empty() {
            Err("接口返回里没有文本内容".into())
        } else {
            Ok(text)
        }
    }
}

// ---------- app ----------

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Polls SQLite's data_version, which changes only when another process
/// (the `shiyi` CLI run by an agent) commits, and tells the window.
fn spawn_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last: Option<i64> = None;
        loop {
            std::thread::sleep(Duration::from_millis(800));
            let store = app.state::<Store>();
            let seen = match store.core.lock() {
                Ok(g) => g.as_ref().map(|c| (c.data_version(), c.max_seq())),
                Err(_) => None,
            };
            if let Some((version, seq)) = seen {
                if last.is_some_and(|l| l != version) {
                    let _ = app.emit("db-changed", json!({ "seq": seq }));
                }
                last = Some(version);
            }
        }
    });
}

fn open_store(app: &AppHandle) -> Store {
    let path = shiyi_core::default_db_path();
    match Core::open(&path) {
        Ok(mut core) => {
            if let Ok(dir) = data_dir(app) {
                match core.import_legacy_file_once(&dir.join("data.json")) {
                    Ok(Some(r)) => eprintln!("拾忆: imported data.json ({} items)", r["imported"]),
                    Ok(None) => {}
                    Err(e) => eprintln!("拾忆: data.json import failed: {e}"),
                }
            }
            if let Err(e) = core.backup_daily(KEEP_BACKUPS) {
                eprintln!("拾忆: backup failed: {e}");
            }
            Store { core: Mutex::new(Some(core)), error: None, path }
        }
        Err(e) => Store { core: Mutex::new(None), error: Some(e.to_string()), path },
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let store = open_store(app.handle());
            app.manage(store);
            spawn_watcher(app.handle().clone());
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                };
                // Alt+Shift switches the input language on Windows, so use Ctrl+Alt there
                let mods = if cfg!(windows) { Modifiers::CONTROL | Modifiers::ALT } else { Modifiers::ALT | Modifiers::SHIFT };
                let capture = Shortcut::new(Some(mods), Code::Space);
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |app, shortcut, event| {
                            if shortcut == &capture && event.state() == ShortcutState::Pressed {
                                show_main(app);
                                let _ = app.emit("quick-capture", ());
                            }
                        })
                        .build(),
                )?;
                if let Err(e) = app.global_shortcut().register(capture) {
                    eprintln!("global shortcut unavailable: {e}");
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // On macOS closing the window keeps the app alive so the
                // global shortcut still works; Cmd+Q quits for real.
                if cfg!(target_os = "macos")
                    && window.label() == "main"
                    && !QUITTING.load(Ordering::SeqCst)
                {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            core_query,
            core_apply,
            is_first_run,
            core_export,
            core_import,
            reveal_data_dir,
            cli_status,
            install_cli,
            install_skill,
            skill_text,
            get_settings,
            save_settings,
            ai_complete
        ])
        .build(tauri::generate_context!())
        .expect("error while building 拾忆");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = &event {
            QUITTING.store(true, Ordering::SeqCst);
        }
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { has_visible_windows, .. } = &event {
            if !*has_visible_windows {
                show_main(app_handle);
            }
        }
        let _ = (app_handle, event);
    });
}
