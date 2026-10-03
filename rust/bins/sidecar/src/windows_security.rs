//! Windows local IPC/state ACLs. Creation is private before any data is written.
use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, DACL_SECURITY_INFORMATION, GetAce,
    GetAclInformation, GetTokenInformation, IsValidAcl, OWNER_SECURITY_INFORMATION, SECURITY_ATTRIBUTES, TOKEN_QUERY,
    TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
};
use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

struct LocalAllocation(*mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: these pointers are allocated by Win32 conversion/security APIs
        // with LocalAlloc, and this wrapper owns their single release.
        unsafe { LocalFree(self.0) };
    }
}

fn sid_string(sid: *mut c_void) -> io::Result<String> {
    let mut output = ptr::null_mut();
    // SAFETY: callers supply a SID owned by a live token/security descriptor.
    if unsafe { ConvertSidToStringSidW(sid, &mut output) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let allocation = LocalAllocation(output.cast());
    let mut len = 0;
    // SAFETY: the successful API returns a NUL-terminated UTF-16 string.
    unsafe {
        while *output.add(len) != 0 {
            len += 1;
        }
        let result = String::from_utf16(std::slice::from_raw_parts(output, len)).map_err(io::Error::other);
        drop(allocation);
        result
    }
}

fn current_user_sid() -> io::Result<String> {
    let mut token = ptr::null_mut();
    // SAFETY: GetCurrentProcess is a valid pseudo-handle and token is writable.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let result = (|| {
        let mut bytes = 0;
        // SAFETY: the null buffer size query fills bytes, without writing data.
        unsafe { GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut bytes) };
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        // TOKEN_USER contains pointers: use word-aligned storage, not Vec<u8>.
        let mut storage = vec![0usize; (bytes as usize).div_ceil(std::mem::size_of::<usize>())];
        // SAFETY: storage is aligned and holds at least bytes; token is live.
        if unsafe { GetTokenInformation(token, TokenUser, storage.as_mut_ptr().cast(), bytes, &mut bytes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful TokenUser query initialized TOKEN_USER and its SID.
        let user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
        sid_string(user.User.Sid)
    })();
    // SAFETY: OpenProcessToken returned an owned handle; release on all paths.
    unsafe { CloseHandle(token) };
    result
}

pub struct PrivateSecurityDescriptor(LocalAllocation);

impl PrivateSecurityDescriptor {
    pub fn new() -> io::Result<Self> {
        let user = current_user_sid()?;
        // Explicit owner avoids an elevated token's default Administrators owner.
        // Inherit to state children, and never inherit a parent's broader DACL.
        let sddl = format!("O:{user}D:P(A;OICI;FA;;;{user})(A;OICI;FA;;;SY)");
        let wide = sddl.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let mut descriptor = ptr::null_mut();
        // SAFETY: wide is NUL-terminated, revision 1 is SDDL_REVISION_1, and the
        // returned descriptor is owned until the creation call has finished.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(wide.as_ptr(), 1, &mut descriptor, ptr::null_mut())
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(LocalAllocation(descriptor)))
    }

    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.0,
            bInheritHandle: 0,
        }
    }
}

/// Fail closed if an existing pipe/state root admits another account.
pub fn verify_private_handle(handle: HANDLE) -> io::Result<()> {
    let mut owner = ptr::null_mut();
    let mut dacl: *mut ACL = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    // SAFETY: handle is borrowed and live; outputs are writable and the returned
    // descriptor owns the owner/ACL memory until it is freed below.
    let error = unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut dacl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if error != 0 {
        return Err(io::Error::from_raw_os_error(error as i32));
    }
    if descriptor.is_null() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "missing local state/relay descriptor"));
    }
    let _allocation = LocalAllocation(descriptor);
    let user = current_user_sid()?;
    if owner.is_null() || sid_string(owner)? != user || dacl.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "local state/relay must be owned by the current user with a private DACL",
        ));
    }
    // SAFETY: dacl is non-null and borrows the live descriptor. Check the ACL
    // through Win32 before reading its size information or retrieving any ACE.
    if unsafe { IsValidAcl(dacl) } == 0 {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "invalid local state/relay DACL"));
    }
    let mut info = ACL_SIZE_INFORMATION { AceCount: 0, AclBytesInUse: 0, AclBytesFree: 0 };
    // SAFETY: the valid ACL and initialized output buffer are live for this call.
    if unsafe {
        GetAclInformation(
            dacl,
            ptr::from_mut(&mut info).cast(),
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let ace_count = info.AceCount;
    if ace_count == 0 {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "local state/relay DACL is empty"));
    }
    for index in 0..ace_count {
        let mut ace = ptr::null_mut();
        // SAFETY: dacl is live and index is within its declared ACE count.
        if unsafe { GetAce(dacl, index, &mut ace) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let header = ptr::NonNull::new(ace.cast::<ACE_HEADER>())
            .ok_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, "missing local state/relay ACE"))?;
        // Only the explicit allow ACE shape used for this private boundary is
        // accepted. Unknown/callback/object ACEs are rejected, never guessed.
        // SAFETY: successful GetAce on the valid, live ACL returns an ACE header;
        // the output was checked non-null. Inspect the type before casting a SID.
        if unsafe { header.as_ref().AceType } != ACCESS_ALLOWED_ACE_TYPE as u8 {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "unsupported local state/relay ACE"));
        }
        let allowed = ace.cast::<ACCESS_ALLOWED_ACE>();
        // SAFETY: ACCESS_ALLOWED_ACE's SID begins at SidStart and the ACL API
        // guarantees that a successfully retrieved ACE contains its full SID.
        let sid = sid_string(unsafe { ptr::addr_of_mut!((*allowed).SidStart).cast() })?;
        if sid != user && sid != "S-1-5-18" {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "local state/relay is accessible to another account",
            ));
        }
    }
    Ok(())
}

pub fn ensure_private_state_root(path: &Path) -> io::Result<()> {
    let parent =
        path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "state root must have a parent"))?;
    std::fs::create_dir_all(parent)?;
    let descriptor = PrivateSecurityDescriptor::new()?;
    let attributes = descriptor.attributes();
    let wide = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    // SAFETY: path and attributes are live and the descriptor owns its storage.
    if unsafe { CreateDirectoryW(wide.as_ptr(), &attributes) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32) {
            return Err(error);
        }
    }
    if !std::fs::symlink_metadata(path)?.is_dir() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "state root must be a private directory"));
    }
    // ponytail: bounded startup walk; archive completed state if a single agent
    // exceeds 100,000 entries rather than doing an unbounded security scan.
    let mut pending = vec![path.to_path_buf()];
    let mut checked = 0;
    while let Some(path) = pending.pop() {
        checked += 1;
        if checked > 100_000 {
            return Err(io::Error::other(
                "state tree exceeds 100,000 entries; archive completed state before starting",
            ));
        }
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !(metadata.is_dir() || metadata.is_file())
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "state must contain private files/directories, not reparse points",
            ));
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)?;
        verify_private_handle(file.as_raw_handle())?;
        if metadata.is_dir() {
            for child in std::fs::read_dir(path)? {
                if checked + pending.len() >= 100_000 {
                    return Err(io::Error::other(
                        "state tree exceeds 100,000 entries; archive completed state before starting",
                    ));
                }
                pending.push(child?.path());
            }
        }
    }
    Ok(())
}

/// Create a state file with an explicit current-user owner, including elevated tokens.
pub fn create_private_file(path: &Path) -> io::Result<std::fs::File> {
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_DELETE, FILE_SHARE_READ,
    };
    let descriptor = PrivateSecurityDescriptor::new()?;
    let attributes = descriptor.attributes();
    let wide = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    // SAFETY: path/descriptor/attributes live through creation; ownership of the
    // successful Win32 file handle transfers exactly once to std::fs::File.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_DELETE,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { std::fs::File::from_raw_handle(handle) })
}

/// Existing state is validated once at startup; every newly created descendant
/// receives the same explicit owner and private inherited ACL as the root.
pub fn create_private_dirs(path: &Path) -> io::Result<()> {
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        match std::fs::symlink_metadata(current) {
            Ok(metadata) => {
                if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "state directory cannot be a reparse point",
                    ));
                }
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(current.to_path_buf());
                current = current
                    .parent()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "state directory must have a parent"))?;
            }
            Err(error) => return Err(error),
        }
    }
    let descriptor = PrivateSecurityDescriptor::new()?;
    let attributes = descriptor.attributes();
    for directory in missing.into_iter().rev() {
        let wide = directory.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        // SAFETY: directory/attributes/descriptor remain live through creation.
        if unsafe { CreateDirectoryW(wide.as_ptr(), &attributes) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32) {
                return Err(error);
            }
            let metadata = std::fs::symlink_metadata(&directory)?;
            if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "state directory cannot be a reparse point",
                ));
            }
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&directory)?;
            verify_private_handle(file.as_raw_handle())?;
        }
    }
    Ok(())
}
