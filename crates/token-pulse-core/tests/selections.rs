use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use token_pulse_core::{error::ErrorCode, selections::*, sources::SourceOrigin};
#[test]
fn selections_are_window_bound_single_use_and_expire() {
    let now = Instant::now();
    let mut selections = DirectorySelections::default();
    selections
        .insert(
            "one".into(),
            "main".into(),
            SelectedDirectory {
                path: PathBuf::from("synthetic-directory"),
                origin: SourceOrigin::Custom,
            },
            now,
        )
        .unwrap();
    assert_eq!(
        selections.take("one", "mini", now).err(),
        Some(ErrorCode::PermissionDenied)
    );
    assert_eq!(
        selections.take("one", "main", now).unwrap().path,
        PathBuf::from("synthetic-directory")
    );
    assert_eq!(
        selections.take("one", "main", now).err(),
        Some(ErrorCode::InvalidQuery)
    );
    selections
        .insert(
            "two".into(),
            "main".into(),
            SelectedDirectory {
                path: PathBuf::from("other"),
                origin: SourceOrigin::Wsl,
            },
            now,
        )
        .unwrap();
    assert_eq!(
        selections
            .take("two", "main", now + Duration::from_secs(300))
            .err(),
        Some(ErrorCode::StaleConfirmation)
    );
}
