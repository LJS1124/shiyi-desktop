use serde_json::{json, Value};
use shiyi_core::update::{is_newer, is_project_url, parse_version, pick_asset, summarize, REPO_URL};

fn release() -> Value {
    json!({
        "tag_name": "v1.2.0",
        "name": "拾忆 1.2.0",
        "body": "更新说明",
        "draft": false,
        "prerelease": false,
        "published_at": "2026-10-10T08:00:00Z",
        "html_url": "https://github.com/LJS1124/shiyi-desktop/releases/tag/v1.2.0",
        "assets": [
            {"name": "Shiyi_1.2.0_universal.dmg", "browser_download_url": "https://example.test/dl/Shiyi_1.2.0_universal.dmg"},
            {"name": "Shiyi.app.zip", "browser_download_url": "https://example.test/dl/Shiyi.app.zip"},
            {"name": "Shiyi_1.2.0_x64-setup.exe", "browser_download_url": "https://example.test/dl/Shiyi_1.2.0_x64-setup.exe"}
        ]
    })
}

// ---------- parse_version ----------

#[test]
fn parse_version_accepts_common_forms() {
    assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
    assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
    assert_eq!(parse_version("V2.0"), Some((2, 0, 0)));
    assert_eq!(parse_version("1"), Some((1, 0, 0)));
    assert_eq!(parse_version("1.2.3-beta.1"), Some((1, 2, 3)));
    assert_eq!(parse_version("1.2.3+build"), Some((1, 2, 3)));
}

#[test]
fn parse_version_rejects_garbage() {
    assert_eq!(parse_version(""), None);
    assert_eq!(parse_version("vx"), None);
    assert_eq!(parse_version("1.a.3"), None);
}

// ---------- is_newer ----------

#[test]
fn is_newer_compares_numerically() {
    assert!(is_newer("1.1.1", "1.1.0"));
    assert!(is_newer("1.10.0", "1.9.9"));
    assert!(!is_newer("1.1.0", "1.1.1"));
    assert!(!is_newer("1.1.1", "1.1.1"));
}

#[test]
fn is_newer_is_false_for_garbage() {
    assert!(!is_newer("garbage", "1.0.0"));
    assert!(!is_newer("1.0.0", "garbage"));
    assert!(!is_newer("", ""));
}

// ---------- pick_asset ----------

#[test]
fn pick_asset_chooses_installer_per_platform() {
    let r = release();
    let (name, url) = pick_asset(&r, "macos").unwrap();
    assert_eq!(name, "Shiyi_1.2.0_universal.dmg");
    assert_eq!(url, "https://example.test/dl/Shiyi_1.2.0_universal.dmg");

    let (name, url) = pick_asset(&r, "windows").unwrap();
    assert_eq!(name, "Shiyi_1.2.0_x64-setup.exe");
    assert_eq!(url, "https://example.test/dl/Shiyi_1.2.0_x64-setup.exe");

    assert_eq!(pick_asset(&r, "linux"), None);
}

#[test]
fn pick_asset_without_assets_is_none() {
    let mut r = release();
    r.as_object_mut().unwrap().remove("assets");
    assert_eq!(pick_asset(&r, "macos"), None);
    assert_eq!(pick_asset(&r, "windows"), None);
}

// ---------- summarize ----------

#[test]
fn summarize_offers_newer_release() {
    let s = summarize(&release(), "1.1.1", "macos");
    assert_eq!(s["available"], true);
    assert_eq!(s["current"], "1.1.1");
    assert_eq!(s["version"], "1.2.0");
    assert_eq!(s["tag"], "v1.2.0");
    assert_eq!(s["title"], "拾忆 1.2.0");
    assert_eq!(s["asset_name"], "Shiyi_1.2.0_universal.dmg");
    assert_eq!(s["asset_url"], "https://example.test/dl/Shiyi_1.2.0_universal.dmg");
}

#[test]
fn summarize_same_tag_is_not_available() {
    let s = summarize(&release(), "1.2.0", "macos");
    assert_eq!(s["available"], false);
}

#[test]
fn summarize_never_offers_drafts_or_prereleases() {
    let mut draft = release();
    draft["draft"] = json!(true);
    assert_eq!(summarize(&draft, "1.1.1", "macos")["available"], false);

    let mut pre = release();
    pre["prerelease"] = json!(true);
    assert_eq!(summarize(&pre, "1.1.1", "macos")["available"], false);
}

#[test]
fn summarize_version_has_no_leading_v() {
    let mut r = release();
    r["tag_name"] = json!("V3.4.5");
    assert_eq!(summarize(&r, "1.0.0", "linux")["version"], "3.4.5");
}

#[test]
fn summarize_caps_notes_at_4000_chars() {
    let mut r = release();
    r["body"] = json!("字".repeat(5000));
    let s = summarize(&r, "1.1.1", "macos");
    assert_eq!(s["notes"].as_str().unwrap().chars().count(), 4000);
}

#[test]
fn summarize_page_falls_back_to_repo_url() {
    let mut r = release();
    r.as_object_mut().unwrap().remove("html_url");
    assert_eq!(summarize(&r, "1.1.1", "macos")["page"], REPO_URL);
}

// ---------- is_project_url ----------

#[test]
fn is_project_url_allows_only_this_project() {
    assert!(is_project_url(REPO_URL));
    assert!(is_project_url(&format!("{REPO_URL}/releases/tag/v1.2.0")));

    assert!(!is_project_url("https://github.com/LJS1124/shiyi-desktop-evil"));
    assert!(!is_project_url("https://github.com/other/repo"));
    assert!(!is_project_url("http://github.com/LJS1124/shiyi-desktop"));
}

#[test]
fn project_url_rejects_traversal_and_tricks() {
    use shiyi_core::update::{is_project_url, REPO_URL};
    assert!(!is_project_url(&format!("{REPO_URL}/../../other/repo")));
    assert!(!is_project_url(&format!("{REPO_URL}/releases/%2e%2e/x\\y")));
    assert!(!is_project_url(&format!("{REPO_URL}/a b")));
    assert!(is_project_url(&format!("{REPO_URL}/releases/download/v1.2.0/Shiyi_1.2.0_universal.dmg")));
}
