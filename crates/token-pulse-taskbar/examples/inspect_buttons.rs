fn main() {
    #[cfg(windows)]
    {
        if std::env::args().skip(1).collect::<Vec<_>>() != ["--inspect-buttons"] {
            std::process::exit(2);
        }
        let result = token_pulse_taskbar::windows::buttons::ButtonProbe::start()
            .and_then(|probe| probe.inspect());
        match result {
            Ok(coverage) => println!(
                "READ_ONLY_BUTTON_GEOMETRY {coverage:?}; rightmost={}",
                coverage.rightmost()
            ),
            Err(error) => {
                println!("BUTTON_GEOMETRY_UNAVAILABLE {error:?}");
                std::process::exit(1);
            }
        }
    }
    #[cfg(not(windows))]
    std::process::exit(2);
}
