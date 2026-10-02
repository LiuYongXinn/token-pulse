use std::{mem, ptr};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, LocalFree},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        },
        GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct LocalMemory(*mut std::ffi::c_void);
impl Drop for LocalMemory {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
pub(super) fn user_sid() -> std::io::Result<String> {
    unsafe {
        let mut raw = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let token = Handle(raw);
        let mut length = 0;
        GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut length);
        if length == 0 || length > 65_536 {
            return Err(std::io::Error::other("notify_invalid_user_token"));
        }
        let mut buffer = vec![0usize; (length as usize).div_ceil(mem::size_of::<usize>())];
        if GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        ) == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut string = ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut string) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let _memory = LocalMemory(string.cast());
        let mut count = 0;
        while *string.add(count) != 0 {
            count += 1;
        }
        String::from_utf16(std::slice::from_raw_parts(string, count))
            .map_err(|_| std::io::Error::other("notify_invalid_user_sid"))
    }
}
pub(super) fn create_server(name: &str, first: bool) -> std::io::Result<NamedPipeServer> {
    let sddl: Vec<u16> = format!("D:P(A;;GA;;;{})", user_sid()?)
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        let mut descriptor = ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        ) == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let _memory = LocalMemory(descriptor);
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        ServerOptions::new()
            .first_pipe_instance(first)
            // Cancelled IO / disconnected peers can temporarily retain old kernel instances.
            // Bound these slots as well as the two live server handles.
            .max_instances(8)
            .reject_remote_clients(true)
            .in_buffer_size(2048)
            .out_buffer_size(512)
            .create_with_security_attributes_raw(
                name,
                (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACL_SIZE_INFORMATION, AclSizeInformation,
        Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT},
        DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, GetSecurityDescriptorControl,
        SE_DACL_PROTECTED,
    };
    #[tokio::test(flavor = "current_thread")]
    async fn real_notify_pipe_has_only_current_user_protected_allow_ace() {
        let cap = crate::notify_channel::NotifyCapability::new();
        let pipe = create_server(&super::super::channel_name(&cap).unwrap(), true).unwrap();
        assert_eq!(pipe.info().unwrap().max_instances, 8);
        unsafe {
            let mut acl = ptr::null_mut();
            let mut descriptor = ptr::null_mut();
            assert_eq!(
                GetSecurityInfo(
                    pipe.as_raw_handle().cast(),
                    SE_KERNEL_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    &mut acl,
                    ptr::null_mut(),
                    &mut descriptor
                ),
                0
            );
            let _memory = LocalMemory(descriptor);
            assert!(!acl.is_null());
            let mut control = 0;
            let mut revision = 0;
            assert_ne!(
                GetSecurityDescriptorControl(descriptor, &mut control, &mut revision),
                0
            );
            assert_ne!(control & SE_DACL_PROTECTED, 0);
            let mut info: ACL_SIZE_INFORMATION = mem::zeroed();
            assert_ne!(
                GetAclInformation(
                    acl,
                    (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
                    mem::size_of_val(&info) as u32,
                    AclSizeInformation
                ),
                0
            );
            assert_eq!(info.AceCount, 1);
            let mut raw = ptr::null_mut();
            assert_ne!(GetAce(acl, 0, &mut raw), 0);
            let ace = &*raw.cast::<ACCESS_ALLOWED_ACE>();
            assert_eq!(ace.Header.AceType, 0);
            let mut string = ptr::null_mut();
            assert_ne!(
                ConvertSidToStringSidW(
                    (&ace.SidStart as *const u32).cast_mut().cast(),
                    &mut string
                ),
                0
            );
            let _sid_memory = LocalMemory(string.cast());
            let mut count = 0;
            while *string.add(count) != 0 {
                count += 1;
            }
            let actual = String::from_utf16(std::slice::from_raw_parts(string, count)).unwrap();
            assert!(actual == user_sid().unwrap());
        }
    }
}
