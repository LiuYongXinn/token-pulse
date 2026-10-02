//! Metadata-only native program discovery. Never runs shims, reads auth or guesses login.
use crate::NativeService;
use std::path::{Path, PathBuf};
use token_pulse_core::{error::ErrorCode, quota::AccountServiceTarget};

#[cfg(all(windows, target_arch = "x86_64"))]
const PACKAGE: &str = "codex-win32-x64";
#[cfg(all(windows, target_arch = "x86_64"))]
const TRIPLE: &str = "x86_64-pc-windows-msvc";
#[cfg(all(windows, target_arch = "aarch64"))]
const PACKAGE: &str = "codex-win32-arm64";
#[cfg(all(windows, target_arch = "aarch64"))]
const TRIPLE: &str = "aarch64-pc-windows-msvc";

fn local(path: &Path) -> Result<(), ErrorCode> {
    if !path.is_absolute() {
        return Err(ErrorCode::InvalidQuery);
    }
    #[cfg(windows)]
    token_pulse_core::sources::validate_root(
        path,
        token_pulse_core::sources::SourceOrigin::Custom,
    )?;
    Ok(())
}
fn discover(
    paths: impl IntoIterator<Item = PathBuf>,
    home: Option<&Path>,
) -> Result<AccountServiceTarget, ErrorCode> {
    if let Some(home) = home {
        local(home)?;
        if !home.is_dir() {
            return Err(ErrorCode::QuotaServiceUnavailable);
        }
    }
    for directory in paths.into_iter().take(256) {
        // Empty or relative PATH entries would search cwd; remote/device roots are never probed.
        if local(&directory).is_err() {
            continue;
        }
        #[cfg(windows)]
        let mut candidates = vec![directory.join("codex.exe")];
        #[cfg(not(windows))]
        let mut candidates = vec![directory.join("codex")];
        #[cfg(all(windows, any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            let package = directory.join("node_modules/@openai/codex");
            candidates.push(package.join(format!(
                "node_modules/@openai/{PACKAGE}/vendor/{TRIPLE}/codex/codex.exe"
            )));
            candidates.push(package.join(format!("vendor/{TRIPLE}/codex/codex.exe")));
        }
        for candidate in candidates.drain(..) {
            if !candidate.is_file() {
                continue;
            }
            // Canonical path, native extension, Home and SHA use the same selection validator.
            if let Ok(target) = NativeService::inspect(&candidate, home) {
                return Ok(target);
            }
        }
    }
    Err(ErrorCode::QuotaServiceUnavailable)
}
/// Existing explicit Home is preserved, including a deliberately selected service-default None.
/// With no configured target, CODEX_HOME takes precedence over the existing user .codex directory.
pub fn detect_local_service(
    existing: Option<&AccountServiceTarget>,
) -> Result<AccountServiceTarget, ErrorCode> {
    let home = if let Some(existing) = existing {
        existing.home_path.as_ref().map(PathBuf::from)
    } else if let Some(value) = std::env::var_os("CODEX_HOME") {
        Some(PathBuf::from(value))
    } else {
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(|profile| PathBuf::from(profile).join(".codex"))
    };
    let paths: Vec<_> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    #[cfg(windows)]
    let paths = {
        let mut paths = paths;
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let npm = PathBuf::from(appdata).join("npm");
            if !paths.contains(&npm) {
                paths.push(npm);
            }
        }
        paths
    };
    discover(paths, home.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn native_name() -> &'static str {
        if cfg!(windows) { "codex.exe" } else { "codex" }
    }
    #[test]
    fn discovery_preserves_path_order_home_and_does_not_execute_or_read_credentials() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let home = temp.path().join("home");
        for path in [&first, &second, &home] {
            std::fs::create_dir(path).unwrap();
        }
        // Deliberately not executable bytes: discovery may inspect, but must never launch.
        std::fs::write(first.join(native_name()), b"abc").unwrap();
        std::fs::write(second.join(native_name()), b"later").unwrap();
        std::fs::write(
            home.join("auth.json"),
            b"deliberately invalid and never parsed",
        )
        .unwrap();
        let found = discover(
            [PathBuf::from("."), first.clone(), second.clone()],
            Some(&home),
        )
        .unwrap();
        assert_eq!(
            found.executable_path,
            first
                .join(native_name())
                .canonicalize()
                .unwrap()
                .to_str()
                .unwrap()
        );
        assert_eq!(
            found.home_path.as_deref(),
            home.canonicalize().unwrap().to_str()
        );
        assert_eq!(
            found.executable_sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(discover([first.clone()], None).unwrap().home_path.is_none());
        assert_eq!(
            discover([first], Some(&temp.path().join("missing"))),
            Err(ErrorCode::QuotaServiceUnavailable)
        );
    }
    #[cfg(all(windows, any(target_arch = "x86_64", target_arch = "aarch64")))]
    #[test]
    fn npm_shims_resolve_native_architecture_without_executing_shells() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(root.join("codex.cmd"), b"must never execute").unwrap();
        std::fs::write(root.join("codex.ps1"), b"must never execute").unwrap();
        assert_eq!(
            discover([root.into()], None),
            Err(ErrorCode::QuotaServiceUnavailable)
        );
        let exe=root.join(format!("node_modules/@openai/codex/node_modules/@openai/{PACKAGE}/vendor/{TRIPLE}/codex/codex.exe"));
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"abc").unwrap();
        assert_eq!(
            discover([PathBuf::from(r"\\server\share"), root.into()], None)
                .unwrap()
                .executable_path,
            exe.canonicalize().unwrap().to_str().unwrap()
        );
        assert_eq!(
            discover([root.into()], Some(Path::new(r"\\server\share"))),
            Err(ErrorCode::InvalidQuery)
        );
    }
}
