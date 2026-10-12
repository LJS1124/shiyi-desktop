fn main() {
    let code = shiyi_core::cli::main_with_args(std::env::args().skip(1).collect());
    std::process::exit(code);
}
