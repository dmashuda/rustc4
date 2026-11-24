fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `bazel run` starts in the runfiles tree; paths are meant from where
    // the user typed the command.
    if let Ok(dir) = std::env::var("BUILD_WORKING_DIRECTORY") {
        let _ = std::env::set_current_dir(dir);
    }
    std::process::exit(rustc4_cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr()));
}
