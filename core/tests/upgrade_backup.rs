use rusqlite::{Connection, OpenFlags};
use serde_json::json;
use shiyi_core::Core;
use std::path::{Path, PathBuf};

// 2026-10-12 10:00 +08:00
const NOW: i64 = 1_791_770_400_000;
const DAY: i64 = 86_400_000;

/// A unique temp dir that is removed when the guard drops (also on panic).
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("shiyi-upgrade-test-{}-{}", std::process::id(), name));
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

/// Upgrade backups in `dir`, by file name.
fn upgrade_files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("upgrade-") && n.ends_with(".db"))
        .collect();
    v.sort();
    v
}

fn daily_files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("shiyi-") && n.ends_with(".db"))
        .collect();
    v.sort();
    v
}

fn file_name(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
fn fresh_empty_database_is_not_backed_up() {
    let t = TempDir::new("fresh");
    let c = open(&t.db(), NOW);

    assert_eq!(c.backup_on_upgrade("1.1.1").unwrap(), None);
    assert!(!t.backups().exists(), "no backups dir for a fresh install");

    // same version again: still nothing
    assert_eq!(c.backup_on_upgrade("1.1.1").unwrap(), None);
    assert!(!t.backups().exists());
}

#[test]
fn database_without_recorded_version_is_backed_up_as_earlier() {
    let t = TempDir::new("earlier");
    let idea_id;
    {
        let mut c = open(&t.db(), NOW);
        idea_id = add_idea(&mut c, "升级前的想法");
        let p = c.backup_on_upgrade("1.1.1").unwrap().expect("an upgrade backup");
        let name = file_name(&p);
        assert!(name.starts_with("upgrade-earlier-to-1.1.1-"), "got {name}");
        assert!(name.ends_with(".db"), "got {name}");
        assert_eq!(p.parent().unwrap(), t.backups());
        assert!(p.exists());

        // the copy is a valid SQLite database holding the idea
        let copy = Connection::open_with_flags(&p, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let doc: String = copy
            .query_row("SELECT doc FROM entities WHERE id=?1", [&idea_id], |r| r.get(0))
            .unwrap();
        assert!(doc.contains("升级前的想法"), "doc was {doc}");
    }
}

#[test]
fn same_version_again_makes_no_second_copy() {
    let t = TempDir::new("same");
    let mut c = open(&t.db(), NOW);
    add_idea(&mut c, "一条");
    assert!(c.backup_on_upgrade("1.1.1").unwrap().is_some());
    assert_eq!(c.backup_on_upgrade("1.1.1").unwrap(), None);
    assert_eq!(upgrade_files(&t.backups()).len(), 1);
}

#[test]
fn newer_version_is_backed_up_from_the_recorded_one() {
    let t = TempDir::new("newer");
    let mut c = open(&t.db(), NOW);
    add_idea(&mut c, "一条");
    c.backup_on_upgrade("1.1.1").unwrap();
    let p = c.backup_on_upgrade("1.2.0").unwrap().expect("an upgrade backup");
    let name = file_name(&p);
    assert!(name.starts_with("upgrade-1.1.1-to-1.2.0-"), "got {name}");
    assert!(name.ends_with(".db"), "got {name}");
    assert_eq!(upgrade_files(&t.backups()).len(), 2);
}

#[test]
fn upgrade_backups_keep_only_the_newest_five() {
    let t = TempDir::new("retention");
    let mut c = open(&t.db(), NOW);
    add_idea(&mut c, "一条");
    let versions = ["1.1.2", "1.1.3", "1.1.4", "1.1.5", "1.1.6", "1.1.7", "1.1.8"];
    let mut last = None;
    for v in versions {
        last = c.backup_on_upgrade(v).unwrap();
    }
    let files = upgrade_files(&t.backups());
    assert_eq!(files.len(), 5, "got {files:?}");
    let newest = file_name(&last.unwrap());
    assert!(newest.starts_with("upgrade-1.1.7-to-1.1.8-"), "got {newest}");
    assert!(files.contains(&newest), "newest copy is kept: {files:?}");
    assert!(files.iter().any(|f| f.contains("-to-1.1.8-")), "got {files:?}");
    assert!(!files.iter().any(|f| f.contains("-to-1.1.2-")), "oldest is removed: {files:?}");
}

#[test]
fn daily_backups_do_not_touch_upgrade_files_and_do_not_count_them() {
    let t = TempDir::new("daily");
    let mut c = open(&t.db(), NOW);
    add_idea(&mut c, "一条");
    for v in ["1.1.2", "1.1.3", "1.1.4"] {
        c.backup_on_upgrade(v).unwrap();
    }
    let upgrades = upgrade_files(&t.backups());
    assert_eq!(upgrades.len(), 3);

    // two daily copies on two different days, keep 2: both stay
    c.backup_daily(14).unwrap();
    c.set_fixed_clock(NOW + DAY, 8 * 3600);
    c.backup_daily(14).unwrap();
    assert_eq!(daily_files(&t.backups()).len(), 2);
    assert_eq!(upgrade_files(&t.backups()), upgrades);

    // keep 1: the older daily copy goes, the upgrade copies all stay
    c.backup_daily(1).unwrap();
    assert_eq!(daily_files(&t.backups()).len(), 1);
    assert_eq!(upgrade_files(&t.backups()), upgrades);
}
