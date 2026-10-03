//! Exclusive debug acceptance: parked own source -> production power route -> rescan.
//! The system observer never requests sleep or sends power messages. OS evidence is separate.
use std::{
    fs,
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;
use token_pulse_core::{
    numeric::EpochMs,
    protocol::{CoverageState, DateRange, DimensionSelection, UsageFilter},
    taskbar::TaskbarPosition,
};
use token_pulse_store::SourceRecord;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, SMTO_ABORTIFHUNG, SMTO_BLOCK,
    SendMessageTimeoutW, WM_POWERBROADCAST,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    AuthoredMessages,
    SystemObserver,
    AuthoredTaskbarMessages,
    SystemTaskbarObserver,
    AuthoredTaskbarRightMessages,
    SystemTaskbarRightObserver,
}
impl Scene {
    fn authored(self) -> bool {
        matches!(
            self,
            Self::AuthoredMessages
                | Self::AuthoredTaskbarMessages
                | Self::AuthoredTaskbarRightMessages
        )
    }
    fn taskbar(self) -> bool {
        matches!(
            self,
            Self::AuthoredTaskbarMessages
                | Self::SystemTaskbarObserver
                | Self::AuthoredTaskbarRightMessages
                | Self::SystemTaskbarRightObserver
        )
    }
    fn position(self) -> TaskbarPosition {
        if matches!(
            self,
            Self::AuthoredTaskbarRightMessages | Self::SystemTaskbarRightObserver
        ) {
            TaskbarPosition::ApplicationRight
        } else {
            TaskbarPosition::NotificationLeft
        }
    }
}
fn parse(args: &[String]) -> Result<Option<Scene>, &'static str> {
    if !args.iter().any(|arg| arg.starts_with("--native-power-")) {
        return Ok(None);
    }
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["--native-smoke", "--native-power-messages-smoke"] => Ok(Some(Scene::AuthoredMessages)),
        ["--native-smoke", "--native-power-resume-smoke"] => Ok(Some(Scene::SystemObserver)),
        ["--native-smoke", "--native-power-taskbar-messages-smoke"] => {
            Ok(Some(Scene::AuthoredTaskbarMessages))
        }
        ["--native-smoke", "--native-power-taskbar-resume-smoke"] => {
            Ok(Some(Scene::SystemTaskbarObserver))
        }
        [
            "--native-smoke",
            "--native-power-taskbar-messages-smoke",
            "--application-right",
        ] => Ok(Some(Scene::AuthoredTaskbarRightMessages)),
        [
            "--native-smoke",
            "--native-power-taskbar-resume-smoke",
            "--application-right",
        ] => Ok(Some(Scene::SystemTaskbarRightObserver)),
        _ => Err("native power acceptance requires its exclusive exact scene"),
    }
}
pub fn scene() -> Result<Option<Scene>, &'static str> {
    parse(&std::env::args().skip(1).collect::<Vec<_>>())
}
#[derive(Default)]
pub struct PowerEvents {
    suspend: AtomicU64,
    automatic_resume: AtomicU64,
    user_resume: AtomicU64,
}
impl PowerEvents {
    fn record(&self, event: u32) {
        let counter = match event {
            PBT_APMSUSPEND => &self.suspend,
            PBT_APMRESUMEAUTOMATIC => &self.automatic_resume,
            PBT_APMRESUMESUSPEND => &self.user_resume,
            _ => return,
        };
        counter.fetch_add(1, Ordering::Release);
    }
    fn snapshot(&self) -> (u64, u64, u64) {
        (
            self.suspend.load(Ordering::Acquire),
            self.automatic_resume.load(Ordering::Acquire),
            self.user_resume.load(Ordering::Acquire),
        )
    }
}
pub fn record_event(app: &tauri::AppHandle, event: u32) {
    if let Some(events) = app.try_state::<PowerEvents>() {
        events.record(event);
    }
}
pub fn start(app: tauri::AppHandle, scene: Scene) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        let result = verify(&app, scene);
        if let Some(service) = super::taskbar_commands::service(&app) {
            service.shutdown();
        }
        if let Err(error) = result {
            eprintln!("NATIVE_POWER_ACCEPTANCE_FAILED: {error}");
            app.exit(1);
        } else {
            app.exit(0);
        }
    });
}
fn wait(condition: impl Fn() -> bool, timeout: Duration, label: &str) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while !condition() {
        if Instant::now() >= deadline {
            return Err(format!("native power condition timed out: {label}"));
        }
        thread::sleep(Duration::from_millis(25));
    }
    Ok(())
}
fn filter() -> UsageFilter {
    UsageFilter {
        range: DateRange {
            start_ms: EpochMs::new(0).unwrap(),
            end_ms: EpochMs::new(5000).unwrap(),
            timezone: "UTC".into(),
        },
        sources: DimensionSelection::All {},
        models: DimensionSelection::All {},
        projects: DimensionSelection::All {},
        sessions: DimensionSelection::All {},
    }
}
fn authored_message(main: &tauri::WebviewWindow, event: u32) -> Result<(), String> {
    let hwnd = main.hwnd().map_err(|_| "own main handle")?.0;
    let mut result = 0;
    if unsafe {
        SendMessageTimeoutW(
            hwnd as _,
            WM_POWERBROADCAST,
            event as usize,
            0,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            2000,
            &mut result,
        )
    } == 0
    {
        return Err("own power message failed or timed out".into());
    }
    Ok(())
}
fn verify(app: &tauri::AppHandle, scene: Scene) -> Result<(), String> {
    if self::scene().map_err(str::to_owned)? != Some(scene)
        || !app.config().identifier.ends_with(".dev")
    {
        return Err("exclusive debug power scene required".into());
    }
    let state = app.state::<super::RuntimeState>();
    if !state
        .data_directory
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("native-probe-"))
    {
        return Err("isolated power database required".into());
    }
    let database = state
        .database
        .as_ref()
        .map_err(|_| "database unavailable")?;
    let collector = state
        .collector
        .as_ref()
        .map_err(|_| "collector unavailable")?;
    let main = app.get_webview_window("main").ok_or("main missing")?;
    main.hide().map_err(|_| "hide own main")?;
    let home = state.data_directory.join("synthetic-power-home");
    fs::create_dir_all(home.join("sessions")).map_err(|_| "fixture directory")?;
    let path = home.join("sessions").join("resume.jsonl");
    let initial = b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"native-power-session\"}}\n{\"timestamp\":\"1970-01-01T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"total_tokens\":3}}}}\n";
    fs::write(&path, initial).map_err(|_| "fixture initialization")?;
    let writable_permissions = fs::metadata(&path)
        .map_err(|_| "fixture metadata")?
        .permissions();
    let mut permissions = writable_permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).map_err(|_| "fixture readonly")?;
    database
        .add_source(SourceRecord {
            source_id: "native-power".into(),
            root_path: home.to_str().ok_or("home encoding")?.into(),
            directory_identity: None,
            kind: "local".into(),
            enabled: true,
            created_at_ms: 1,
        })
        .map_err(|_| "source registration")?;
    collector.reconcile();
    wait(
        || {
            database
                .usage_totals(&filter())
                .is_ok_and(|totals| totals.total_tokens.as_str() == "3")
                && database
                    .usage_coverage(&filter())
                    .is_ok_and(|coverage| matches!(coverage.state, CoverageState::Complete))
                && collector.status().queue_length == 0
        },
        Duration::from_secs(15),
        "baseline source scanned",
    )?;
    // Complete window/scope initialization before parking and appending the pending data.
    let taskbar = if scene.taskbar() {
        Some(super::power_taskbar_smoke::prepare(app, scene.position())?)
    } else {
        None
    };
    // Park the fixture before appending: periodic proof jobs must not be mistaken for resume.
    // This direct preparation is distinct from the power events observed below.
    collector.suspend();
    wait(
        || collector.status().suspended,
        Duration::from_secs(5),
        "fixture collector parked",
    )?;
    fs::set_permissions(&path, writable_permissions).map_err(|_| "fixture write preparation")?;
    let addition = b"{\"timestamp\":\"1970-01-01T00:00:02Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"total_tokens\":7}}}}\n";
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .and_then(|mut file| file.write_all(addition))
        .map_err(|_| "fixture append")?;
    let mut permissions = fs::metadata(&path)
        .map_err(|_| "fixture metadata")?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).map_err(|_| "fixture readonly restore")?;
    let expected = [initial.as_slice(), addition.as_slice()].concat();
    thread::sleep(Duration::from_millis(250));
    if database
        .usage_totals(&filter())
        .map_err(|_| "parked totals")?
        .total_tokens
        .as_str()
        != "3"
    {
        return Err("parked source was collected before resume".into());
    }
    let events = app.state::<PowerEvents>();
    let before = events.snapshot();
    println!(
        "NATIVE_POWER_READY: scene={scene:?} pid={} fixture_parked=true baseline_tokens=3 pending_tokens=7 watcher=false poll_seconds=3600 computer_restart=false",
        std::process::id()
    );
    if scene.authored() {
        authored_message(&main, PBT_APMSUSPEND)?;
        if let Some(probe) = &taskbar {
            probe.wait_suspended(app)?;
        }
        authored_message(&main, PBT_APMRESUMEAUTOMATIC)?;
    }
    let power_deadline = Instant::now() + Duration::from_secs(150);
    loop {
        let observed = events.snapshot();
        if observed.0 > before.0 && (observed.1 > before.1 || observed.2 > before.2) {
            break;
        }
        if database
            .usage_totals(&filter())
            .map_err(|_| "waiting power totals")?
            .total_tokens
            .as_str()
            != "3"
        {
            return Err(format!(
                "pending fixture collected before the suspend/resume barrier: events={observed:?} baseline={before:?}"
            ));
        }
        if Instant::now() >= power_deadline {
            return Err("suspend and resume events not observed within deadline".into());
        }
        thread::sleep(Duration::from_millis(25));
    }
    wait(
        || {
            !collector.status().suspended
                && database
                    .usage_totals(&filter())
                    .is_ok_and(|totals| totals.total_tokens.as_str() == "10")
                && database
                    .usage_coverage(&filter())
                    .is_ok_and(|coverage| matches!(coverage.state, CoverageState::Complete))
                && collector.status().queue_length == 0
        },
        Duration::from_secs(15),
        "resume rescan collected pending source",
    )?;
    if scene.authored() && !scene.taskbar() {
        authored_message(&main, PBT_APMRESUMESUSPEND)?;
        authored_message(&main, PBT_APMRESUMEAUTOMATIC)?;
        thread::sleep(Duration::from_millis(250));
    }
    if database
        .usage_totals(&filter())
        .map_err(|_| "final totals")?
        .total_tokens
        .as_str()
        != "10"
        || fs::read(&path).map_err(|_| "fixture readback")? != expected
        || !fs::metadata(&path)
            .map_err(|_| "fixture permissions")?
            .permissions()
            .readonly()
        || main.is_visible().map_err(|_| "main visibility")?
    {
        return Err("resume duplicated data, changed source or opened main".into());
    }
    if let Some(probe) = taskbar {
        probe.verify_resumed(app)?;
    }
    let observed = events.snapshot();
    let marker = if scene.authored() {
        "NATIVE_POWER_MESSAGES_OK"
    } else {
        "NATIVE_POWER_RESUME_OBSERVER_OK"
    };
    println!(
        "{marker}: suspend={} automatic_resume={} user_resume={} totals=3/10 source_readonly=true hidden_main=true authored_messages={} actual_system_transition_requires_os_evidence=true computer_restart=false",
        observed.0 - before.0,
        observed.1 - before.1,
        observed.2 - before.2,
        scene.authored()
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }
    #[test]
    fn exclusive_power_modes_reject_mixed_unknown_and_duplicate_flags() {
        assert_eq!(
            parse(&args(&["--native-smoke", "--native-power-messages-smoke"])),
            Ok(Some(Scene::AuthoredMessages))
        );
        assert_eq!(
            parse(&args(&["--native-smoke", "--native-power-resume-smoke"])),
            Ok(Some(Scene::SystemObserver))
        );
        assert_eq!(
            parse(&args(&[
                "--native-smoke",
                "--native-power-taskbar-messages-smoke"
            ])),
            Ok(Some(Scene::AuthoredTaskbarMessages))
        );
        assert_eq!(
            parse(&args(&[
                "--native-smoke",
                "--native-power-taskbar-resume-smoke"
            ])),
            Ok(Some(Scene::SystemTaskbarObserver))
        );
        assert_eq!(
            parse(&args(&["--native-smoke", "--native-taskbar-actions-smoke"])),
            Ok(None)
        );
        for (flag, scene) in [
            (
                "--native-power-taskbar-messages-smoke",
                Scene::AuthoredTaskbarRightMessages,
            ),
            (
                "--native-power-taskbar-resume-smoke",
                Scene::SystemTaskbarRightObserver,
            ),
        ] {
            assert_eq!(
                parse(&args(&["--native-smoke", flag, "--application-right"])),
                Ok(Some(scene))
            );
            assert!(scene.taskbar());
            assert_eq!(scene.position(), TaskbarPosition::ApplicationRight);
        }
        assert!(Scene::AuthoredTaskbarRightMessages.authored());
        assert!(!Scene::SystemTaskbarRightObserver.authored());
        assert_eq!(
            Scene::SystemTaskbarObserver.position(),
            TaskbarPosition::NotificationLeft
        );
        for invalid in [
            vec![
                "--native-smoke",
                "--native-power-resume-smoke",
                "--application-right",
            ],
            vec![
                "--native-smoke",
                "--application-right",
                "--native-power-taskbar-resume-smoke",
            ],
            vec![
                "--native-smoke",
                "--native-power-taskbar-resume-smoke",
                "--application-right",
                "--application-right",
            ],
            vec!["--native-power-resume-smoke"],
            vec!["--native-power-taskbar-resume-smoke"],
            vec![
                "--native-smoke",
                "--native-power-taskbar-resume-smoke",
                "--native-power-resume-smoke",
            ],
            vec![
                "--native-smoke",
                "--native-power-resume-smoke",
                "--native-taskbar-actions-smoke",
            ],
            vec!["--native-smoke", "--native-power-unknown"],
            vec![
                "--native-smoke",
                "--native-power-resume-smoke",
                "--native-power-resume-smoke",
            ],
        ] {
            assert!(parse(&args(&invalid)).is_err());
        }
    }
    #[test]
    fn unknown_power_events_do_not_satisfy_the_resume_barrier() {
        let counters = PowerEvents::default();
        counters.record(0);
        counters.record(0xf00d);
        assert_eq!(counters.snapshot(), (0, 0, 0));
        counters.record(PBT_APMSUSPEND);
        counters.record(PBT_APMRESUMEAUTOMATIC);
        counters.record(PBT_APMRESUMESUSPEND);
        counters.record(PBT_APMRESUMESUSPEND);
        assert_eq!(counters.snapshot(), (1, 1, 2));
    }
}
