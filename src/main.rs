mod geometry;
mod input;
mod interaction;
mod keyboard;
mod layout;
mod platform;
mod runtime;
mod settings;
mod shell;
use std::{
    env,
    path::PathBuf,
    process::{Command, ExitCode},
    thread,
    time::{Duration, Instant},
};
fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let has = |s: &str| args.iter().any(|v| v == s);
    // A packaged copy (scripts/package.ps1) has its assets beside the
    // executable. A development build uses the native build folder.
    let beside = env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.to_path_buf()))
        .filter(|folder| folder.join("assets").is_dir());
    let directory =
        beside.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("build/Release"));
    // --diagnostic-input is the diagnostic with real OS input; see docs/technical.md.
    let input_test = has("--diagnostic-input");
    let diagnostic = has("--diagnostic") || input_test;
    if has("--worker") || has("--smoke") || diagnostic {
        if let Err(e) = env::set_current_dir(directory) {
            eprintln!("Run scripts/build.ps1 first: {e}");
            return ExitCode::FAILURE;
        }
        return match runtime::run(has("--smoke"), diagnostic, input_test) {
            Ok(code) => ExitCode::from(code as u8),
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    let mut failures = 0;
    let mut waiting = false;
    loop {
        let start = Instant::now();
        let result = Command::new(env::current_exe().unwrap())
            .arg("--worker")
            .args(&args)
            .current_dir(&directory)
            .status();
        match result {
            Ok(s) if s.success() => return ExitCode::SUCCESS,
            Ok(s) if s.code() == Some(runtime::EXIT_WAIT_FOR_RUNTIME) => {
                // SteamVR absent or quit. The worker never launches SteamVR.
                if !waiting || start.elapsed() > Duration::from_secs(5) {
                    println!("Waiting for SteamVR to start...");
                }
                waiting = true;
                thread::sleep(Duration::from_secs(2));
            }
            Ok(s) => {
                if start.elapsed() > Duration::from_secs(60) {
                    failures = 0;
                }
                failures += 1;
                eprintln!("Rust worker exited {s}; failure {failures}/3");
                if failures >= 3 {
                    return ExitCode::FAILURE;
                }
                thread::sleep(Duration::from_secs(2 * failures));
            }
            Err(e) => {
                eprintln!("Cannot launch Rust worker: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
}
