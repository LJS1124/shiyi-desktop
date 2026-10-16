//! Checking GitHub Releases for a newer version. The network call lives in the
//! desktop shell; this part only reads the release JSON, so it can be tested.

use serde_json::{json, Value};

pub const REPO_URL: &str = "https://github.com/LJS1124/shiyi-desktop";
pub const LATEST_API: &str = "https://api.github.com/repos/LJS1124/shiyi-desktop/releases/latest";

/// `v1.2.3` / `1.2.3` / `1.2.3-beta` → (1, 2, 3). Missing parts count as 0.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>());
    let major = it.next()?.ok()?;
    let minor = it.next().unwrap_or(Ok(0)).ok()?;
    let patch = it.next().unwrap_or(Ok(0)).ok()?;
    Some((major, minor, patch))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// The installer for this platform: `macos` → `.dmg`, `windows` → `*-setup.exe`
/// (or `.msi`). Returns (name, download url).
pub fn pick_asset(release: &Value, os: &str) -> Option<(String, String)> {
    let assets = release["assets"].as_array()?;
    let wanted = |name: &str| {
        let n = name.to_lowercase();
        match os {
            "macos" => n.ends_with(".dmg"),
            "windows" => n.ends_with("-setup.exe") || n.ends_with(".msi"),
            _ => false,
        }
    };
    assets.iter().find_map(|a| {
        let name = a["name"].as_str()?;
        let url = a["browser_download_url"].as_str()?;
        wanted(name).then(|| (name.to_string(), url.to_string()))
    })
}

/// What the app shows: whether an update is available, its version, notes and
/// where to get it. Drafts and pre-releases are never offered.
pub fn summarize(release: &Value, current: &str, os: &str) -> Value {
    let tag = release["tag_name"].as_str().unwrap_or("");
    let usable = !release["draft"].as_bool().unwrap_or(false) && !release["prerelease"].as_bool().unwrap_or(false);
    let available = usable && is_newer(tag, current);
    let asset = pick_asset(release, os);
    json!({
        "available": available,
        "current": current,
        "version": tag.trim_start_matches(['v', 'V']),
        "tag": tag,
        "title": release["name"].as_str().unwrap_or(tag),
        "notes": release["body"].as_str().unwrap_or("").chars().take(4000).collect::<String>(),
        "published_at": release["published_at"].as_str().unwrap_or(""),
        "page": release["html_url"].as_str().unwrap_or(REPO_URL),
        "asset_name": asset.as_ref().map(|a| a.0.clone()),
        "asset_url": asset.map(|a| a.1),
    })
}

/// Links the app may open in the browser: only this project on GitHub.
pub fn is_project_url(url: &str) -> bool {
    // no `..` (browsers would resolve it out of the repo), backslashes or credentials
    let lower = url.to_lowercase();
    if url.contains("..") || lower.contains("%2e") || lower.contains("%2f") || lower.contains("%5c") || url.contains('\\') || url.contains('@') || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    url == REPO_URL || url.starts_with(&format!("{REPO_URL}/"))
}
