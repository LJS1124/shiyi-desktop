// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `拾忆.app/Contents/MacOS/shiyi --cli <command>` is the command line tool
    // AI agents use; the `shiyi` wrapper installed from Settings calls this.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--cli") {
        std::process::exit(shiyi_core::cli::main_with_args(args[2..].to_vec()));
    }
    shiyi_lib::run()
}
