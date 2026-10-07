//! Application-owned files stay under the repository (or a portable executable's directory).
use std::{
    io,
    path::{Component, Path, PathBuf},
};

fn validate_root(path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(io::Error::other(
            "TokenPulse storage requires an absolute local directory",
        ));
    }
    #[cfg(windows)]
    {
        use std::path::Prefix;
        let drive = match path.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive.to_ascii_uppercase(),
                _ => {
                    return Err(io::Error::other(
                        "TokenPulse storage requires a local drive",
                    ));
                }
            },
            _ => {
                return Err(io::Error::other(
                    "TokenPulse storage requires a local drive",
                ));
            }
        };
        let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        if drive == b'C'
            || system_drive
                .as_bytes()
                .first()
                .is_some_and(|system| system.to_ascii_uppercase() == drive)
        {
            return Err(io::Error::other(
                "TokenPulse refuses to store data on the system drive; move the project to a data drive",
            ));
        }
    }
    Ok(())
}

fn root_for_executable(executable: &Path) -> io::Result<PathBuf> {
    let directory = executable
        .parent()
        .ok_or_else(|| io::Error::other("Executable directory unavailable"))?;
    let root = directory
        .ancestors()
        .find(|candidate| {
            candidate.join("package.json").is_file()
                && candidate.join("src-tauri/Cargo.toml").is_file()
        })
        .unwrap_or(directory);
    validate_root(root)?;
    Ok(root.to_path_buf())
}

pub fn project_root() -> io::Result<PathBuf> {
    let root = match std::env::var_os("TOKENPULSE_PROJECT_ROOT") {
        Some(root) => PathBuf::from(root),
        None => root_for_executable(&std::env::current_exe()?)?,
    };
    validate_root(&root)?;
    let root = root.canonicalize()?;
    validate_root(&root)?;
    // Windows canonicalize adds \\?\ to local drives. Store/source safety checks distinguish
    // ordinary local paths from network/device paths, so retain the canonical location
    // while spelling a local drive normally (without converting Unicode path components).
    #[cfg(windows)]
    if let Some(Component::Prefix(prefix)) = root.components().next() {
        if let std::path::Prefix::VerbatimDisk(drive) = prefix.kind() {
            let mut local = PathBuf::from(format!("{}:\\", char::from(drive)));
            for component in root.components().skip(2) {
                local.push(component.as_os_str());
            }
            return Ok(local);
        }
    }
    Ok(root)
}

fn validate_location(path: &Path) -> io::Result<()> {
    let existing = path
        .ancestors()
        .find(|candidate| candidate.exists())
        .ok_or_else(|| io::Error::other("Storage parent unavailable"))?;
    validate_root(&existing.canonicalize()?)
}

pub fn local_root() -> io::Result<PathBuf> {
    let root = project_root()?.join(".local");
    validate_location(&root)?;
    Ok(root)
}

pub fn data_directory() -> io::Result<PathBuf> {
    Ok(local_root()?.join("data").join(if cfg!(debug_assertions) {
        "dev"
    } else {
        "release"
    }))
}

pub fn temporary_directory() -> io::Result<PathBuf> {
    let directory = local_root()?.join("tmp");
    validate_location(&directory)?;
    std::fs::create_dir_all(&directory)?;
    Ok(directory)
}

/// Set before desktop/headless startup so ordinary shortcut launches and their children
/// use the same temporary directory as launches through the development scripts.
#[cfg(windows)]
pub fn initialize_process_environment() -> io::Result<()> {
    let directory = temporary_directory()?;
    for name in ["TEMP", "TMP", "TMPDIR"] {
        // Windows environment mutation is thread-safe. main calls this before starting
        // Tauri, Tokio or any application worker, including WebView child processes.
        unsafe { std::env::set_var(name, &directory) };
    }
    Ok(())
}

pub(super) fn configure(context: &mut tauri::Context<tauri::Wry>) -> io::Result<()> {
    let directory = data_directory()?;
    validate_location(&directory)?;
    std::fs::create_dir_all(&directory)?;
    context.config_mut().app.app_directories_override = Some(
        tauri::utils::config::AppDirectoriesOverride::Root(directory.clone()),
    );
    // Explicit window overrides otherwise bypass Tauri's application directory override.
    for window in &mut context.config_mut().app.windows {
        window.data_directory = Some(directory.clone());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_and_parent_traversal_roots_are_rejected() {
        assert!(validate_root(Path::new(".local")).is_err());
        assert!(validate_root(&std::env::current_dir().unwrap().join("../outside")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn system_drive_and_network_roots_are_rejected() {
        for path in [
            r"C:\data",
            r"c:\data",
            r"\\?\C:\data",
            r"\\server\share\data",
            r"\\.\C:\data",
        ] {
            assert!(validate_root(Path::new(path)).is_err(), "accepted {path}");
        }
        assert!(validate_root(Path::new(r"E:\Documents\Code\token-pulse")).is_ok());
    }

    #[test]
    fn build_and_installed_executables_resolve_the_same_repository() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for executable in [
            "target/debug/token-pulse-desktop.exe",
            "target/release/token-pulse-desktop.exe",
            ".local/app/token-pulse-desktop.exe",
        ] {
            assert_eq!(root_for_executable(&root.join(executable)).unwrap(), root);
        }
    }

    #[cfg(windows)]
    #[test]
    fn resolved_data_directory_is_accepted_by_the_store() {
        let directory = data_directory().unwrap();
        assert!(!directory.to_string_lossy().starts_with("\\\\"));
        let fixture = tempfile::tempdir_in(temporary_directory().unwrap()).unwrap();
        let database = token_pulse_store::Database::open(fixture.path()).unwrap();
        assert!(database.path().is_file());
    }
}
