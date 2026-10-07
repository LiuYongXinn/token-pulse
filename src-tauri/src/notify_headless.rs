use token_pulse_integration::notify_invocation::parse_invocation;

pub fn run_if_requested() -> Option<i32> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let invocation = match parse_invocation(arguments.iter().cloned()) {
        Ok(Some(invocation)) => invocation,
        Ok(None) => return None,
        Err(error) => {
            eprintln!("{error}");
            return Some(2);
        }
    };
    if invocation.hint().is_none() {
        return Some(0);
    }
    #[cfg(windows)]
    {
        use token_pulse_integration::{
            notify_invocation::windows::dispatch,
            notify_registry::{RegistryError, windows::NotifyRegistry},
        };
        let result = (|| {
            let directory =
                token_pulse_app::local_paths::data_directory().map_err(|_| RegistryError::Io)?;
            // Explicit debug-native acceptance only; a single validated child name cannot redirect
            // product notifications to another Home or external directory.
            #[cfg(debug_assertions)]
            let directory = if let Some(name) = std::env::var_os("TOKENPULSE_NATIVE_NOTIFY_PROBE") {
                let name = name.to_str().ok_or(RegistryError::UnsafePath)?;
                let id = name
                    .strip_prefix("native-notify-")
                    .ok_or(RegistryError::UnsafePath)?;
                if id.len() != 32
                    || !id
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    return Err(RegistryError::UnsafePath);
                }
                directory.join(name)
            } else {
                directory
            };
            let registry = NotifyRegistry::open(&directory)?;
            let executable = std::env::current_exe().map_err(|_| RegistryError::Io)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| RegistryError::Io)?;
            runtime.block_on(dispatch(&registry, &executable, &invocation, &arguments[3]))
        })();
        match result {
            Ok(outcome) => {
                let mut failed = false;
                if let Err(error) = outcome.wake {
                    eprintln!("{error}");
                    failed = true;
                }
                if let Some(Err(error)) = outcome.original {
                    eprintln!("{error}");
                    failed = true;
                }
                Some(if failed { 1 } else { 0 })
            }
            Err(error) => {
                eprintln!("{error}");
                Some(1)
            }
        }
    }
    #[cfg(not(windows))]
    {
        Some(1)
    }
}
