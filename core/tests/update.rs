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

#[test]
fn manifest_summary() {
    use shiyi_core::update::{summarize_manifest, REPO_URL};
    let m: serde_json::Value = serde_json::from_str(r#"{"version":"1.2.2","notes":"拾忆 1.2.2：改进","pub_date":"2026-10-22T08:00:00Z",
      "platforms":{"darwin-aarch64":{"url":"https://github.com/LJS1124/shiyi-desktop/releases/download/v1.2.2/Shiyi.app.tar.gz","signature":"x"},
                   "windows-x86_64":{"url":"https://github.com/LJS1124/shiyi-desktop/releases/download/v1.2.2/Shiyi_1.2.2_x64-setup.exe","signature":"y"}}}"#).unwrap();
    let mac = summarize_manifest(&m, "1.2.1", "macos").unwrap();
    assert_eq!(mac["available"], true);
    assert_eq!(mac["version"], "1.2.2");
    assert_eq!(mac["asset_url"], format!("{REPO_URL}/releases/download/v1.2.2/Shiyi_1.2.2_universal.dmg"));
    assert_eq!(mac["page"], format!("{REPO_URL}/releases/tag/v1.2.2"));
    let win = summarize_manifest(&m, "1.2.2", "windows").unwrap();
    assert_eq!(win["available"], false);
    assert_eq!(win["asset_name"], "Shiyi_1.2.2_x64-setup.exe");
    assert!(summarize_manifest(&serde_json::json!({"notes": "x"}), "1.0.0", "macos").is_none());
    assert!(summarize_manifest(&serde_json::json!({"version": "abc"}), "1.0.0", "macos").is_none());
}

#[test]
fn http_reasons_are_readable() {
    use shiyi_core::update::http_reason;
    assert!(http_reason(403, r#"{"message":"API rate limit exceeded for 1.2.3.4."}"#).contains("访问频率"));
    assert!(http_reason(429, "").contains("访问频率"));
    assert!(http_reason(403, "<html>blocked by proxy</html>").contains("拒绝"));
    assert!(http_reason(404, "").contains("没有找到"));
}

// ---------- percent_encode / issue_url ----------

#[test]
fn percent_encode_keeps_unreserved_characters() {
    use shiyi_core::update::percent_encode;
    assert_eq!(percent_encode("a-Z_9.~"), "a-Z_9.~");
}

#[test]
fn percent_encode_escapes_space_slash_utf8_and_reserved() {
    use shiyi_core::update::percent_encode;
    assert_eq!(percent_encode(" "), "%20");
    assert_eq!(percent_encode("/"), "%2F");
    assert_eq!(percent_encode("中"), "%E4%B8%AD");
    assert_eq!(percent_encode("&=?#"), "%26%3D%3F%23");
}

#[test]
fn issue_url_fills_title_and_body() {
    use shiyi_core::update::issue_url;
    let url = issue_url("Bug: x", "hello world");
    assert!(url.starts_with("https://github.com/LJS1124/shiyi-desktop/issues/new?title=Bug%3A%20x&body="), "{url}");
    assert!(url.ends_with("hello%20world"), "{url}");
}

#[test]
fn issue_url_cuts_a_long_body_and_adds_a_note() {
    use shiyi_core::update::{issue_url, percent_encode, MAX_ISSUE_URL};
    const PREFIX: &str = "https://github.com/LJS1124/shiyi-desktop/issues/new?title=Bug%3A%20x&body=";
    let url = issue_url("Bug: x", &"中".repeat(5000));
    let note = percent_encode("\n\n…（内容太长，其余部分已复制，请直接粘贴在这里）");

    assert!(url.len() <= MAX_ISSUE_URL, "len {}", url.len());
    // the cut fills the link to within one character (9 encoded bytes) of the limit
    assert!(url.len() > MAX_ISSUE_URL - 9, "len {}", url.len());
    assert!(url.starts_with(PREFIX), "{url}");
    assert!(url.ends_with(&note), "{url}");
    assert!(url.contains(&percent_encode("内容太长")), "{url}");

    // the body part is whole characters only: every cut char is one full "%E4%B8%AD"
    let cut = &url[PREFIX.len()..url.len() - note.len()];
    assert_eq!(cut.matches("%E4%B8%AD").count() * 9, cut.len(), "cut: {cut}");
}
