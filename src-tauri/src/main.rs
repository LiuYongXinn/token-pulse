#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod notify_headless;

fn main() {
    #[cfg(windows)]
    if let Some(status) = token_pulse_app::release_verifier::run_if_requested() {
        std::process::exit(status);
    }
    if let Some(status) = notify_headless::run_if_requested() {
        std::process::exit(status);
    }
    token_pulse_app::run();
}
