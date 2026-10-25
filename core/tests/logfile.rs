use shiyi_core::logfile::{append, tail};
use std::path::{Path, PathBuf};

/// A unique temp dir that is removed when the guard drops (also on panic).
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("shiyi-logfile-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    /// `logs/shiyi.log` inside the guard's folder; `logs/` does not exist yet.
    fn log(&self) -> PathBuf {
        self.0.join("logs").join("shiyi.log")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn old_of(log: &Path) -> PathBuf {
    log.with_file_name("shiyi.log.1")
}

/// 29 bytes, so each appended line is 30 bytes with its newline.
fn entry(i: usize) -> String {
    format!("{i:02} {}", "x".repeat(26))
}

#[test]
fn append_creates_the_folder_and_the_file() {
    let t = TempDir::new("create");
    assert!(!t.log().parent().unwrap().exists());
    append(&t.log(), "启动", 1024).unwrap();
    assert!(t.log().is_file());
    assert_eq!(tail(&t.log(), 10), vec!["启动"]);
}

#[test]
fn tail_returns_lines_in_order() {
    let t = TempDir::new("order");
    for i in 0..5 {
        append(&t.log(), &format!("line {i}"), 1024).unwrap();
    }
    assert_eq!(tail(&t.log(), 10), vec!["line 0", "line 1", "line 2", "line 3", "line 4"]);
}

#[test]
fn tail_returns_only_the_last_n() {
    let t = TempDir::new("last-n");
    for i in 0..5 {
        append(&t.log(), &format!("line {i}"), 1024).unwrap();
    }
    assert_eq!(tail(&t.log(), 2), vec!["line 3", "line 4"]);
    assert!(tail(&t.log(), 0).is_empty());
}

#[test]
fn tail_of_a_missing_file_is_empty() {
    let t = TempDir::new("missing");
    assert!(tail(&t.log(), 5).is_empty());
}

#[test]
fn newlines_inside_a_line_become_spaces() {
    let t = TempDir::new("newlines");
    append(&t.log(), "第一行\n第二行\r\n第三行", 1024).unwrap();
    append(&t.log(), "next", 1024).unwrap();
    // \n -> space, \r\n -> two spaces; still one entry per call
    assert_eq!(tail(&t.log(), 10), vec!["第一行 第二行  第三行", "next"]);
}

#[test]
fn a_line_is_capped_at_600_chars() {
    let t = TempDir::new("cap");
    append(&t.log(), &"字".repeat(1000), 1024 * 1024).unwrap();
    let lines = tail(&t.log(), 10);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].chars().count(), 600);
}

#[test]
fn rotation_moves_the_full_file_to_log_1() {
    let t = TempDir::new("rotate-once");
    for i in 0..3 {
        append(&t.log(), &entry(i), 50).unwrap();
    }
    // e0 and e1 filled the file past 50 bytes; e2 went into a new file
    assert_eq!(std::fs::read_to_string(old_of(&t.log())).unwrap(), format!("{}\n{}\n", entry(0), entry(1)));
    assert_eq!(std::fs::read_to_string(t.log()).unwrap(), format!("{}\n", entry(2)));
    assert!(std::fs::metadata(t.log()).unwrap().len() < 50);
}

#[test]
fn second_rotation_replaces_log_1_and_keeps_only_one_old_file() {
    let t = TempDir::new("rotate-twice");
    for i in 0..5 {
        append(&t.log(), &entry(i), 50).unwrap();
    }
    assert_eq!(std::fs::read_to_string(old_of(&t.log())).unwrap(), format!("{}\n{}\n", entry(2), entry(3)));
    assert_eq!(std::fs::read_to_string(t.log()).unwrap(), format!("{}\n", entry(4)));

    let mut names: Vec<String> = std::fs::read_dir(t.log().parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["shiyi.log", "shiyi.log.1"]);
}
