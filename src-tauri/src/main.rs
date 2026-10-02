#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod notify_headless;

fn main() {
    if let Some(status) = notify_headless::run_if_requested() {
        std::process::exit(status);
    }
    token_pulse_app::run();
}
