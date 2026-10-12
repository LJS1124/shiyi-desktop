//! 十一遗 desktop backend: local JSON storage with daily backups, settings,
//! AI proxy (Anthropic Messages or OpenAI-compatible), import/export dialogs,
//! and a global quick-capture shortcut.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

static QUITTING: AtomicBool = AtomicBool::new(false);
const KEEP_BACKUPS: usize = 14;

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

fn prune_backups(dir: &Path, keep: usize) {
    if let Ok(rd) = fs::read_dir(dir) {
        let mut files: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        files.sort();
        if files.len() > keep {
            let extra = files.len() - keep;
            for p in files.iter().take(extra) {
                let _ = fs::remove_file(p);
            }
        }
    }
}

#[tauri::command]
fn is_first_run(app: AppHandle) -> Result<bool, String> {
    Ok(!data_dir(&app)?.join("data.json").exists())
}

#[tauri::command]
fn load_data(app: AppHandle) -> Result<Value, String> {
    let path = data_dir(&app)?.join("data.json");
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("数据文件无法解析：{e}。可以在「设置 → 数据」里从备份恢复。"))
}

#[tauri::command]
fn save_data(app: AppHandle, data: Value) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let path = dir.join("data.json");
    let bytes = serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?;
    // keep the first version of each day before overwriting it
    let backups = dir.join("backups");
    fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let daily = backups.join(format!("data-{today}.json"));
    if !daily.exists() && path.exists() {
        let _ = fs::copy(&path, &daily);
        prune_backups(&backups, KEEP_BACKUPS);
    }
    write_atomic(&path, &bytes)
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<PublicSettings, String> {
    let s = read_settings(&app);
    let path = data_dir(&app)?.join("data.json");
    Ok(PublicSettings {
        api_format: s.api_format,
        api_base: s.api_base,
        model: s.model,
        quick_model: s.quick_model,
        has_key: !s.api_key.trim().is_empty(),
        data_path: path.to_string_lossy().to_string(),
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

#[tauri::command]
async fn export_data(app: AppHandle, data: Value, file_name: String) -> Result<bool, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .set_file_name(file_name)
        .blocking_save_file();
    let Some(fp) = picked else { return Ok(false) };
    let path = fp.into_path().map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?;
    fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
async fn import_data(app: AppHandle) -> Result<Option<Value>, String> {
    let picked = app.dialog().file().add_filter("JSON", &["json"]).blocking_pick_file();
    let Some(fp) = picked else { return Ok(None) };
    let path = fp.into_path().map_err(|e| e.to_string())?;
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("不是有效的 JSON：{e}"))
}

#[tauri::command]
fn reveal_data_dir(app: AppHandle) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(&dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                };
                let capture = Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Space);
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
            is_first_run,
            load_data,
            save_data,
            get_settings,
            save_settings,
            ai_complete,
            export_data,
            import_data,
            reveal_data_dir
        ])
        .build(tauri::generate_context!())
        .expect("error while building 十一遗");

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
