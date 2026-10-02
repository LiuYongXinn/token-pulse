//! Strict headless invocation. Raw provider JSON is borrowed, parsed and then discarded.
use crate::notify_registry::valid_registration_id;
use std::ffi::OsString;
use token_pulse_core::notify::{NotifyWakeHint, parse_codex_notification};

pub struct NotifyInvocation {
    registration_id: String,
    hint: Option<NotifyWakeHint>,
}
impl NotifyInvocation {
    pub fn registration_id(&self) -> &str {
        &self.registration_id
    }
    pub fn hint(&self) -> Option<&NotifyWakeHint> {
        self.hint.as_ref()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvocationError {
    InvalidArguments,
    InvalidPayload,
}
impl std::fmt::Display for InvocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidArguments => "notify_invocation_invalid_arguments",
            Self::InvalidPayload => "notify_invocation_invalid_payload",
        })
    }
}
impl std::error::Error for InvocationError {}

/// Arguments exclude argv[0]. A misplaced notify flag is an error, never a request to open a GUI.
pub fn parse_invocation(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<Option<NotifyInvocation>, InvocationError> {
    let mut arguments = arguments.into_iter();
    let Some(first) = arguments.next() else {
        return Ok(None);
    };
    if first != "--tokenpulse-notify" {
        if arguments.any(|argument| argument == "--tokenpulse-notify") {
            return Err(InvocationError::InvalidArguments);
        }
        return Ok(None);
    }
    let flag = arguments.next().ok_or(InvocationError::InvalidArguments)?;
    let id = arguments.next().ok_or(InvocationError::InvalidArguments)?;
    let payload = arguments.next().ok_or(InvocationError::InvalidArguments)?;
    if flag != "--integration" || arguments.next().is_some() {
        return Err(InvocationError::InvalidArguments);
    }
    let id = id
        .to_str()
        .filter(|id| valid_registration_id(id))
        .ok_or(InvocationError::InvalidArguments)?;
    let payload = payload.to_str().ok_or(InvocationError::InvalidPayload)?;
    let hint = parse_codex_notification(payload.as_bytes())
        .map_err(|_| InvocationError::InvalidPayload)?;
    Ok(Some(NotifyInvocation {
        registration_id: id.to_owned(),
        hint,
    }))
}

#[cfg(windows)]
pub mod windows {
    use super::NotifyInvocation;
    use crate::{
        notify_channel::windows::send_hint,
        notify_config::{owns_current_notify, windows::read_config},
        notify_original::{OriginalCommandError, run_original},
        notify_registry::{RegistryError, windows::NotifyRegistry},
    };
    use std::path::Path;
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum WakeOutcome {
        Ignored,
        Delivered,
        Pending,
    }
    pub struct DispatchOutcome {
        pub wake: Result<WakeOutcome, RegistryError>,
        /// None means no explicitly retained original command was executed.
        pub original: Option<Result<(), OriginalCommandError>>,
    }
    /// Own-command dispatch never trusts raw arguments from UI or notify payload fields.
    /// Provider JSON exists only in the caller's OS argument; it is not added to the minimal DTO.
    pub async fn dispatch(
        registry: &NotifyRegistry,
        current_executable: &Path,
        invocation: &NotifyInvocation,
        payload: &std::ffi::OsStr,
    ) -> Result<DispatchOutcome, RegistryError> {
        let checked = super::parse_invocation([
            "--tokenpulse-notify".into(),
            "--integration".into(),
            invocation.registration_id().into(),
            payload.to_owned(),
        ])
        .map_err(|_| RegistryError::InvalidRecord)?
        .ok_or(RegistryError::InvalidRecord)?;
        if checked.hint() != invocation.hint() {
            return Err(RegistryError::InvalidRecord);
        }
        if invocation.hint().is_none() {
            return Ok(DispatchOutcome {
                wake: Ok(WakeOutcome::Ignored),
                original: None,
            });
        }
        let registered = registry.get(invocation.registration_id())?;
        if Path::new(&registered.restore_record().installed_arguments()[0]) != current_executable {
            return Err(RegistryError::Unauthorized);
        }
        let owned = || {
            let config =
                read_config(registered.codex_home()).map_err(|_| RegistryError::InvalidRecord)?;
            owns_current_notify(&config, registered.restore_record())
                .map_err(|_| RegistryError::InvalidRecord)
        };
        if !owned()? {
            return Ok(DispatchOutcome {
                wake: Ok(WakeOutcome::Ignored),
                original: None,
            });
        }
        // Either result remains observable: a local channel/marker failure must not silently
        // suppress an explicitly retained legacy command, nor can legacy failure lose our wake.
        let wake = wake_only(registry, current_executable, invocation).await;
        let original = if registered.chain_original() && owned()? {
            Some(run_original(&registered, current_executable, payload))
        } else {
            None
        };
        Ok(DispatchOutcome { wake, original })
    }
    /// Wake routing only; original-command execution is a separate explicitly authorized step.
    pub async fn wake_only(
        registry: &NotifyRegistry,
        current_executable: &Path,
        invocation: &NotifyInvocation,
    ) -> Result<WakeOutcome, RegistryError> {
        let Some(hint) = invocation.hint() else {
            return Ok(WakeOutcome::Ignored);
        };
        let registered = registry.get(invocation.registration_id())?;
        if Path::new(&registered.restore_record().installed_arguments()[0]) != current_executable {
            return Err(RegistryError::Unauthorized);
        }
        let config =
            read_config(registered.codex_home()).map_err(|_| RegistryError::InvalidRecord)?;
        if !owns_current_notify(&config, registered.restore_record())
            .map_err(|_| RegistryError::InvalidRecord)?
        {
            return Ok(WakeOutcome::Ignored);
        }
        match send_hint(registered.capability(), hint).await {
            Ok(()) => Ok(WakeOutcome::Delivered),
            Err(_) => {
                registry.mark_pending(registered.capability())?;
                Ok(WakeOutcome::Pending)
            }
        }
    }
}
