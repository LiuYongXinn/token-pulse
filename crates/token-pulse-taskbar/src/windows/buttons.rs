//! Read-only, bounded geometry of the verified primary task list. No names or actions.
use super::topology::{
    DpiGuard, ProbeError, ScreenRect, TaskbarTopology, discover_primary_taskbar,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
        UI::Accessibility::*,
    },
    core::Interface,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonCoverage {
    pub list: ScreenRect,
    pub occupied: Vec<ScreenRect>,
}
impl ButtonCoverage {
    pub fn rightmost(&self) -> i32 {
        self.occupied
            .iter()
            .map(|r| r.right)
            .max()
            .unwrap_or(self.list.left)
    }
    pub(crate) fn validate(&self) -> Result<(), ProbeError> {
        if !self.list.valid() || self.occupied.len() > 256 {
            return Err(ProbeError::UnexpectedStructure);
        }
        for item in &self.occupied {
            if !item.valid()
                || item.left < self.list.left
                || item.right > self.list.right
                || item.top < self.list.top
                || item.bottom > self.list.bottom
            {
                return Err(ProbeError::UnsafeGeometry);
            }
        }
        Ok(())
    }
}
#[derive(Clone)]
struct Identity {
    root: usize,
    list: usize,
    pid: u32,
    birth: [u32; 2],
    topology: TaskbarTopology,
}
impl Identity {
    fn capture() -> Result<Self, ProbeError> {
        let _dpi = DpiGuard::enter()?;
        let windows = discover_primary_taskbar()?;
        Ok(Self {
            root: windows.root as usize,
            list: windows.list as usize,
            pid: windows.pid,
            birth: windows.birth,
            topology: windows.topology()?,
        })
    }
    fn verify(&self) -> Result<(), ProbeError> {
        let current = Self::capture()?;
        if self.root != current.root
            || self.list != current.list
            || self.pid != current.pid
            || self.birth != current.birth
            || self.topology != current.topology
        {
            Err(ProbeError::UnexpectedStructure)
        } else {
            Ok(())
        }
    }
}
struct Request {
    identity: Identity,
    reply: SyncSender<Result<ButtonCoverage, ProbeError>>,
}
/// One MTA worker without windows; no COM interface leaves its apartment.
/// A timeout poisons this probe so a blocked provider cannot accumulate more work.
pub struct ButtonProbe {
    sender: SyncSender<Request>,
    poisoned: Arc<AtomicBool>,
    worker: thread::JoinHandle<()>,
}
impl ButtonProbe {
    pub fn start() -> Result<Self, ProbeError> {
        let (sender, receiver) = mpsc::sync_channel::<Request>(1);
        let poisoned = Arc::new(AtomicBool::new(false));
        let worker_poisoned = poisoned.clone();
        let worker = thread::Builder::new()
            .name("taskbar-button-geometry".into())
            .spawn(move || {
                let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
                loop {
                    let request = match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout)
                            if !worker_poisoned.load(Ordering::Acquire) =>
                        {
                            continue;
                        }
                        Err(_) => break,
                    };
                    let result = if initialized && !worker_poisoned.load(Ordering::Acquire) {
                        query(&request.identity)
                    } else {
                        Err(ProbeError::UnexpectedStructure)
                    };
                    let _ = request.reply.send(result);
                    if worker_poisoned.load(Ordering::Acquire) {
                        break;
                    }
                }
                if initialized {
                    unsafe {
                        CoUninitialize();
                    }
                }
            })
            .map_err(|_| ProbeError::Os)?;
        Ok(Self {
            sender,
            poisoned,
            worker,
        })
    }
    pub(crate) fn finished(&self) -> bool {
        self.worker.is_finished()
    }
    pub fn inspect(&self) -> Result<ButtonCoverage, ProbeError> {
        if self.poisoned.load(Ordering::Acquire) {
            return Err(ProbeError::UnexpectedStructure);
        }
        let identity = Identity::capture()?;
        self.inspect_identity(identity, Duration::from_millis(1200))
    }
    fn inspect_identity(
        &self,
        identity: Identity,
        timeout: Duration,
    ) -> Result<ButtonCoverage, ProbeError> {
        if self.poisoned.load(Ordering::Acquire) {
            return Err(ProbeError::UnexpectedStructure);
        }
        let (reply, response) = mpsc::sync_channel(1);
        self.sender
            .try_send(Request {
                identity: identity.clone(),
                reply,
            })
            .map_err(|_| ProbeError::UnexpectedStructure)?;
        match response.recv_timeout(timeout) {
            Ok(result) => {
                identity.verify()?;
                result
            }
            Err(_) => {
                self.poisoned.store(true, Ordering::Release);
                Err(ProbeError::UnexpectedStructure)
            }
        }
    }
}
fn query(identity: &Identity) -> Result<ButtonCoverage, ProbeError> {
    let _dpi = DpiGuard::enter()?;
    identity.verify()?;
    let query = || -> windows::core::Result<ButtonCoverage> {
        unsafe {
            let automation: IUIAutomation2 =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
            automation.SetConnectionTimeout(300)?;
            automation.SetTransactionTimeout(300)?;
            automation.SetAutoSetFocus(false)?;
            let automation: IUIAutomation = automation.cast()?;
            let cache = automation.CreateCacheRequest()?;
            cache.SetTreeScope(TreeScope_Element)?;
            cache.SetAutomationElementMode(AutomationElementMode_None)?;
            for property in [
                UIA_BoundingRectanglePropertyId,
                UIA_ProcessIdPropertyId,
                UIA_IsOffscreenPropertyId,
            ] {
                cache.AddProperty(property)?;
            }
            let condition = automation.CreateTrueCondition()?;
            cache.SetTreeFilter(&condition)?;
            let root = automation.ElementFromHandle(HWND(identity.list as _))?;
            let elements = root.FindAllBuildCache(TreeScope_Children, &condition, &cache)?;
            let count = elements.Length()?;
            if !(0..=256).contains(&count) {
                return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x80070057u32 as i32,
                )));
            }
            let mut occupied = vec![];
            for index in 0..count {
                let item = elements.GetElement(index)?;
                if item.CachedProcessId()? != identity.pid as i32 {
                    return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                        0x80070057u32 as i32,
                    )));
                }
                // Every direct raw-view item contributes, including overflow controls.
                // Hidden/off-screen children indicate incomplete visible button coverage.
                if item.CachedIsOffscreen()?.as_bool() {
                    return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                        0x80070057u32 as i32,
                    )));
                }
                let rect = item.CachedBoundingRectangle()?;
                occupied.push(ScreenRect {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                });
            }
            Ok(ButtonCoverage {
                list: identity.topology.task_list,
                occupied,
            })
        }
    };
    let result = query().map_err(|_| ProbeError::UnexpectedStructure)?;
    result.validate()?;
    identity.verify()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocked_provider_poison_rejects_more_work_and_cannot_be_replaced_until_finished() {
        let (sender, receiver) = mpsc::sync_channel::<Request>(1);
        let (release, gate) = mpsc::sync_channel(1);
        let (observed, observation) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let request = receiver.recv().unwrap();
            gate.recv().unwrap();
            assert!(
                receiver.try_recv().is_err(),
                "poisoned probe must not queue another request"
            );
            let _ = request.reply.send(Err(ProbeError::Os));
            observed.send(()).unwrap();
        });
        let probe = ButtonProbe {
            sender,
            worker,
            poisoned: Arc::new(AtomicBool::new(false)),
        };
        // Synthetic identity is never inspected: this verifies a blocked-provider boundary only.
        let identity = Identity {
            root: 0,
            list: 0,
            pid: 0,
            birth: [0; 2],
            topology: TaskbarTopology {
                build: 19045,
                dpi: 96,
                taskbar: ScreenRect {
                    left: 0,
                    top: 0,
                    right: 1600,
                    bottom: 40,
                },
                rebar: ScreenRect {
                    left: 0,
                    top: 0,
                    right: 1400,
                    bottom: 40,
                },
                task_switch: ScreenRect {
                    left: 0,
                    top: 0,
                    right: 1400,
                    bottom: 40,
                },
                task_list: ScreenRect {
                    left: 0,
                    top: 0,
                    right: 1400,
                    bottom: 40,
                },
                notification: ScreenRect {
                    left: 1400,
                    top: 0,
                    right: 1600,
                    bottom: 40,
                },
            },
        };
        assert_eq!(
            probe.inspect_identity(identity.clone(), Duration::from_millis(20)),
            Err(ProbeError::UnexpectedStructure)
        );
        assert!(probe.poisoned.load(Ordering::Acquire));
        assert!(!probe.finished());
        assert_eq!(
            probe.inspect_identity(identity, Duration::from_millis(20)),
            Err(ProbeError::UnexpectedStructure)
        );
        release.send(()).unwrap();
        observation.recv_timeout(Duration::from_secs(1)).unwrap();
        probe.worker.join().unwrap();
    }
    #[test]
    fn button_extent_is_complete_order_independent_and_rejects_hidden_area_or_overflow() {
        let list = ScreenRect {
            left: -1000,
            top: -48,
            right: -100,
            bottom: 0,
        };
        let mut coverage = ButtonCoverage {
            list,
            occupied: vec![
                ScreenRect {
                    right: -700,
                    ..list
                },
                ScreenRect {
                    left: -650,
                    right: -500,
                    ..list
                },
            ],
        };
        coverage.validate().unwrap();
        assert_eq!(coverage.rightmost(), -500);
        coverage.occupied.reverse();
        assert_eq!(coverage.rightmost(), -500);
        coverage.occupied.clear();
        assert_eq!(coverage.rightmost(), -1000);
        coverage.occupied = vec![ScreenRect {
            left: -1001,
            ..list
        }];
        assert!(coverage.validate().is_err());
        coverage.occupied = vec![list; 257];
        assert!(coverage.validate().is_err());
    }
}
