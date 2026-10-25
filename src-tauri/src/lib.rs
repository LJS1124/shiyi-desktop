//! 拾忆 desktop backend: the shiyi-core SQLite store (shared with the `shiyi`
//! CLI that AI agents use), a watcher that tells the window when an agent
//! changed something, settings, the AI proxy, import/export dialogs, CLI and
//! skill installers, the global quick-capture shortcut, the daily summary
//! notification, the Windows tray, launch at login and a small diagnostics log.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shiyi_core::Core;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

static QUITTING: AtomicBool = AtomicBool::new(false);
/// local UTC offset for log timestamps (taken from the database clock at start)
static LOG_OFFSET: AtomicI64 = AtomicI64::new(0);
const KEEP_BACKUPS: usize = 14;

struct Store {
    core: Mutex<Option<Core>>,
    error: Option<String>,
    path: PathBuf,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
struct Settings {
    // the provider in use; these five fields always mirror it, so the AI
    // call path reads one place
    api_format: String,
    api_base: String,
    api_key: String,
    model: String,
    quick_model: String,
    /// id from shiyi_core::providers::CATALOG
    provider: String,
    /// every provider the user has set up, so switching keeps their keys
    providers: std::collections::BTreeMap<String, ProviderCfg>,
    /// the morning summary notification and when it comes ("HH:MM")
    daily_summary: bool,
    daily_time: String,
    /// Windows: closing the window keeps 拾忆 running in the tray
    close_to_tray: bool,
    /// start at login (without showing the window)
    autostart: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
struct ProviderCfg {
    api_format: String,
    api_base: String,
    api_key: String,
    model: String,
    quick_model: String,
}

impl Settings {
    /// Settings from before providers existed: file the single setup under the
    /// provider its address belongs to.
    fn migrate(mut self) -> Self {
        if self.provider.is_empty() {
            self.provider = shiyi_core::providers::guess(&self.api_base, &self.api_format).to_string();
        }
        if !self.providers.contains_key(&self.provider) {
            self.providers.insert(
                self.provider.clone(),
                ProviderCfg {
                    api_format: self.api_format.clone(),
                    api_base: self.api_base.clone(),
                    api_key: self.api_key.clone(),
                    model: self.model.clone(),
                    quick_model: self.quick_model.clone(),
                },
            );
        }
        self
    }

    fn activate(&mut self, id: &str) {
        if let Some(c) = self.providers.get(id).cloned() {
            self.provider = id.to_string();
            self.api_format = c.api_format;
            self.api_base = c.api_base;
            self.api_key = c.api_key;
            self.model = c.model;
            self.quick_model = c.quick_model;
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api_format: "anthropic".into(),
            api_base: String::new(),
            api_key: String::new(),
            model: "claude-sonnet-5-5".into(),
            quick_model: "claude-haiku-5-5".into(),
            provider: String::new(),
            providers: Default::default(),
            daily_summary: true,
            daily_time: shiyi_core::remind::DEFAULT_TIME.into(),
            close_to_tray: true,
            autostart: false,
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
    provider: String,
    /// per provider: format, address, models, whether a key is saved and its masked form
    providers: Value,
    catalog: Value,
    /// reminders and background running
    prefs: Value,
}

fn prefs_json(s: &Settings) -> Value {
    json!({
        "dailySummary": s.daily_summary,
        "dailyTime": shiyi_core::remind::normalize_hhmm(&s.daily_time),
        "closeToTray": s.close_to_tray,
        "autostart": s.autostart,
        "tray": cfg!(windows),
        "canAutostart": cfg!(any(target_os = "macos", windows))
    })
}

fn write_settings(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let path = data_dir(app)?.join("settings.json");
    let bytes = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    write_atomic(&path, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
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
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default()
        .migrate()
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
    let providers: serde_json::Map<String, Value> = s
        .providers
        .iter()
        .map(|(id, c)| {
            (id.clone(), json!({
                "apiFormat": c.api_format, "apiBase": c.api_base, "model": c.model, "quickModel": c.quick_model,
                "hasKey": !c.api_key.trim().is_empty(), "keyHint": shiyi_core::providers::key_hint(&c.api_key)
            }))
        })
        .collect();
    Ok(PublicSettings {
        has_key: !s.api_key.trim().is_empty(),
        api_format: s.api_format,
        api_base: s.api_base,
        model: s.model,
        quick_model: s.quick_model,
        data_path: store.path.display().to_string(),
        provider: s.provider,
        providers: Value::Object(providers),
        catalog: shiyi_core::providers::catalog_json(),
        prefs: prefs_json(&s),
    })
}

/// Saves one provider's setup and (with `activate`) makes it the one in use.
/// `api_key: None` keeps the saved key; `Some("")` removes it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderInput {
    api_format: String,
    api_base: String,
    model: String,
    quick_model: String,
    api_key: Option<String>,
}

#[tauri::command]
fn save_provider(app: AppHandle, provider: String, cfg: ProviderInput, activate: bool) -> Result<(), String> {
    if shiyi_core::providers::get(&provider).is_none() {
        return Err(format!("未知的服务商：{provider}"));
    }
    let mut s = read_settings(&app);
    let old = s.providers.get(&provider).cloned().unwrap_or_default();
    let fmt = if cfg.api_format == "anthropic" { "anthropic" } else { "openai" };
    s.providers.insert(
        provider.clone(),
        ProviderCfg {
            api_format: fmt.into(),
            api_base: cfg.api_base.trim().to_string(),
            api_key: cfg.api_key.map(|k| k.trim().to_string()).unwrap_or(old.api_key),
            model: cfg.model.trim().to_string(),
            quick_model: cfg.quick_model.trim().to_string(),
        },
    );
    if activate || s.provider == provider {
        s.activate(&provider);
    }
    write_settings(&app, &s)
}

/// The models a provider offers (its `/models` endpoint), using the saved key.
#[tauri::command]
async fn list_models(app: AppHandle, provider: String) -> Result<Vec<String>, String> {
    let s = read_settings(&app);
    let c = s.providers.get(&provider).cloned().ok_or("先填好这个服务商的 Key 并保存")?;
    let url = shiyi_core::providers::models_url(&c.api_format, &c.api_base);
    let client = reqwest::Client::builder().timeout(Duration::from_secs(20)).build().map_err(|e| e.to_string())?;
    let mut req = client.get(&url);
    if !c.api_key.trim().is_empty() {
        req = if c.api_format == "anthropic" {
            req.header("x-api-key", c.api_key.trim()).header("anthropic-version", "2023-06-01")
        } else {
            req.bearer_auth(c.api_key.trim())
        };
    }
    let res = req.send().await.map_err(net_err)?;
    let status = res.status();
    let text = res.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        let v = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "raw": text.chars().take(300).collect::<String>() }));
        return Err(api_err(status, &v));
    }
    let models = shiyi_core::providers::parse_models(&text);
    if models.is_empty() {
        return Err("这个服务商没有返回模型列表，可以直接手动填写模型名".into());
    }
    Ok(models)
}

/// Opens a link in the default browser. Callers check the link first.
fn open_in_browser(url: &str) -> Result<(), String> {
    let mut cmd = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        std::process::Command::new("explorer")
    } else {
        std::process::Command::new("xdg-open")
    };
    cmd.arg(url).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Opens a provider's key page in the browser (allowlisted hosts only).
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    if !shiyi_core::providers::is_allowed_external(&url) {
        return Err("只能打开模型服务商的官方页面".into());
    }
    open_in_browser(&url)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings, keep_key: bool) -> Result<(), String> {
    // older single-provider form: apply to the provider in use
    let cur = read_settings(&app);
    let mut s = settings;
    if keep_key {
        s.api_key = cur.api_key.clone();
    }
    s.providers = cur.providers;
    s.provider = if s.provider.is_empty() { cur.provider } else { s.provider };
    (s.daily_summary, s.daily_time, s.close_to_tray, s.autostart) = (cur.daily_summary, cur.daily_time, cur.close_to_tray, cur.autostart);
    s.providers.insert(
        s.provider.clone(),
        ProviderCfg { api_format: s.api_format.clone(), api_base: s.api_base.clone(), api_key: s.api_key.clone(), model: s.model.clone(), quick_model: s.quick_model.clone() },
    );
    write_settings(&app, &s)
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
async fn ai_complete(app: AppHandle, prompt: String, quick: bool, provider: Option<String>) -> Result<String, String> {
    let r = ai_complete_inner(&app, prompt, quick, provider.clone()).await;
    if let Err(e) = &r {
        if e != "NO_KEY" {
            let who = provider.unwrap_or_else(|| read_settings(&app).provider);
            applog(&app, &format!("AI 调用失败（{who}）：{}", e.chars().take(160).collect::<String>()));
        }
    }
    r
}

async fn ai_complete_inner(app: &AppHandle, prompt: String, quick: bool, provider: Option<String>) -> Result<String, String> {
    let mut s = read_settings(app);
    // "测试连接" on a provider that isn't the one in use
    if let Some(id) = provider.filter(|id| s.providers.contains_key(id)) {
        s.activate(&id);
    }
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
    let openai = s.api_format == "openai";
    let (url, base_body) = if openai {
        let base = if base.is_empty() { "https://api.openai.com/v1".to_string() } else { base };
        (format!("{base}/chat/completions"), json!({ "model": model, "messages": [{ "role": "user", "content": prompt }] }))
    } else {
        let base = if base.is_empty() { "https://api.anthropic.com".to_string() } else { base };
        let url = if base.ends_with("/v1") { format!("{base}/messages") } else { format!("{base}/v1/messages") };
        (url, json!({ "model": model, "messages": [{ "role": "user", "content": prompt }] }))
    };

    // Start with a roomy budget (thinking models need it); if the provider
    // caps max_tokens lower, step down; if the reply is still cut off, step up.
    use shiyi_core::ai::{self as ai_text, NoText};
    let mut budget = ai_text::START_BUDGET;
    let mut ceiling: Option<u32> = None;
    let mut partial: Option<String> = None;
    let mut first_err: Option<String> = None;
    for _ in 0..4 {
        let mut body = base_body.clone();
        body["max_tokens"] = json!(budget);
        let req = client.post(&url).json(&body);
        let req = if openai {
            req.bearer_auth(&key)
        } else {
            req.header("x-api-key", &key).header("anthropic-version", "2023-06-01")
        };
        let res = req.send().await.map_err(net_err)?;
        let status = res.status();
        let text = res.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            let v = serde_json::from_str::<Value>(&text)
                .unwrap_or_else(|_| json!({ "raw": text.chars().take(300).collect::<String>() }));
            let err = api_err(status, &v);
            if ai_text::is_budget_rejection(status.as_u16(), &text) {
                if let Some(smaller) = ai_text::smaller_budget(budget) {
                    first_err.get_or_insert(err);
                    ceiling = Some(budget);
                    budget = smaller;
                    continue;
                }
            }
            return Err(first_err.unwrap_or(err));
        }
        match ai_text::reply_text(&s.api_format, &text) {
            Ok(t) => return Ok(t),
            Err(NoText::Truncated) => {}
            Err(NoText::Partial(t)) => partial = Some(t),
            Err(e) => return Err(e.message()),
        }
        match ai_text::larger_budget(budget, ceiling) {
            Some(b) => budget = b,
            None => break,
        }
    }
    // still cut off at the largest budget allowed: use what arrived
    match partial {
        Some(t) => Ok(t),
        None => Err(NoText::Truncated.message()),
    }
}

// ---------- updates ----------

/// Asks GitHub for the latest release. Never fails loudly: no network just
/// means no update is shown.
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Value, String> {
    let r = check_update_inner().await;
    if let Err(e) = &r {
        applog(&app, &format!("检查更新失败：{e}"));
    }
    r
}

async fn check_update_inner() -> Result<Value, String> {
    use shiyi_core::update as up;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent(format!("Shiyi/{}", shiyi_core::VERSION))
        .build()
        .map_err(|e| e.to_string())?;
    let os = if cfg!(target_os = "macos") { "macos" } else if cfg!(windows) { "windows" } else { "linux" };
    // 1) latest.json from the release downloads: no rate limit
    let mut first_err: Option<String> = None;
    match client.get(up::MANIFEST_URL).send().await {
        Ok(res) if res.status().is_success() => {
            let text = res.text().await.map_err(|e| e.to_string())?;
            if let Some(v) = serde_json::from_str::<Value>(&text).ok().and_then(|m| up::summarize_manifest(&m, shiyi_core::VERSION, os)) {
                return Ok(v);
            }
        }
        Ok(res) => {
            let s = res.status().as_u16();
            let body = res.text().await.unwrap_or_default();
            first_err = Some(up::http_reason(s, &body));
        }
        Err(e) => first_err = Some(net_err(e)),
    }
    // 2) the REST API (rate limited per IP), e.g. for a release without latest.json
    let res = client
        .get(up::LATEST_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| first_err.clone().unwrap_or_else(|| net_err(e)))?;
    let status = res.status();
    let text = res.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(up::http_reason(status.as_u16(), &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(up::summarize(&v, shiyi_core::VERSION, os))
}

/// Opens a page of this project (releases, downloads) in the browser.
#[tauri::command]
fn open_project_url(url: String) -> Result<(), String> {
    if !shiyi_core::update::is_project_url(&url) {
        return Err("只能打开拾忆的 GitHub 页面".into());
    }
    open_in_browser(&url)
}

/// The updater's public key from tauri.conf.json; empty until signing is set up,
/// in which case the app falls back to "download the installer".
fn updater_pubkey(app: &AppHandle) -> String {
    app.config().plugins.0.get("updater").and_then(|u| u["pubkey"].as_str()).unwrap_or("").trim().to_string()
}

/// Downloads the signed update, checks its signature against the public key
/// built into the app, installs it and restarts. Progress goes out as
/// `update-progress` events. The database is not touched here; the new
/// version backs it up on its first start (backup_on_upgrade).
#[tauri::command]
async fn update_install(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    if updater_pubkey(&app).is_empty() {
        return Err("这个版本还不支持自动更新，请下载安装包手动更新".into());
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("检查更新失败：{e}"))?
        .ok_or_else(|| "已经是最新版本".to_string())?;
    let mut done: u64 = 0;
    let progress = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                done += chunk as u64;
                let _ = progress.emit("update-progress", json!({ "done": done, "total": total }));
            },
            || {},
        )
        .await
        .map_err(|e| {
            let m = format!("下载或安装失败：{e}");
            applog(&app, &m);
            m
        })?;
    app.restart();
}

#[tauri::command]
fn app_info(app: AppHandle, store: State<'_, Store>) -> Value {
    let backups = store.path.parent().map(|p| p.join("backups").display().to_string()).unwrap_or_default();
    json!({
        "version": shiyi_core::VERSION,
        "repo": shiyi_core::update::REPO_URL,
        "data_path": store.path.display().to_string(),
        "backups": backups,
        "os": if cfg!(target_os = "macos") { "macos" } else if cfg!(windows) { "windows" } else { "linux" },
        "auto_update": !updater_pubkey(&app).is_empty() && !cfg!(target_os = "linux")
    })
}

// ---------- reminders, background running, diagnostics ----------

fn log_path(app: &AppHandle) -> Option<PathBuf> {
    data_dir(app).ok().map(|d| d.join("logs").join("shiyi.log"))
}

/// One line in the diagnostics log (also on stderr for `tauri dev`). Never
/// pass note contents, prompts or keys.
fn applog(app: &AppHandle, msg: &str) {
    eprintln!("拾忆: {msg}");
    if let Some(p) = log_path(app) {
        let ts = shiyi_core::time::iso_from_ms(shiyi_core::time::now_ms(), LOG_OFFSET.load(Ordering::Relaxed));
        let _ = shiyi_core::logfile::append(&p, &format!("{ts} {msg}"), shiyi_core::logfile::MAX_BYTES);
    }
}

fn notify(app: &AppHandle, title: &str, body: &str) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;
    app.notification().builder().title(title).body(body).show().map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrefsInput {
    daily_summary: Option<bool>,
    daily_time: Option<String>,
    close_to_tray: Option<bool>,
    autostart: Option<bool>,
}

/// Saves the reminder / background settings that were sent; returns them all.
#[tauri::command]
fn save_prefs(app: AppHandle, prefs: PrefsInput) -> Result<Value, String> {
    let mut s = read_settings(&app);
    if let Some(v) = prefs.daily_summary {
        s.daily_summary = v;
    }
    if let Some(t) = prefs.daily_time {
        if shiyi_core::remind::parse_hhmm(&t).is_none() {
            return Err("时间的格式是 09:00 这样".into());
        }
        s.daily_time = shiyi_core::remind::normalize_hhmm(&t);
    }
    if let Some(v) = prefs.close_to_tray {
        s.close_to_tray = v;
    }
    if let Some(v) = prefs.autostart {
        set_autostart(&app, v)?;
        s.autostart = v;
    }
    write_settings(&app, &s)?;
    Ok(prefs_json(&s))
}

fn set_autostart(app: &AppHandle, on: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    if !cfg!(any(target_os = "macos", windows)) {
        return Err("这个系统还不支持开机自启动".into());
    }
    if on && cfg!(target_os = "macos") {
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
        if exe.contains("/AppTranslocation/") || exe.contains("/Volumes/") {
            return Err("先把拾忆拖进「应用程序」文件夹并从那里打开，再开启开机自启动。".into());
        }
    }
    let al = app.autolaunch();
    let r = if on {
        al.enable()
    } else if al.is_enabled().unwrap_or(true) {
        al.disable()
    } else {
        Ok(())
    };
    r.map_err(|e| {
        let m = format!("设置开机自启动失败：{e}");
        applog(app, &m);
        m
    })
}

/// "发一条试试": today's summary right now (or a short note when there is nothing due).
#[tauri::command]
fn notify_test(app: AppHandle, store: State<'_, Store>) -> Result<String, String> {
    let today = with_core(&store, |c| c.query(json!({ "q": "today" }))).map_err(|e| e["message"].as_str().unwrap_or("读取数据失败").to_string())?;
    let (title, body) = shiyi_core::remind::daily_message(&today).unwrap_or_else(|| ("拾忆".into(), "通知正常。今天没有到期或逾期的任务。".into()));
    notify(&app, &title, &body).map_err(|e| {
        applog(&app, &format!("通知发送失败：{e}"));
        format!("通知没有发出去：{e}")
    })?;
    Ok(body)
}

/// Every 30 s: once a day at the chosen time, the summary notification. The
/// date it was shown is kept in the database, so restarts don't repeat it.
fn spawn_reminder(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(30));
        let s = read_settings(&app);
        if !s.daily_summary {
            continue;
        }
        let msg = {
            let store = app.state::<Store>();
            let Ok(guard) = store.core.lock() else { continue };
            let Some(c) = guard.as_ref() else { continue };
            let clock = c.clock();
            let now = shiyi_core::remind::minutes_of_day(clock.now_ms, clock.offset_s);
            let today = clock.today();
            let last = c.meta_get(shiyi_core::remind::META_LAST).ok().flatten();
            if !shiyi_core::remind::is_due(now, &s.daily_time, &today, last.as_deref()) {
                continue;
            }
            let _ = c.set_meta(shiyi_core::remind::META_LAST, &today);
            c.query(json!({ "q": "today" })).ok().and_then(|v| shiyi_core::remind::daily_message(&v))
        };
        if let Some((title, body)) = msg {
            if let Err(e) = notify(&app, &title, &body) {
                applog(&app, &format!("通知发送失败：{e}"));
            }
        }
    });
}

/// Windows: an icon in the notification area; left click opens the window.
fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let open = MenuItem::with_id(app, "open", "打开拾忆", true, None::<&str>)?;
    let capture = MenuItem::with_id(app, "capture", "快速记录", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出拾忆", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &capture, &sep, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("拾忆")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "capture" => {
                show_main(app);
                let _ = app.emit("quick-capture", ());
            }
            "quit" => {
                QUITTING.store(true, Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// The first time the window goes to the tray, say where it went.
fn tray_hint_once(app: &AppHandle) {
    let store = app.state::<Store>();
    let first = with_core(&store, |c| {
        let seen = c.meta_get("tray_hint_shown")?.is_some();
        if !seen {
            c.set_meta("tray_hint_shown", "1")?;
        }
        Ok(!seen)
    })
    .unwrap_or(false);
    if first {
        let _ = notify(app, "拾忆还在后台运行", "提醒和全局快捷键照常工作。点任务栏右下角的拾忆图标打开，右键菜单里可以退出。");
    }
}

/// What "复制诊断信息" shows: versions, counts and settings, never note
/// contents or keys.
#[tauri::command]
fn diagnostics(app: AppHandle, store: State<'_, Store>) -> Value {
    let s = read_settings(&app);
    let status = with_core(&store, |c| c.query(json!({ "q": "status" }))).unwrap_or_else(|e| json!({ "error": e }));
    let os_version = if cfg!(target_os = "macos") {
        std::process::Command::new("sw_vers").arg("-productVersion").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
    } else {
        String::new()
    };
    let cli = cli_targets().into_iter().any(|p| fs::read_to_string(p).map(|t| is_our_cli(&t)).unwrap_or(false));
    let skill = skill_path();
    let backups = store.path.parent().map(|p| p.join("backups"));
    let last_backup = backups
        .and_then(|d| fs::read_dir(d).ok())
        .and_then(|it| {
            it.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter_map(|n| n.strip_prefix("shiyi-").and_then(|d| d.strip_suffix(".db")).map(str::to_string))
                .max()
        });
    json!({
        "version": shiyi_core::VERSION,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "osVersion": os_version,
        "counts": status["counts"],
        "permission": status["agent_permission"],
        "dbBytes": fs::metadata(&store.path).map(|m| m.len()).unwrap_or(0),
        "dbError": store.error,
        "lastBackup": last_backup,
        "provider": s.provider,
        "hasKey": !s.api_key.trim().is_empty(),
        "format": s.api_format,
        "model": s.model,
        "quickModel": s.quick_model,
        "cli": cli,
        "skill": skill.exists(),
        "skillCurrent": fs::read_to_string(&skill).map(|t| t == shiyi_core::SKILL_MD).unwrap_or(false),
        "autoUpdate": !updater_pubkey(&app).is_empty(),
        "prefs": prefs_json(&s),
        "log": log_path(&app).map(|p| shiyi_core::logfile::tail(&p, 20)).unwrap_or_default()
    })
}

/// Opens "New issue" on GitHub with the title and text filled in.
#[tauri::command]
fn open_feedback(title: String, body: String) -> Result<(), String> {
    open_in_browser(&shiyi_core::update::issue_url(&title, &body))
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
            // before anything else touches the database after an update
            LOG_OFFSET.store(core.clock().offset_s, Ordering::Relaxed);
            applog(app, &format!("启动 v{}（{}/{}）", shiyi_core::VERSION, std::env::consts::OS, std::env::consts::ARCH));
            match core.backup_on_upgrade(shiyi_core::VERSION) {
                Ok(Some(p)) => applog(app, &format!("升级前备份：{}", p.display())),
                Ok(None) => {}
                Err(e) => applog(app, &format!("升级前备份失败：{e}")),
            }
            if let Ok(dir) = data_dir(app) {
                match core.import_legacy_file_once(&dir.join("data.json")) {
                    Ok(Some(r)) => applog(app, &format!("导入旧版 data.json：{} 条", r["imported"])),
                    Ok(None) => {}
                    Err(e) => applog(app, &format!("导入旧版 data.json 失败：{e}")),
                }
            }
            if let Err(e) = core.backup_daily(KEEP_BACKUPS) {
                applog(app, &format!("每日备份失败：{e}"));
            }
            Store { core: Mutex::new(Some(core)), error: None, path }
        }
        Err(e) => {
            applog(app, &format!("打不开数据库：{e}"));
            Store { core: Mutex::new(None), error: Some(e.to_string()), path }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    // the updater needs the public key; without it the app offers manual download
    let has_updater_key = context
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u["pubkey"].as_str())
        .is_some_and(|k| !k.trim().is_empty());
    // a second launch (Start menu while 拾忆 sits in the tray) brings back the first
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)));
    if has_updater_key {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }
    let app = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--autostart"])))
        .setup(|app| {
            // the window starts hidden; launched at login it stays that way
            let at_login = std::env::args().any(|a| a == "--autostart") && cfg!(any(target_os = "macos", windows));
            if !at_login {
                show_main(app.handle());
            }
            let store = open_store(app.handle());
            app.manage(store);
            spawn_watcher(app.handle().clone());
            spawn_reminder(app.handle().clone());
            if cfg!(windows) {
                if let Err(e) = build_tray(app.handle()) {
                    applog(app.handle(), &format!("托盘图标创建失败：{e}"));
                }
            }
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
                // Closing the window keeps the app alive so reminders and the
                // global shortcut still work: always on macOS (Cmd+Q quits),
                // on Windows unless turned off (the tray menu quits).
                if window.label() == "main" && !QUITTING.load(Ordering::SeqCst) {
                    let app = window.app_handle();
                    let keep = cfg!(target_os = "macos") || (cfg!(windows) && read_settings(app).close_to_tray);
                    if keep {
                        let _ = window.hide();
                        api.prevent_close();
                        if cfg!(windows) {
                            tray_hint_once(app);
                        }
                    }
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
            ai_complete,
            check_update,
            open_project_url,
            app_info,
            update_install,
            save_provider,
            list_models,
            open_external,
            save_prefs,
            notify_test,
            diagnostics,
            open_feedback
        ])
        .build(context)
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
