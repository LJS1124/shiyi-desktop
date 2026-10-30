use serde_json::{json, Value};
use shiyi_core::Core;
use std::path::{Path, PathBuf};

// 2026-10-12 10:00 +08:00
const NOW: i64 = 1_791_770_400_000;

/// A unique temp dir that is removed when the guard drops (also on panic).
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("shiyi-backups-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn db(&self) -> PathBuf {
        self.0.join("shiyi.db")
    }
    fn backups(&self) -> PathBuf {
        self.0.join("backups")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn open(path: &Path, now: i64) -> Core {
    let mut c = Core::open(path).unwrap();
    c.set_fixed_clock(now, 8 * 3600);
    c
}

fn add_idea(c: &mut Core, text: &str) -> String {
    let r = c.apply(json!({"ops": [{"op": "idea.create", "text": text}]}), "agent:test").unwrap();
    r["results"][0]["id"].as_str().unwrap().to_string()
}

/// Number of live ideas, from the status query.
fn idea_count(c: &Core) -> i64 {
    let s = c.query(json!({"q": "status"})).unwrap();
    s["counts"]["ideas"].as_i64().unwrap()
}

fn file_name(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

/// File names in `dir` that start with `prefix`, sorted.
fn names_with_prefix(dir: &Path, prefix: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefix))
        .collect();
    v.sort();
    v
}

/// The `list_backups` entry for `file`.
fn entry<'a>(list: &'a [Value], file: &str) -> &'a Value {
    list.iter().find(|e| e["file"] == file).unwrap_or_else(|| panic!("{file} is not listed: {list:?}"))
}

#[test]
fn fresh_database_without_backups_folder_lists_nothing() {
    let t = TempDir::new("empty");
    let c = open(&t.db(), NOW);
    assert!(!t.backups().exists());
    assert!(c.list_backups().unwrap().is_empty());
}

#[test]
fn daily_backup_is_listed_with_its_counts() {
    let t = TempDir::new("daily-list");
    let mut c = open(&t.db(), NOW);
    add_idea(&mut c, "一");
    add_idea(&mut c, "二");
    let p = c.backup_daily(14).unwrap().expect("a daily backup");
    assert_eq!(file_name(&p), "shiyi-2026-10-12.db");

    let list = c.list_backups().unwrap();
    assert_eq!(list.len(), 1, "{list:?}");
    let b = &list[0];
    let name = b["file"].as_str().unwrap();
    assert!(name.starts_with("shiyi-") && name.ends_with(".db"), "got {name}");
    assert_eq!(b["kind"], "daily");
    assert!(b["bytes"].as_u64().unwrap() > 0, "{b}");
    assert_eq!(b["counts"]["ideas"], 2, "{b}");
}

#[test]
fn restore_rolls_back_and_saves_the_current_data_first() {
    let t = TempDir::new("restore");
    let mut c = open(&t.db(), NOW);
    add_idea(&mut c, "一");
    add_idea(&mut c, "二");
    let daily = file_name(&c.backup_daily(14).unwrap().expect("a daily backup"));
    for text in ["三", "四", "五"] {
        add_idea(&mut c, text);
    }
    assert_eq!(idea_count(&c), 5);

    let r = c.restore_backup(&daily).unwrap();
    assert_eq!(r["restored"], daily.as_str());
    let saved = r["saved_current"].as_str().unwrap().to_string();
    assert!(saved.starts_with("before-restore-"), "got {saved}");
    assert_eq!(idea_count(&c), 2);

    let list = c.list_backups().unwrap();
    let saved_entry = entry(&list, &saved);
    assert_eq!(saved_entry["kind"], "restore");
    assert_eq!(saved_entry["counts"]["ideas"], 5, "{saved_entry}");

    // restoring the saved copy brings the five ideas back
    c.restore_backup(&saved).unwrap();
    assert_eq!(idea_count(&c), 5);
}

#[test]
fn writes_work_after_a_restore_and_the_version_is_recorded() {
    let t = TempDir::new("after-restore");
    let mut c = open(&t.db(), NOW);
    add_idea(&mut c, "一");
    let daily = file_name(&c.backup_daily(14).unwrap().expect("a daily backup"));
    add_idea(&mut c, "二");
    c.restore_backup(&daily).unwrap();

    let id = add_idea(&mut c, "恢复之后的新想法");
    assert_eq!(idea_count(&c), 2);
    let listed = c.query(json!({"q": "list", "type": "idea"})).unwrap();
    assert_eq!(listed["total"], 2, "{listed}");
    assert!(listed["items"].as_array().unwrap().iter().any(|i| i["id"] == id.as_str()), "{listed}");

    assert_eq!(c.meta_get("app_version").unwrap().as_deref(), Some(shiyi_core::VERSION));
}

#[test]
fn bad_or_missing_names_are_rejected_and_change_nothing() {
    let t = TempDir::new("names");
    let mut c = open(&t.db(), NOW);
    add_idea(&mut c, "一");
    c.backup_daily(14).unwrap();

    for name in ["../shiyi.db", "x/shiyi-2026-10-12.db", "notes.txt", "random.db"] {
        let e = c.restore_backup(name).unwrap_err();
        assert_eq!(e.code, "invalid_value", "{name}: {e}");
    }
    let e = c.restore_backup("shiyi-1999-01-01.db").unwrap_err();
    assert_eq!(e.code, "not_found", "{e}");

    assert_eq!(idea_count(&c), 1);
    assert!(names_with_prefix(&t.backups(), "before-restore-").is_empty());
}

#[test]
fn corrupt_backup_is_listed_without_counts_and_cannot_be_restored() {
    let t = TempDir::new("corrupt");
    let mut c = open(&t.db(), NOW);
    add_idea(&mut c, "一");
    add_idea(&mut c, "二");
    std::fs::create_dir_all(t.backups()).unwrap();
    std::fs::write(t.backups().join("shiyi-2026-10-01.db"), b"not a database").unwrap();

    let list = c.list_backups().unwrap();
    let e = entry(&list, "shiyi-2026-10-01.db");
    assert!(e["counts"].is_null(), "{e}");

    assert!(c.restore_backup("shiyi-2026-10-01.db").is_err());
    assert_eq!(idea_count(&c), 2);
    assert!(names_with_prefix(&t.backups(), "before-restore-").is_empty());
}

#[test]
fn only_the_newest_five_before_restore_copies_are_kept() {
    let t = TempDir::new("retention");
    let mut c = open(&t.db(), NOW);
    add_idea(&mut c, "一");
    let daily = file_name(&c.backup_daily(14).unwrap().expect("a daily backup"));

    // one minute apart, so every saved copy gets its own name
    let mut saved = vec![];
    for i in 0..7i64 {
        c.set_fixed_clock(NOW + i * 60_000, 8 * 3600);
        let r = c.restore_backup(&daily).unwrap();
        saved.push(r["saved_current"].as_str().unwrap().to_string());
    }

    let kept = names_with_prefix(&t.backups(), "before-restore-");
    assert_eq!(kept.len(), 5, "got {kept:?}");
    assert_eq!(kept, saved[2..].to_vec(), "the five newest are the ones kept");
}
