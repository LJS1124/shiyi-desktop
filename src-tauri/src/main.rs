// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// A GUI program on Windows has no console, so `shiyi --cli …` would print
/// into the void when started from cmd or PowerShell. Join the parent's
/// console, and for any of stdin/stdout/stderr that wasn't redirected to a
/// pipe or file, point it at that console. (When an agent runs the command with
/// pipes, those handles are already valid and stay as they are.)
#[cfg(windows)]
fn attach_parent_console() {
    use std::ffi::c_void;
    use std::os::windows::io::IntoRawHandle;
    type Handle = *mut c_void;
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn GetStdHandle(which: u32) -> Handle;
        fn SetStdHandle(which: u32, handle: Handle) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const STD_INPUT: u32 = -10i32 as u32;
    const STD_OUTPUT: u32 = -11i32 as u32;
    const STD_ERROR: u32 = -12i32 as u32;
    let invalid = usize::MAX as Handle;
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
        for (which, name, write) in [(STD_INPUT, "CONIN$", false), (STD_OUTPUT, "CONOUT$", true), (STD_ERROR, "CONOUT$", true)] {
            let h = GetStdHandle(which);
            if h.is_null() || h == invalid {
                let file = std::fs::OpenOptions::new().read(!write).write(write).open(name);
                if let Ok(f) = file {
                    SetStdHandle(which, f.into_raw_handle() as Handle);
                }
            }
        }
    }
}

fn main() {
    // `<app>/shiyi --cli <command>` is the command line tool AI agents use;
    // the `shiyi` wrapper installed from Settings calls this.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--cli") {
        #[cfg(windows)]
        attach_parent_console();
        std::process::exit(shiyi_core::cli::main_with_args(args[2..].to_vec()));
    }
    shiyi_lib::run()
}
