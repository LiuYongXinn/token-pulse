//! Opt-in debug cold starts. Real existing-account scenes require an additional explicit flag.
use std::{
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;
use token_pulse_core::{protocol::QuotaState, quota::AccountServiceCandidate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Seed,
    Ready,
    Disabled,
    Changed,
    LocalSeed,
    LocalReady,
    LocalTaskbar,
}
pub struct Scene {
    pub directory: String,
    pub phase: Phase,
}
pub fn scene() -> Result<Option<Scene>, &'static str> {
    parse(std::env::args().skip(1))
}
fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Scene>, &'static str> {
    let mut id = None;
    let mut phase = None;
    let mut enabled = false;
    let mut existing_account = false;
    for arg in args {
        if arg == "--native-smoke" {
            enabled = true;
        }
        if arg == "--native-existing-account" {
            if existing_account {
                return Err("duplicate existing-account opt-in");
            }
            existing_account = true;
        }
        if let Some(value) = arg.strip_prefix("--native-account-startup-id=") {
            if id.is_some() {
                return Err("duplicate startup id");
            }
            id = Some(uuid::Uuid::parse_str(value).map_err(|_| "invalid startup UUID")?);
        }
        if let Some(value) = arg.strip_prefix("--native-account-phase=") {
            if phase.is_some() {
                return Err("duplicate startup phase");
            }
            phase = Some(match value {
                "seed" => Phase::Seed,
                "ready" => Phase::Ready,
                "disabled" => Phase::Disabled,
                "changed" => Phase::Changed,
                "local_seed" => Phase::LocalSeed,
                "local_ready" => Phase::LocalReady,
                "local_taskbar" => Phase::LocalTaskbar,
                _ => return Err("invalid startup phase"),
            });
        }
    }
    match (id, phase, enabled) {
        (None, None, _) if !existing_account => Ok(None),
        (Some(id), Some(phase), true)
            if existing_account
                == matches!(
                    phase,
                    Phase::LocalSeed | Phase::LocalReady | Phase::LocalTaskbar
                ) =>
        {
            Ok(Some(Scene {
                directory: format!("native-account-startup-{}", id.simple()),
                phase,
            }))
        }
        _ => Err("startup probe requires native-smoke, UUID and phase"),
    }
}
pub fn start(app: tauri::AppHandle, phase: Phase) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        let result = verify(&app, phase);
        match &result {
            Ok(()) => eprintln!("NATIVE_ACCOUNT_COLD_OK: {phase:?}"),
            Err(error) => eprintln!("NATIVE_ACCOUNT_COLD_FAILED: {phase:?}: {error}"),
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}
fn save(
    app: &tauri::AppHandle,
    auto: bool,
    target: token_pulse_core::quota::AccountServiceTarget,
) -> Result<(), String> {
    let state = app.state::<super::RuntimeState>();
    let db = state.database.as_ref().map_err(|e| e.to_string())?;
    let (_, revision) = db
        .account_service_preferences()
        .map_err(|e| e.to_string())?;
    let handle = uuid::Uuid::new_v4().to_string();
    state
        .quota_selections
        .lock()
        .map_err(|_| "selection lock")?
        .insert(
            handle.clone(),
            AccountServiceCandidate {
                target,
                settings_revision: revision,
            },
            Instant::now(),
        )
        .map_err(|e| format!("{e:?}"))?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_smoke::evaluate(
        app,
        &main,
        &format!(
            r#"
        const before=await invoke('get_account_service_config',{{requestId:'cold-before'}});
        const saved=await invoke('save_account_service_config',{{requestId:'cold-save',request:{{
            selection_handle:{},auto_connect:{auto},expected_settings_revision:before.data.settings_revision
        }}}});
        if(!saved.data.configured || saved.data.auto_connect!=={auto})throw new Error('COLD_SAVE_INVALID');
    "#,
            serde_json::to_string(&handle).map_err(|e| e.to_string())?
        ),
    )
}
fn verify(app: &tauri::AppHandle, phase: Phase) -> Result<(), String> {
    let state = app.state::<super::RuntimeState>();
    if !state
        .data_directory
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("native-account-startup-"))
    {
        return Err("isolated cold-start directory required".into());
    }
    let db = state.database.as_ref().map_err(|e| e.to_string())?;
    let quota = state.quota.as_ref().map_err(|e| format!("{e:?}"))?;
    if matches!(
        phase,
        Phase::LocalSeed | Phase::LocalReady | Phase::LocalTaskbar
    ) {
        return verify_existing(app, phase);
    }
    let home = state.data_directory.join("synthetic-home");
    let program = home.join("synthetic-codex.exe");
    if phase == Phase::Seed {
        if home.exists()
            || db
                .account_service_preferences()
                .map_err(|e| e.to_string())?
                .0
                .target
                .is_some()
        {
            return Err("seed requires fresh database".into());
        }
        std::fs::create_dir(&home).map_err(|e| e.to_string())?;
        std::fs::write(home.join("fixture-mode"), "service").map_err(|e| e.to_string())?;
        std::fs::copy(
            std::env::current_exe()
                .map_err(|e| e.to_string())?
                .with_file_name("quota-fixture.exe"),
            &program,
        )
        .map_err(|e| e.to_string())?;
        save(
            app,
            true,
            token_pulse_quota::NativeService::inspect(&program, Some(&home))
                .map_err(|e| format!("{e:?}"))?,
        )?;
        if !matches!(
            quota.snapshot().map_err(|e| format!("{e:?}"))?.state,
            QuotaState::Disconnected
        ) || home.join("service.pid").exists()
        {
            return Err("save unexpectedly launched account service".into());
        }
        return Ok(());
    }
    let (prefs, _) = db
        .account_service_preferences()
        .map_err(|e| e.to_string())?;
    let target = prefs.target.ok_or("persisted target missing")?;
    if prefs.auto_connect != (phase != Phase::Disabled) {
        return Err("wrong persisted auto-connect state".into());
    }
    let expected = match phase {
        Phase::Ready => QuotaState::Ready,
        Phase::Disabled => QuotaState::Disconnected,
        Phase::Changed => QuotaState::Error,
        Phase::Seed | Phase::LocalSeed | Phase::LocalReady | Phase::LocalTaskbar => unreachable!(),
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let snapshot = loop {
        let current = quota.snapshot().map_err(|e| format!("{e:?}"))?;
        if std::mem::discriminant(&current.state) == std::mem::discriminant(&expected) {
            break current;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "cold-start state {:?}, expected {expected:?}",
                current.state
            ));
        }
        thread::sleep(Duration::from_millis(50));
    };
    let main = app.get_webview_window("main").ok_or("main missing")?;
    if phase == Phase::Ready {
        if snapshot.windows.len() != 1
            || snapshot.available_limits.len() != 2
            || snapshot.windows[0].remaining_percent != Some(75.0)
            || snapshot.fetched_at_ms.is_none()
        {
            return Err("cold-start quota independent expectation failed".into());
        }
        super::mini_smoke::evaluate(
            app,
            &main,
            r#"
            const quota=await invoke('get_account_quota',{requestId:'cold-ready'});
            if(quota.data.state!=='ready'||quota.data.windows[0].remaining_percent!==75)throw new Error('COLD_READY_IPC');
        "#,
        )?;
        let pid = std::fs::read_to_string(home.join("service.pid"))
            .map_err(|e| e.to_string())?
            .parse::<u32>()
            .map_err(|e| e.to_string())?;
        eprintln!("NATIVE_ACCOUNT_COLD_OWNED_PID: {pid}");
        std::fs::write(home.join("cold-owned-pid"), pid.to_string()).map_err(|e| e.to_string())?;
        save(app, false, target)?;
        if quota
            .snapshot()
            .map_err(|e| format!("{e:?}"))?
            .connection_epoch
            != snapshot.connection_epoch
        {
            return Err("saving disabled auto-connect replaced live connection".into());
        }
    } else {
        let expected_pid = std::fs::read(home.join("cold-owned-pid")).map_err(|e| e.to_string())?;
        if std::fs::read(home.join("service.pid")).map_err(|e| e.to_string())? != expected_pid {
            return Err(
                "disabled or changed target unexpectedly launched synthetic service".into(),
            );
        }
        if !snapshot.windows.is_empty()
            || snapshot.fetched_at_ms.is_some()
            || snapshot.selected_limit_id.is_some()
        {
            return Err("cold-start unknown data replaced with stale quota".into());
        }
        if phase == Phase::Changed && snapshot.error_code.is_none() {
            return Err("changed program failure missing".into());
        }
        super::mini_smoke::evaluate(
            app,
            &main,
            &format!(
                r#"
            const quota=await invoke('get_account_quota',{{requestId:'cold-empty'}});
            if(quota.data.state!=={}||quota.data.windows.length||quota.data.fetched_at_ms!==null)throw new Error('COLD_EMPTY_IPC');
        "#,
                serde_json::to_string(&expected).map_err(|e| e.to_string())?
            ),
        )?;
        if phase == Phase::Disabled {
            save(app, true, target)?;
            // Change only our synthetic executable after its reviewed target has been saved.
            use std::io::Write;
            std::fs::OpenOptions::new()
                .append(true)
                .open(&program)
                .map_err(|e| e.to_string())?
                .write_all(b"synthetic fingerprint change")
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
fn verify_existing(app: &tauri::AppHandle, phase: Phase) -> Result<(), String> {
    let state = app.state::<super::RuntimeState>();
    let db = state.database.as_ref().map_err(|e| e.to_string())?;
    let quota = state.quota.as_ref().map_err(|e| format!("{e:?}"))?;
    let (prefs, _) = db
        .account_service_preferences()
        .map_err(|e| e.to_string())?;
    if phase == Phase::LocalSeed {
        if prefs.target.is_some() {
            return Err("existing-account seed requires fresh isolated database".into());
        }
        let target = token_pulse_quota::detect_local_service(None).map_err(|e| format!("{e:?}"))?;
        save(app, true, target)?;
        if !matches!(
            quota.snapshot().map_err(|e| format!("{e:?}"))?.state,
            QuotaState::Disconnected
        ) {
            return Err("saving existing target launched account service".into());
        }
        return Ok(());
    }
    if !prefs.auto_connect || prefs.target.is_none() {
        return Err("existing cold-start configuration missing".into());
    }
    let deadline = Instant::now() + Duration::from_secs(25);
    let snapshot = loop {
        let snapshot = quota.snapshot().map_err(|e| format!("{e:?}"))?;
        if matches!(snapshot.state, QuotaState::Ready) && snapshot.fetched_at_ms.is_some() {
            break snapshot;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "existing account cold-start unavailable: {:?}",
                snapshot.state
            ));
        }
        thread::sleep(Duration::from_millis(50));
    };
    let epoch = serde_json::to_string(&snapshot.connection_epoch).map_err(|e| e.to_string())?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    super::mini_smoke::evaluate(
        app,
        &main,
        &format!(
            r#"
        const q=(await invoke('get_account_quota',{{requestId:'cold-existing-main'}})).data;
        if(q.state!=='ready'||q.connection_epoch!=={epoch}||q.fetched_at_ms===null)throw new Error('EXISTING_MAIN_PROOF');
        await wait(()=>document.querySelectorAll('.quota-overview .quota-period').length===q.windows.length);
        const values=q.windows.filter(w=>w.remaining_percent!==null).map(w=>w.remaining_percent);
        await wait(()=>JSON.stringify([...document.querySelectorAll('.quota-overview progress')].map(p=>p.value))===JSON.stringify(values));
    "#
        ),
    )?;
    super::mini_window::show(app)?;
    let mini = app.get_webview_window("mini").ok_or("mini missing")?;
    super::mini_smoke::evaluate(
        app,
        &mini,
        &format!(
            r#"
        const q=(await invoke('get_account_quota',{{requestId:'cold-existing-mini'}})).data;
        if(q.state!=='ready'||q.connection_epoch!=={epoch}||q.fetched_at_ms===null)throw new Error('EXISTING_MINI_PROOF');
        await wait(()=>document.querySelector('.mini-quota:not(:disabled)'));
        document.querySelector('.mini-quota').click();
        await wait(()=>document.querySelector('.mini-window.expanded') && document.querySelector('.mini-quota-details'));
        await wait(()=>document.querySelectorAll('.mini-quota-details .quota-period').length===q.windows.length);
        const values=q.windows.filter(w=>w.remaining_percent!==null).map(w=>w.remaining_percent);
        await wait(()=>JSON.stringify([...document.querySelectorAll('.mini-quota-details progress')].map(p=>p.value))===JSON.stringify(values));
    "#
        ),
    )?;
    eprintln!(
        "NATIVE_ACCOUNT_EXISTING_COLD_DISPLAY_OK: real persisted startup, main/mini IPC and rendered progress, no identity or quota values logged"
    );
    if phase == Phase::LocalTaskbar {
        super::quota_taskbar_smoke::verify(app)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_scene_never_accepts_a_path_or_partial_authorization() {
        let args = |id: &str, phase: &str| {
            vec![
                "--native-smoke".into(),
                format!("--native-account-startup-id={id}"),
                format!("--native-account-phase={phase}"),
            ]
        };
        let id = "bd4a2122-8066-4e03-a846-3665274fa201";
        let scene = parse(args(id, "ready")).unwrap().unwrap();
        assert_eq!(
            scene.directory,
            "native-account-startup-bd4a212280664e03a8463665274fa201"
        );
        assert_eq!(scene.phase, Phase::Ready);
        assert!(parse(args("../real-data", "ready")).is_err());
        assert!(parse(args(id, "unknown")).is_err());
        assert!(parse(args(id, "seed").into_iter().skip(1)).is_err());
        let mut duplicate = args(id, "seed");
        duplicate.push(format!("--native-account-startup-id={id}"));
        assert!(parse(duplicate).is_err());
        assert!(parse(Vec::<String>::new()).unwrap().is_none());
    }
    #[test]
    fn existing_account_scene_requires_separate_explicit_opt_in() {
        let args = vec![
            "--native-smoke".into(),
            "--native-account-startup-id=bd4a2122-8066-4e03-a846-3665274fa201".into(),
            "--native-account-phase=local_ready".into(),
        ];
        assert!(parse(args.clone()).is_err());
        let mut enabled = args;
        enabled.push("--native-existing-account".into());
        assert_eq!(
            parse(enabled.clone()).unwrap().unwrap().phase,
            Phase::LocalReady
        );
        enabled.push("--native-existing-account".into());
        assert!(parse(enabled).is_err());
        assert!(parse(vec!["--native-existing-account".into()]).is_err());
        assert!(
            parse(vec![
                "--native-smoke".into(),
                "--native-account-startup-id=bd4a2122-8066-4e03-a846-3665274fa201".into(),
                "--native-account-phase=seed".into(),
                "--native-existing-account".into(),
            ])
            .is_err()
        );
    }
    #[test]
    fn native_taskbar_account_scene_is_not_a_default_or_synthetic_scene() {
        let mut args = vec![
            "--native-smoke".into(),
            "--native-account-startup-id=bd4a2122-8066-4e03-a846-3665274fa201".into(),
            "--native-account-phase=local_taskbar".into(),
        ];
        assert!(parse(args.clone()).is_err());
        args.push("--native-existing-account".into());
        assert_eq!(parse(args).unwrap().unwrap().phase, Phase::LocalTaskbar);
    }
}
