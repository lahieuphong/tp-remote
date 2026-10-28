//! `tp-bindings generate <dir>` writes the IPC TypeScript bindings.
//! `tp-bindings check <dir>` exits non-zero if `<dir>` differs from what would be generated.

use std::path::Path;
use std::process::ExitCode;

use tp_test_harness::bindings;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command, dir] if command == "generate" => match bindings::write(Path::new(dir)) {
            Ok(count) => {
                println!("tp-bindings: wrote {count} files to {dir}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("tp-bindings: generation failed: {error}");
                ExitCode::FAILURE
            }
        },
        [command, dir] if command == "check" => match bindings::check(Path::new(dir)) {
            Ok(()) => {
                println!("tp-bindings: {dir} is up to date");
                ExitCode::SUCCESS
            }
            Err(drift) => {
                eprintln!("tp-bindings: {dir} has drifted from the Rust contracts:");
                for line in drift {
                    eprintln!("  - {line}");
                }
                eprintln!("Run `npm run bindings:generate` and review the diff.");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: tp-bindings <generate|check> <dir>");
            ExitCode::from(2)
        }
    }
}
