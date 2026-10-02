#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    #[cfg(windows)]
    {
        let Ok(startup) = token_pulse_taskbar::windows::Startup::parse(std::env::args().skip(1))
        else {
            std::process::exit(2);
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("native host runtime");
        if runtime
            .block_on(token_pulse_taskbar::windows::run_host(startup))
            .is_err()
        {
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    std::process::exit(2);
}
