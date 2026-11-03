//! OIA desktop entry point. `--status` works without opening a window.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--status") => println!("{}", oia_app::status_banner()),
        Some("--help" | "-h") => println!(
            "oia: local research workbench\nUsage: oia [--status|--help]\nNo arguments opens the desktop app (default desktop feature)."
        ),
        Some(_) => {
            eprintln!("Unknown argument. Use oia --help.");
            std::process::exit(1);
        }
        None => {
            #[cfg(feature = "desktop")]
            if let Err(error) = oia_app::desktop::run() {
                eprintln!("OIA could not start: {error}");
                std::process::exit(1);
            }
            #[cfg(not(feature = "desktop"))]
            {
                eprintln!(
                    "Desktop support is disabled. Build with the desktop feature or use --status."
                );
                std::process::exit(1);
            }
        }
    }
}
