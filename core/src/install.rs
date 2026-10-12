//! Pieces of installing the `shiyi` command that don't depend on Tauri, so
//! they can be tested without a window system: the home directory, and the
//! small wrapper scripts that call back into the app's executable.

use std::path::{Path, PathBuf};

/// First line of every wrapper we write; used to tell our file from somebody
/// else's `shiyi`.
pub const CLI_MARK: &str = "# 拾忆命令行";
/// Marker written by versions before the rename, still recognised as ours.
pub const CLI_MARK_OLD: &str = "# 十一遗命令行";

pub fn is_our_cli(text: &str) -> bool {
    text.contains(CLI_MARK) || text.contains(CLI_MARK_OLD)
}

/// The user's home directory. `HOME` first (also set by Git Bash and tests),
/// then `USERPROFILE` (Windows). `None` when neither is set.
pub fn home_dir() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .iter()
        .filter_map(|k| std::env::var_os(k))
        .find(|v| !v.is_empty())
        .map(PathBuf::from)
}

pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn looks_like_windows_path(p: &str) -> bool {
    let b = p.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')) || p.starts_with("\\\\")
}

/// `/bin/sh` wrapper (macOS, Linux, and Git Bash on Windows).
pub fn sh_wrapper(exe: &str) -> String {
    // Git Bash understands `C:/Users/…` but not `C:\Users\…` inside quotes.
    // (A backslash is a legal file name character elsewhere, so only
    // paths that look like Windows paths are converted.)
    let exe = if looks_like_windows_path(exe) { exe.replace('\\', "/") } else { exe.to_string() };
    format!(
        "#!/bin/sh\n{CLI_MARK}（由拾忆安装；重新安装应用后在 设置 → AI 接入 里再点一次安装）\nexec {} --cli \"$@\"\n",
        sh_quote(&exe)
    )
}

/// `shiyi.cmd` for cmd.exe and PowerShell. The code page is switched to UTF-8
/// first so a path with Chinese characters is read correctly; the exit code of
/// the app is passed on to the caller.
pub fn cmd_wrapper(exe: &str) -> String {
    // `%` is expanded by cmd even inside quotes, and `"` can't be in a path.
    let exe = exe.replace('"', "").replace('%', "%%");
    format!(
        "@echo off\r\nchcp 65001 >nul\r\nrem {CLI_MARK}（由拾忆安装；重新安装应用后在 设置 → AI 接入 里再点一次安装）\r\n\"{exe}\" --cli %*\r\nexit /b %ERRORLEVEL%\r\n"
    )
}

/// The wrapper text for a given target file: `.cmd` files get the batch
/// version, everything else the shell version.
pub fn wrapper_for(path: &Path, exe: &str) -> String {
    let is_cmd = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("cmd"))
        .unwrap_or(false);
    if is_cmd {
        cmd_wrapper(exe)
    } else {
        sh_wrapper(exe)
    }
}

/// Where wrappers go on Windows: next to each other in `~/.local/bin`, which
/// uv, pipx and Claude Code also use there. Both `shiyi.cmd` (cmd, PowerShell)
/// and an extension-less `shiyi` (Git Bash) are written.
pub fn windows_targets(home: &Path) -> Vec<PathBuf> {
    let dir = home.join(".local").join("bin");
    vec![dir.join("shiyi.cmd"), dir.join("shiyi")]
}

/// A PowerShell line that puts `dir` on the user's PATH; running it twice
/// adds the directory only once.
pub fn windows_path_hint(dir: &Path) -> String {
    let d = dir.display().to_string().replace('\'', "''");
    format!(
        "$p=[Environment]::GetEnvironmentVariable('Path','User'); if (\";$p;\" -notlike '*;{d};*') {{ [Environment]::SetEnvironmentVariable('Path', ((@($p,'{d}') | Where-Object {{ $_ }}) -join ';'), 'User') }}"
    )
}

/// Is `dir` one of the entries of a PATH-style list? Ignores a trailing
/// separator, and case on Windows.
pub fn dir_on_path(dir: &Path, path_var: &std::ffi::OsStr) -> bool {
    let norm = |p: &Path| p.display().to_string().trim_end_matches(['/', '\\']).to_string();
    let norm = |p: &Path| if cfg!(windows) { norm(p).to_lowercase() } else { norm(p) };
    let want = norm(dir);
    std::env::split_paths(path_var).any(|p| norm(&p) == want)
}
