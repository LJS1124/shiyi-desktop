// Integration tests for shiyi_core::install: wrapper scripts, PATH and home helpers.

use shiyi_core::install::*;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

fn path_var(dirs: &[&str]) -> OsString {
    std::env::join_paths(dirs).unwrap()
}

fn only_crlf(s: &str) -> bool {
    let b = s.as_bytes();
    let every_lf_after_cr = b
        .iter()
        .enumerate()
        .all(|(i, &c)| c != b'\n' || (i > 0 && b[i - 1] == b'\r'));
    every_lf_after_cr && !s.replace("\r\n", "").contains('\r')
}

#[test]
fn sh_quote_plain_text() {
    assert_eq!(sh_quote("shiyi"), "'shiyi'");
}

#[test]
fn sh_quote_single_quote() {
    assert_eq!(sh_quote("it's"), "'it'\\''s'");
}

#[test]
fn sh_quote_spaces_and_chinese() {
    assert_eq!(
        sh_quote("/Users/张三/My Apps/拾忆 app/shiyi"),
        "'/Users/张三/My Apps/拾忆 app/shiyi'"
    );
}

#[test]
fn sh_wrapper_shape() {
    let w = sh_wrapper("/opt/shiyi/shiyi");
    assert!(w.starts_with("#!/bin/sh\n"));
    assert!(w.lines().nth(1).unwrap().contains(CLI_MARK));
    assert!(w.contains("exec '/opt/shiyi/shiyi' --cli \"$@\""));
}

#[test]
fn sh_wrapper_windows_path_uses_forward_slashes() {
    let w = sh_wrapper(r"C:\Users\张三\AppData\Local\Shiyi\shiyi.exe");
    assert!(w.contains("exec 'C:/Users/张三/AppData/Local/Shiyi/shiyi.exe' --cli \"$@\""));
    assert!(!w.contains('\\'));
}

#[test]
fn cmd_wrapper_shape() {
    let w = cmd_wrapper(r"C:\Program Files\Shiyi\shiyi.exe");
    assert!(only_crlf(&w));
    assert_eq!(w.lines().next(), Some("@echo off"));
    assert!(w.contains("chcp 65001"));
    assert!(w
        .lines()
        .any(|l| l.starts_with("rem ") && l.contains(CLI_MARK)));
    assert!(w
        .lines()
        .any(|l| l == r#""C:\Program Files\Shiyi\shiyi.exe" --cli %*"#));
    assert!(w.ends_with("exit /b %ERRORLEVEL%\r\n"));
}

#[test]
fn cmd_wrapper_strips_double_quotes_from_exe() {
    let w = cmd_wrapper(r#"C:\a"b\shiyi.exe"#);
    assert!(w.lines().any(|l| l == r#""C:\ab\shiyi.exe" --cli %*"#));
}

#[test]
fn is_our_cli_recognises_both_markers() {
    assert!(is_our_cli(&sh_wrapper("/opt/shiyi/shiyi")));
    assert!(is_our_cli(&cmd_wrapper(r"C:\x\shiyi.exe")));
    assert!(is_our_cli(&format!("#!/bin/sh\n{CLI_MARK_OLD}\n")));
    assert!(!is_our_cli("#!/bin/sh\necho hi\n"));
    assert!(!is_our_cli(""));
}

#[test]
fn wrapper_for_picks_by_extension() {
    let exe = r"C:\x\shiyi.exe";
    assert_eq!(wrapper_for(Path::new("x/shiyi.cmd"), exe), cmd_wrapper(exe));
    assert_eq!(wrapper_for(Path::new("x/SHIYI.CMD"), exe), cmd_wrapper(exe));
    assert_eq!(wrapper_for(Path::new("x/shiyi"), exe), sh_wrapper(exe));
    assert_eq!(wrapper_for(Path::new("x/shiyi.sh"), exe), sh_wrapper(exe));
}

#[test]
fn windows_targets_under_local_bin() {
    let home = Path::new("C:/Users/a");
    let bin = home.join(".local").join("bin");
    let t = windows_targets(home);
    assert_eq!(t.len(), 2);
    assert!(t[0].ends_with("shiyi.cmd"));
    assert_eq!(t[0].parent(), Some(bin.as_path()));
    assert_eq!(t[1].file_name(), Some(OsStr::new("shiyi")));
    assert_eq!(t[1].parent(), Some(bin.as_path()));
}

#[test]
fn windows_path_hint_mentions_dir() {
    let h = windows_path_hint(Path::new(r"C:\Users\a\.local\bin"));
    assert!(h.contains("SetEnvironmentVariable('Path'"));
    assert!(h.contains(r"C:\Users\a\.local\bin"));
}

#[test]
fn windows_path_hint_doubles_single_quote() {
    let h = windows_path_hint(Path::new("/home/o'brien/bin"));
    assert!(h.contains("/home/o''brien/bin"));
    assert!(!h.contains("o'brien"));
}

#[test]
fn cmd_wrapper_escapes_percent() {
    let w = cmd_wrapper(r"C:\Users\50%off\shiyi.exe");
    assert!(w.contains(r#""C:\Users\50%%off\shiyi.exe" --cli %*"#));
}

#[test]
fn sh_wrapper_keeps_backslash_in_unix_paths() {
    assert!(sh_wrapper(r"/odd\name/shiyi").contains(r"'/odd\name/shiyi'"));
}

#[test]
fn windows_path_hint_is_idempotent_script() {
    let h = windows_path_hint(Path::new("C:/Users/a/.local/bin"));
    assert!(h.contains("-notlike '*;C:/Users/a/.local/bin;*'"));
}

#[test]
fn dir_on_path_matching_rules() {
    let p = path_var(&["/bin", "/usr/bin"]);
    assert!(dir_on_path(Path::new("/usr/bin"), &p));
    assert!(!dir_on_path(Path::new("/opt/bin"), &p));

    let p = path_var(&["/a/bin"]);
    // case only matters on case-sensitive systems
    assert_eq!(dir_on_path(Path::new("/A/Bin"), &p), cfg!(windows));
    assert!(dir_on_path(Path::new("/a/bin/"), &p));

    let p = path_var(&["/a/bin/"]);
    assert!(dir_on_path(Path::new("/a/bin"), &p));
}

#[test]
fn home_dir_env_rules() {
    struct Restore(Option<OsString>, Option<OsString>);
    impl Drop for Restore {
        fn drop(&mut self) {
            match &self.0 {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match &self.1 {
                Some(v) => std::env::set_var("USERPROFILE", v),
                None => std::env::remove_var("USERPROFILE"),
            }
        }
    }
    let _restore = Restore(std::env::var_os("HOME"), std::env::var_os("USERPROFILE"));

    std::env::set_var("HOME", "/h/one");
    std::env::set_var("USERPROFILE", "/u/two");
    assert_eq!(home_dir(), Some(PathBuf::from("/h/one")));

    std::env::set_var("HOME", "");
    assert_eq!(home_dir(), Some(PathBuf::from("/u/two")));

    std::env::remove_var("HOME");
    std::env::remove_var("USERPROFILE");
    assert_eq!(home_dir(), None);
}
