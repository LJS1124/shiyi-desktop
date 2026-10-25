//! A small rolling log for the desktop app (`logs/shiyi.log` in the data
//! folder): startup, backups, failed AI or update requests. It never holds
//! note contents, prompts or keys; "复制诊断信息" shows its last lines.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub const MAX_BYTES: u64 = 256 * 1024;

/// Appends one line; when the file grows past `max_bytes` it becomes
/// `shiyi.log.1` (replacing the older one) and a new file starts.
pub fn append(path: &Path, line: &str, max_bytes: u64) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    if fs::metadata(path).map(|m| m.len() >= max_bytes).unwrap_or(false) {
        let old = path.with_extension("log.1");
        let _ = fs::remove_file(&old);
        fs::rename(path, &old)?;
    }
    let one_line: String = line.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).take(600).collect();
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{one_line}")
}

/// The last `n` lines (empty when there is no log yet).
pub fn tail(path: &Path, n: usize) -> Vec<String> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].iter().map(|s| s.to_string()).collect()
}
