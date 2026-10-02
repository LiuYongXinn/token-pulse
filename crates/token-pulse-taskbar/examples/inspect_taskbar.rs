fn main() {
    #[cfg(windows)]
    {
        if std::env::args().skip(1).collect::<Vec<_>>() != ["--inspect-taskbar"] {
            std::process::exit(2);
        }
        match token_pulse_taskbar::windows::topology::inspect_primary_taskbar() {
            Ok(topology) => {
                println!("READ_ONLY_TASKBAR_TOPOLOGY {topology:?}");
                let width = (360 * topology.dpi / 96) as i32;
                let minimum = (320 * topology.dpi / 96) as i32;
                println!("PLAN_ONLY {:?}", topology.plan(width, minimum));
            }
            Err(error) => {
                println!("TASKBAR_PROBE_UNAVAILABLE {error:?}");
                std::process::exit(1);
            }
        }
    }
    #[cfg(not(windows))]
    std::process::exit(2);
}
