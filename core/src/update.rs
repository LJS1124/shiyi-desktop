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

/// The manifest published with every release (also what the in-app updater
/// reads). Served from GitHub's download CDN, so unlike the REST API it has no
/// per-IP rate limit.
pub const MANIFEST_URL: &str = "https://github.com/LJS1124/shiyi-desktop/releases/latest/download/latest.json";

/// Same shape as [`summarize`], built from `latest.json`:
/// `{version, notes, pub_date, platforms: {"darwin-aarch64": {url, signature}, …}}`.
pub fn summarize_manifest(m: &Value, current: &str, os: &str) -> Option<Value> {
    let version = m["version"].as_str()?.trim().trim_start_matches(['v', 'V']).to_string();
    parse_version(&version)?;
    let tag = format!("v{version}");
    let base = format!("{REPO_URL}/releases/download/{tag}/");
    // the installer people download by hand (the updater packages are not meant for that)
    let (asset_name, asset_url) = match os {
        "macos" => {
            let n = format!("Shiyi_{version}_universal.dmg");
            (Some(n.clone()), Some(format!("{base}{n}")))
        }
        "windows" => match m["platforms"]["windows-x86_64"]["url"].as_str() {
            Some(u) if u.starts_with(&format!("{REPO_URL}/")) => (u.rsplit('/').next().map(str::to_string), Some(u.to_string())),
            _ => {
                let n = format!("Shiyi_{version}_x64-setup.exe");
                (Some(n.clone()), Some(format!("{base}{n}")))
            }
        },
        _ => (None, None),
    };
    Some(json!({
        "available": is_newer(&version, current),
        "current": current,
        "version": version,
        "tag": tag,
        "title": format!("拾忆 {tag}"),
        "notes": m["notes"].as_str().unwrap_or("").chars().take(4000).collect::<String>(),
        "published_at": m["pub_date"].as_str().unwrap_or(""),
        "page": format!("{REPO_URL}/releases/tag/{tag}"),
        "asset_name": asset_name,
        "asset_url": asset_url,
    }))
}

/// A readable reason for a failed request to GitHub.
pub fn http_reason(status: u16, body: &str) -> String {
    let b = body.to_lowercase();
    if status == 429 || (status == 403 && (b.contains("rate limit") || b.is_empty() || b.contains("api rate"))) {
        return format!("HTTP {status}：GitHub 限制了这个网络的访问频率（同一出口 IP 每小时 60 次，用代理或公司网络时常见），稍后再试");
    }
    if status == 403 {
        return format!("HTTP {status}：GitHub 拒绝了这次请求，可能是网络或代理拦截");
    }
    if status == 404 {
        return format!("HTTP {status}：没有找到发布信息");
    }
    format!("HTTP {status}")
}

/// Percent-encodes everything except RFC 3986 unreserved characters.
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Longest link we hand to the browser; GitHub refuses much longer ones.
pub const MAX_ISSUE_URL: usize = 7000;

/// "New issue" on this project with the title and body filled in. A body that
/// would make the link too long is cut, with a note to paste the rest.
pub fn issue_url(title: &str, body: &str) -> String {
    let head = format!("{REPO_URL}/issues/new?title={}&body=", percent_encode(title.trim()));
    let full = percent_encode(body);
    if head.len() + full.len() <= MAX_ISSUE_URL {
        return head + &full;
    }
    let note = percent_encode("\n\n…（内容太长，其余部分已复制，请直接粘贴在这里）");
    let room = MAX_ISSUE_URL.saturating_sub(head.len() + note.len());
    let mut cut = String::new();
    for c in body.chars() {
        let e = percent_encode(c.encode_utf8(&mut [0u8; 4]));
        if cut.len() + e.len() > room {
            break;
        }
        cut.push_str(&e);
    }
    head + &cut + &note
}
