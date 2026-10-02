use token_pulse_integration::notify_invocation::parse_invocation;

pub fn run_if_requested() -> Option<i32> {
    let invocation = match parse_invocation(std::env::args_os().skip(1)) {
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
            notify_invocation::windows::wake_only,
            notify_registry::{RegistryError, windows::NotifyRegistry},
        };
        let result = (|| {
            let app_id = if cfg!(debug_assertions) {
                "com.tokenpulse.desktop.dev"
            } else {
                "com.tokenpulse.desktop"
            };
            let directory = dirs::data_local_dir()
                .ok_or(RegistryError::Io)?
                .join(app_id);
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
            // The original-command runner is not available yet. Never silently drop that choice
            // or execute an unreviewed replacement; the formal enable UI remains unavailable.
            if registry.get(invocation.registration_id())?.chain_original() {
                eprintln!("notify_original_chain_unavailable");
                return Err(RegistryError::InvalidRecord);
            }
            let executable = std::env::current_exe().map_err(|_| RegistryError::Io)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| RegistryError::Io)?;
            runtime.block_on(wake_only(&registry, &executable, &invocation))
        })();
        match result {
            Ok(_) => Some(0),
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
