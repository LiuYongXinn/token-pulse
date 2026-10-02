//! Bounded window-generation metadata. Values encode integers, never cross-process pointers.
use super::topology::{ProbeError, ScreenRect, birth, wide};
use windows_sys::Win32::{
    Foundation::{HANDLE, HWND},
    System::Threading::{GetCurrentProcess, GetCurrentProcessId},
    UI::WindowsAndMessaging::{GetPropW, RemovePropW, SetPropW},
};
const OWNER: &str = "TokenPulse.Taskbar.Layout.v1.owner";
const WORDS: usize = 17;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProcessIdentity {
    pub(crate) pid: u32,
    pub(crate) birth: [u32; 2],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LayoutRecord {
    pub(crate) original: ScreenRect,
    pub(crate) expected: ScreenRect,
    pub(crate) parent_size: (i32, i32),
    pub(crate) dpi: u32,
    pub(crate) host: ProcessIdentity,
    pub(crate) shell: ProcessIdentity,
}
impl LayoutRecord {
    fn words(self) -> [u32; WORDS] {
        [
            self.original.left as u32,
            self.original.top as u32,
            self.original.right as u32,
            self.original.bottom as u32,
            self.expected.left as u32,
            self.expected.top as u32,
            self.expected.right as u32,
            self.expected.bottom as u32,
            self.parent_size.0 as u32,
            self.parent_size.1 as u32,
            self.dpi,
            self.host.pid,
            self.host.birth[0],
            self.host.birth[1],
            self.shell.pid,
            self.shell.birth[0],
            self.shell.birth[1],
        ]
    }
    fn from_words(w: [u32; WORDS]) -> Result<Self, ProbeError> {
        let record = Self {
            original: ScreenRect {
                left: w[0] as i32,
                top: w[1] as i32,
                right: w[2] as i32,
                bottom: w[3] as i32,
            },
            expected: ScreenRect {
                left: w[4] as i32,
                top: w[5] as i32,
                right: w[6] as i32,
                bottom: w[7] as i32,
            },
            parent_size: (w[8] as i32, w[9] as i32),
            dpi: w[10],
            host: ProcessIdentity {
                pid: w[11],
                birth: [w[12], w[13]],
            },
            shell: ProcessIdentity {
                pid: w[14],
                birth: [w[15], w[16]],
            },
        };
        let (original, expected) = (record.original, record.expected);
        if !(96..=768).contains(&record.dpi)
            || !original.valid()
            || !expected.valid()
            || record.host.pid == 0
            || record.shell.pid == 0
            || original.left < 0
            || original.top < 0
            || original.right > record.parent_size.0
            || original.bottom > record.parent_size.1
            || !(1..=1_000_000).contains(&record.parent_size.0)
            || !(1..=1_000_000).contains(&record.parent_size.1)
            || expected.left != original.left
            || expected.top != original.top
            || expected.bottom != original.bottom
            || expected.right >= original.right
            || expected.width() < (320 * record.dpi / 96) as i32
        {
            return Err(ProbeError::UnsafeGeometry);
        }
        Ok(record)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Prepared = 1,
    Reserved = 2,
}
pub(crate) struct Ownership {
    namespace: String,
    marker: usize,
    instance_words: [u32; 4],
    pub(crate) record: LayoutRecord,
    pub(crate) phase: Phase,
}
fn namespace(instance: &str) -> Result<(String, usize, [u32; 4]), ProbeError> {
    if instance.len() != 32 || !instance.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(ProbeError::UnexpectedStructure);
    }
    let instance = instance.to_ascii_lowercase();
    let marker = u32::from_str_radix(&instance[8..16], 16)
        .map_err(|_| ProbeError::UnexpectedStructure)?
        .max(1) as usize;
    let mut words = [0; 4];
    for (i, word) in words.iter_mut().enumerate() {
        *word = u32::from_str_radix(&instance[i * 8..i * 8 + 8], 16)
            .map_err(|_| ProbeError::UnexpectedStructure)?;
    }
    Ok((
        format!("TokenPulse.Taskbar.Layout.v1.{instance}"),
        marker,
        words,
    ))
}
fn instance_key(index: usize, half: usize) -> Vec<u16> {
    wide(&format!("{OWNER}.instance.{index}.{half}"))
}
fn read_instance(window: HWND) -> Option<[u32; 4]> {
    let mut words = [0; 4];
    for (i, word) in words.iter_mut().enumerate() {
        let low = get(window, &instance_key(i, 0));
        let high = get(window, &instance_key(i, 1));
        if !(1..=65536).contains(&low) || !(1..=65536).contains(&high) {
            return None;
        }
        *word = ((low - 1) as u32) | (((high - 1) as u32) << 16);
    }
    Some(words)
}
fn remove_instance(window: HWND) -> bool {
    let mut complete = true;
    for i in 0..4 {
        for half in 0..2 {
            complete &= !unsafe { RemovePropW(window, instance_key(i, half).as_ptr()) }.is_null();
        }
    }
    complete
}
fn key(namespace: &str, index: usize, half: usize) -> Vec<u16> {
    wide(&format!("{namespace}.{index}.{half}"))
}
fn get(window: HWND, key: &[u16]) -> usize {
    (unsafe { GetPropW(window, key.as_ptr()) }) as usize
}
fn set(window: HWND, key: &[u16], value: usize) -> Result<(), ProbeError> {
    if unsafe { SetPropW(window, key.as_ptr(), value as HANDLE) } == 0 {
        Err(ProbeError::Os)
    } else {
        Ok(())
    }
}
fn remove_namespace(window: HWND, namespace: &str) -> bool {
    let mut complete = true;
    for i in 0..WORDS {
        for half in 0..2 {
            unsafe {
                complete &= !RemovePropW(window, key(namespace, i, half).as_ptr()).is_null();
            }
        }
    }
    unsafe {
        complete &= !RemovePropW(window, wide(&format!("{namespace}.phase")).as_ptr()).is_null();
    }
    complete
}
fn read_record(window: HWND, namespace: &str) -> Result<LayoutRecord, ProbeError> {
    let mut words = [0; WORDS];
    for (i, word) in words.iter_mut().enumerate() {
        let low = get(window, &key(namespace, i, 0));
        let high = get(window, &key(namespace, i, 1));
        if !(1..=65536).contains(&low) || !(1..=65536).contains(&high) {
            return Err(ProbeError::UnexpectedStructure);
        }
        *word = ((low - 1) as u32) | (((high - 1) as u32) << 16);
    }
    LayoutRecord::from_words(words)
}
impl Ownership {
    pub(crate) fn current_host() -> Result<ProcessIdentity, ProbeError> {
        Ok(ProcessIdentity {
            pid: unsafe { GetCurrentProcessId() },
            birth: birth(unsafe { GetCurrentProcess() })?,
        })
    }
    /// Caller holds the layout mutex and has validated the switch window generation.
    pub(crate) fn publish(
        window: HWND,
        instance: &str,
        record: LayoutRecord,
    ) -> Result<Self, ProbeError> {
        LayoutRecord::from_words(record.words())?;
        let (namespace, marker, instance_words) = namespace(instance)?;
        if get(window, &wide(OWNER)) != 0 {
            return Err(ProbeError::UnexpectedStructure);
        }
        for i in 0..WORDS {
            for half in 0..2 {
                if get(window, &key(&namespace, i, half)) != 0 {
                    return Err(ProbeError::UnexpectedStructure);
                }
            }
        }
        if get(window, &wide(&format!("{namespace}.phase"))) != 0 {
            return Err(ProbeError::UnexpectedStructure);
        }
        // Under the exclusive mutex and a full-width validated baseline, an unpublished header
        // left before owner publication cannot represent an active reservation.
        remove_instance(window);
        let result = (|| {
            for (i, word) in record.words().into_iter().enumerate() {
                set(window, &key(&namespace, i, 0), (word & 65535) as usize + 1)?;
                set(window, &key(&namespace, i, 1), (word >> 16) as usize + 1)?;
            }
            for (i, word) in instance_words.into_iter().enumerate() {
                set(window, &instance_key(i, 0), (word & 65535) as usize + 1)?;
                set(window, &instance_key(i, 1), (word >> 16) as usize + 1)?;
            }
            set(
                window,
                &wide(&format!("{namespace}.phase")),
                Phase::Prepared as usize,
            )?;
            // Publish owner last: an incomplete record cannot authorize layout mutation.
            set(window, &wide(OWNER), marker)
        })();
        if let Err(error) = result {
            remove_namespace(window, &namespace);
            remove_instance(window);
            return Err(error);
        }
        Ok(Self {
            namespace,
            marker,
            instance_words,
            record,
            phase: Phase::Prepared,
        })
    }
    pub(crate) fn load(window: HWND, instance: &str) -> Result<Option<Self>, ProbeError> {
        let (namespace, marker, instance_words) = namespace(instance)?;
        let owner = get(window, &wide(OWNER));
        if owner == 0 {
            // Known private instance namespace only. No owner and no mutation was published.
            remove_namespace(window, &namespace);
            if read_instance(window) == Some(instance_words) {
                remove_instance(window);
            }
            return Ok(None);
        }
        if owner != marker || read_instance(window) != Some(instance_words) {
            return Err(ProbeError::UnexpectedStructure);
        }
        let record = read_record(window, &namespace)?;
        let phase = match get(window, &wide(&format!("{namespace}.phase"))) {
            1 => Phase::Prepared,
            2 => Phase::Reserved,
            _ => return Err(ProbeError::UnexpectedStructure),
        };
        Ok(Some(Self {
            namespace,
            marker,
            instance_words,
            record,
            phase,
        }))
    }
    pub(crate) fn owns(&self, window: HWND) -> bool {
        get(window, &wide(OWNER)) == self.marker
            && read_instance(window) == Some(self.instance_words)
            && read_record(window, &self.namespace).ok() == Some(self.record)
            && get(window, &wide(&format!("{}.phase", self.namespace))) == self.phase as usize
    }
    pub(crate) fn reserved(&mut self, window: HWND) -> Result<(), ProbeError> {
        if !self.owns(window) {
            return Err(ProbeError::UnexpectedStructure);
        }
        set(
            window,
            &wide(&format!("{}.phase", self.namespace)),
            Phase::Reserved as usize,
        )?;
        self.phase = Phase::Reserved;
        Ok(())
    }
    pub(crate) fn remove(&self, window: HWND) -> Result<(), ProbeError> {
        if !self.owns(window) {
            return Err(ProbeError::UnexpectedStructure);
        }
        if unsafe { RemovePropW(window, wide(OWNER).as_ptr()) } as usize != self.marker {
            return Err(ProbeError::Os);
        }
        let record_removed = remove_namespace(window, &self.namespace);
        let instance_removed = remove_instance(window);
        if record_removed && instance_removed {
            Ok(())
        } else {
            Err(ProbeError::Os)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WS_POPUP};
    fn record() -> LayoutRecord {
        LayoutRecord {
            original: ScreenRect {
                left: 2,
                top: 0,
                right: 1000,
                bottom: 60,
            },
            expected: ScreenRect {
                left: 2,
                top: 0,
                right: 600,
                bottom: 60,
            },
            parent_size: (1000, 60),
            dpi: 144,
            host: ProcessIdentity {
                pid: 700,
                birth: [0, u32::MAX],
            },
            shell: ProcessIdentity {
                pid: 800,
                birth: [u32::MAX, 0],
            },
        }
    }
    #[test]
    fn native_window_properties_bind_complete_record_and_cannot_remove_another_owner() {
        let window = unsafe {
            CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("OWN TEST ONLY").as_ptr(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        assert!(!window.is_null());
        let instance = "00112233445566778899aabbccddeeff";
        let mut owner = Ownership::publish(window, instance, record()).unwrap();
        assert!(Ownership::publish(window, instance, record()).is_err());
        let loaded = Ownership::load(window, instance).unwrap().unwrap();
        assert_eq!(loaded.record, record());
        assert_eq!(loaded.phase, Phase::Prepared);
        owner.reserved(window).unwrap();
        assert_eq!(
            Ownership::load(window, instance).unwrap().unwrap().phase,
            Phase::Reserved
        );
        assert!(Ownership::load(window, "ff112233445566778899aabbccddeeff").is_err()); // Same marker, different namespace.
        let phase_key = wide(&format!("{}.phase", owner.namespace));
        set(window, &phase_key, 99).unwrap();
        assert!(owner.remove(window).is_err());
        assert_ne!(get(window, &wide(OWNER)), 0);
        assert!(Ownership::load(window, instance).is_err());
        set(window, &phase_key, Phase::Reserved as usize).unwrap();
        let missing = key(&owner.namespace, 0, 0);
        unsafe {
            RemovePropW(window, missing.as_ptr());
        }
        assert!(Ownership::load(window, instance).is_err());
        assert!(owner.remove(window).is_err());
        assert_ne!(get(window, &wide(OWNER)), 0);
        set(
            window,
            &missing,
            (record().original.left as u32 & 65535) as usize + 1,
        )
        .unwrap();
        owner.remove(window).unwrap();
        assert!(Ownership::load(window, instance).unwrap().is_none());
        assert!(read_instance(window).is_none());
        for i in 0..WORDS {
            for half in 0..2 {
                assert_eq!(get(window, &key(&owner.namespace, i, half)), 0);
            }
        }
        unsafe {
            DestroyWindow(window);
        }
    }
    #[test]
    fn metadata_rejects_unbounded_or_unsafe_restoration_geometry() {
        let mut words = record().words();
        assert_eq!(LayoutRecord::from_words(words).unwrap(), record());
        words[2] = 1001;
        assert_eq!(
            LayoutRecord::from_words(words),
            Err(ProbeError::UnsafeGeometry)
        );
        words = record().words();
        words[6] = 480; // Less than minimum remaining tasks.
        assert_eq!(
            LayoutRecord::from_words(words),
            Err(ProbeError::UnsafeGeometry)
        );
        words = record().words();
        words[0] = u32::MAX;
        assert_eq!(
            LayoutRecord::from_words(words),
            Err(ProbeError::UnsafeGeometry)
        );
        assert!(namespace("0011").is_err());
    }
}
