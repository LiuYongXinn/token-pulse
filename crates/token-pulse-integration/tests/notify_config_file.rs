#![cfg(windows)]
use std::{fs, path::Path};
use token_pulse_integration::notify_config::{
    ConfigError, MAX_CONFIG_BYTES, ManagedNotifyCommand,
    windows::{ConfigFileError, prepare_enable_file, prepare_restore_file, read_config},
};
fn command() -> ManagedNotifyCommand {
    ManagedNotifyCommand::new(
        &std::env::current_exe().unwrap(),
        "0123456789abcdef0123456789abcdef",
    )
    .unwrap()
}
fn security(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, GetFileSecurityW,
        OWNER_SECURITY_INFORMATION,
    };
    let path: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let flags = OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    unsafe {
        let mut needed = 0;
        GetFileSecurityW(path.as_ptr(), flags, std::ptr::null_mut(), 0, &mut needed);
        assert!(needed > 0 && needed <= 65536);
        let mut bytes = vec![0u8; needed as usize];
        assert_ne!(
            GetFileSecurityW(
                path.as_ptr(),
                flags,
                bytes.as_mut_ptr().cast(),
                needed,
                &mut needed
            ),
            0
        );
        bytes
    }
}
#[test]
fn actual_apply_and_restore_preserve_bytes_acl_and_only_touch_explicit_config() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    let before = "\u{feff}# 中文\r\n\"notify\"  = [\r\n 'original.exe', # keep syntax\r\n 'space arg',\r\n] # tail\r\nmodel='keep'\r\n[profiles.work]\r\nnotify=['nested']\r\n";
    fs::write(&path, before).unwrap();
    let acl = security(&path);
    let sentinel = home.path().join("do-not-open.fixture");
    fs::write(&sentinel, b"unrelated content").unwrap();
    let plan = prepare_enable_file(home.path(), &command()).unwrap();
    assert!(!plan.creates_config());
    assert_eq!(
        plan.before_value(),
        Some("[\r\n 'original.exe', # keep syntax\r\n 'space arg',\r\n]")
    );
    let expected = before.replacen(plan.before_value().unwrap(), plan.after_value().unwrap(), 1);
    plan.apply().unwrap();
    assert_eq!(read_config(home.path()).unwrap(), expected.as_bytes());
    assert_eq!(security(&path), acl);
    assert_eq!(plan.apply(), Err(ConfigError::StalePlan.into()));
    // Undo is prepared from the latest contents; unrelated user edits survive.
    let changed = expected.replace("model='keep'", "model='changed'\r\nnew_key='user'");
    fs::write(&path, &changed).unwrap();
    let undo = prepare_restore_file(home.path(), plan.restore_record()).unwrap();
    undo.apply().unwrap();
    assert_eq!(
        read_config(home.path()).unwrap(),
        before
            .replace("model='keep'", "model='changed'\r\nnew_key='user'")
            .as_bytes()
    );
    assert_eq!(security(&path), acl);
    assert_eq!(fs::read(sentinel).unwrap(), b"unrelated content");
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 2);
}
#[test]
fn preview_binds_exact_bytes_file_identity_and_missing_versus_empty() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    let absent = prepare_enable_file(home.path(), &command()).unwrap();
    fs::write(&path, b"").unwrap();
    assert_eq!(absent.apply(), Err(ConfigError::StalePlan.into()));
    assert_eq!(fs::read(&path).unwrap(), b"");
    let plan = prepare_enable_file(home.path(), &command()).unwrap();
    fs::rename(&path, home.path().join("user-moved-original")).unwrap();
    fs::write(&path, b"").unwrap();
    assert_eq!(plan.apply(), Err(ConfigError::StalePlan.into()));
    assert_eq!(fs::read(&path).unwrap(), b"");
    let plan = prepare_enable_file(home.path(), &command()).unwrap();
    fs::write(&path, b"new_key='user'\n").unwrap();
    assert_eq!(plan.apply(), Err(ConfigError::StalePlan.into()));
    assert_eq!(fs::read(&path).unwrap(), b"new_key='user'\n");
}
#[test]
fn restoration_refuses_changed_notify_and_preview_becomes_stale_after_any_edit() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    let plan = prepare_enable_file(home.path(), &command()).unwrap();
    plan.apply().unwrap();
    let undo = prepare_restore_file(home.path(), plan.restore_record()).unwrap();
    let installed = read_config(home.path()).unwrap();
    let mut changed = installed.clone();
    changed.extend_from_slice(b"model='user'\n");
    fs::write(&path, &changed).unwrap();
    assert_eq!(undo.apply(), Err(ConfigError::StalePlan.into()));
    assert_eq!(read_config(home.path()).unwrap(), changed);
    let undo = prepare_restore_file(home.path(), plan.restore_record()).unwrap();
    undo.apply().unwrap();
    assert_eq!(read_config(home.path()).unwrap(), b"model='user'\n"); // removing our key never removes the user's config
    fs::write(&path, b"notify=['user-reassigned']\n").unwrap();
    assert_eq!(
        prepare_restore_file(home.path(), plan.restore_record()).err(),
        Some(ConfigError::OwnershipConflict.into())
    );
    assert_eq!(fs::read(&path).unwrap(), b"notify=['user-reassigned']\n");
}
#[test]
fn readonly_hardlink_directory_bounds_and_busy_writer_leave_all_bytes_unchanged() {
    use std::os::windows::fs::OpenOptionsExt;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config.toml");
    let before = b"model='keep'\n";
    fs::write(&path, before).unwrap();
    let plan = prepare_enable_file(home.path(), &command()).unwrap();
    fs::hard_link(&path, home.path().join("alias")).unwrap();
    assert_eq!(plan.apply(), Err(ConfigFileError::UnsafeFile));
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::remove_file(home.path().join("alias")).unwrap();
    let original_permissions = fs::metadata(&path).unwrap().permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    assert_eq!(plan.apply(), Err(ConfigFileError::PermissionDenied));
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::set_permissions(&path, original_permissions).unwrap();
    let writer = fs::OpenOptions::new()
        .write(true)
        .share_mode(7)
        .open(&path)
        .unwrap();
    assert_eq!(
        prepare_enable_file(home.path(), &command()).err(),
        Some(ConfigFileError::Busy)
    );
    drop(writer);
    fs::write(&path, vec![b'#'; MAX_CONFIG_BYTES + 1]).unwrap();
    assert_eq!(
        prepare_enable_file(home.path(), &command()).err(),
        Some(ConfigError::TooLarge.into())
    );
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert_eq!(
        prepare_enable_file(home.path(), &command()).err(),
        Some(ConfigFileError::UnsafeFile)
    );
    assert_eq!(
        prepare_enable_file(Path::new(r"\\server\home"), &command()).err(),
        Some(ConfigFileError::UnsafePath)
    );
}
