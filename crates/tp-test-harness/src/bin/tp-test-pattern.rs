//! Opens the synthetic test-pattern window for manual QA (CAP-02A): a non-sensitive,
//! animated target to choose in the Windows capture picker. Closes when its window is
//! closed or after `--seconds N` (default 3600).
//!
//!   cargo run -p tp-test-harness --bin tp-test-pattern -- [--x X --y Y --width W --height H] [--seconds N] [--topmost]

#[cfg(windows)]
fn main() {
    use std::time::Duration;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |name: &str, default: i32| -> i32 {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let (x, y) = (value("--x", 160), value("--y", 160));
    let (width, height) = (value("--width", 960), value("--height", 600));
    let seconds = u64::try_from(value("--seconds", 3600)).unwrap_or(3600);

    let window = tp_capture::wgc::testing::TestPatternWindow::open(
        "TP Remote Test Pattern",
        x,
        y,
        width,
        height,
    )
    .unwrap_or_else(|error| {
        eprintln!("tp-test-pattern: {error}");
        std::process::exit(1);
    });
    // WGC delivered no frames for this GDI window while another window covered it
    // (CAP-02A finding), so QA keeps it above other windows.
    if args.iter().any(|a| a == "--topmost") {
        let _ = window.set_topmost(true);
    }
    println!(
        "tp-test-pattern pid={} hwnd={:#x}",
        std::process::id(),
        window.hwnd()
    );
    // Exit when the window is closed by the user, or after the time limit.
    let closed = tp_test_harness::capture::wait_until(Duration::from_secs(seconds), || {
        !tp_capture::wgc::testing::window_exists(window.hwnd())
    });
    println!("tp-test-pattern exit closed_by_user={closed}");
    window.close();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("tp-test-pattern is Windows-only");
    std::process::exit(2);
}
