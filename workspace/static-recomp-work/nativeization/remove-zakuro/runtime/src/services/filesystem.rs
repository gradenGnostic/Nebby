//! Production rooted CTR filesystem ownership, with no process-shell types.
//! Donor: TriAevum RootedFilesystemBackendV1 via local licensed C ABI bridge.
use std::ffi::{CString, c_char, c_void};
pub type Open =
    unsafe extern "C" fn(*mut c_void, u32, *const c_char, u32, *mut u64, *mut u64) -> i32;
pub type Read = unsafe extern "C" fn(*mut c_void, u64, u64, *mut u8, u32, *mut u32) -> i32;
pub type Write = unsafe extern "C" fn(*mut c_void, u64, u64, *const u8, u32, *mut u32) -> i32;
pub type HandleOp = unsafe extern "C" fn(*mut c_void, u64) -> i32;
pub type PathOp = unsafe extern "C" fn(*mut c_void, *const c_char) -> i32;
pub type EntryCallback = unsafe extern "C" fn(*mut c_void, *const c_char, u32, u64);
/// Transitional raw ABI access; calls require the live owner's exclusive
/// execution context and valid C strings/buffers. Prefer checked methods below.
pub struct FsAbi {
    pub context: *mut c_void,
    pub open: Open,
    pub read: Read,
    pub write: Write,
    pub resize: unsafe extern "C" fn(*mut c_void, u64, u64) -> i32,
    pub close: HandleOp,
    pub flush: HandleOp,
    pub commit: unsafe extern "C" fn(*mut c_void) -> i32,
    pub format: PathOp,
    pub mkdir: PathOp,
    pub remove: PathOp,
    pub rename: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> i32,
    pub list: unsafe extern "C" fn(*mut c_void, *const c_char, EntryCallback, *mut c_void) -> i32,
}
#[link(name = "dl")]
unsafe extern "C" {
    fn dlopen(path: *const c_char, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> i32;
}
pub struct FsRuntime {
    library: *mut c_void,
    abi: FsAbi,
    destroy: unsafe extern "C" fn(*mut c_void),
}
// Owned exclusively by the cooperative native process. The donor also locks
// its storage state; this is not a shared reference to mutable process data.
unsafe impl Send for FsRuntime {}
impl std::ops::Deref for FsRuntime {
    type Target = FsAbi;
    fn deref(&self) -> &FsAbi {
        &self.abi
    }
}
impl FsRuntime {
    pub fn new(library_path: &str, save_root: &str, content_image: &str) -> Result<Self, String> {
        let path = CString::new(library_path).map_err(|_| "invalid filesystem library path")?;
        let root = CString::new(save_root).map_err(|_| "invalid filesystem root")?;
        let image = CString::new(content_image).map_err(|_| "invalid content path")?;
        let library = unsafe { dlopen(path.as_ptr(), 2) };
        if library.is_null() {
            return Err("native filesystem library could not load".into());
        }
        macro_rules! symbol {
            ($name:literal,$ty:ty) => {{
                let pointer = unsafe { dlsym(library, concat!($name, "\0").as_ptr().cast()) };
                if pointer.is_null() {
                    unsafe {
                        dlclose(library);
                    }
                    return Err(concat!("missing ", $name).into());
                }
                unsafe { std::mem::transmute::<*mut c_void, $ty>(pointer) }
            }};
        }
        let create = symbol!(
            "ctr_native_fs_create",
            unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void
        );
        let destroy = symbol!("ctr_native_fs_destroy", unsafe extern "C" fn(*mut c_void));
        let mut abi = FsAbi {
            context: std::ptr::null_mut(),
            open: symbol!("ctr_native_fs_open", Open),
            read: symbol!("ctr_native_fs_read", Read),
            write: symbol!("ctr_native_fs_write", Write),
            resize: symbol!(
                "ctr_native_fs_resize",
                unsafe extern "C" fn(*mut c_void, u64, u64) -> i32
            ),
            close: symbol!("ctr_native_fs_close", HandleOp),
            flush: symbol!("ctr_native_fs_flush", HandleOp),
            commit: symbol!(
                "ctr_native_fs_commit",
                unsafe extern "C" fn(*mut c_void) -> i32
            ),
            format: symbol!("ctr_native_fs_format", PathOp),
            mkdir: symbol!("ctr_native_fs_mkdir", PathOp),
            remove: symbol!("ctr_native_fs_remove", PathOp),
            rename: symbol!(
                "ctr_native_fs_rename",
                unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> i32
            ),
            list: symbol!(
                "ctr_native_fs_list",
                unsafe extern "C" fn(*mut c_void, *const c_char, EntryCallback, *mut c_void) -> i32
            ),
        };
        abi.context = unsafe { create(root.as_ptr(), image.as_ptr()) };
        if abi.context.is_null() {
            unsafe {
                dlclose(library);
            }
            return Err("native filesystem initialization failed".into());
        }
        Ok(Self {
            library,
            abi,
            destroy,
        })
    }
    pub fn open_file(&self, root: u32, path: &str, flags: u32) -> Result<(u64, u64), i32> {
        let path = CString::new(path).map_err(|_| 1)?;
        let (mut handle, mut size) = (0, 0);
        let result = unsafe {
            (self.abi.open)(
                self.abi.context,
                root,
                path.as_ptr(),
                flags,
                &mut handle,
                &mut size,
            )
        };
        if result == 0 {
            Ok((handle, size))
        } else {
            Err(result)
        }
    }
    pub fn read_file(&self, handle: u64, offset: u64, bytes: &mut [u8]) -> Result<usize, i32> {
        let size = u32::try_from(bytes.len()).map_err(|_| 1)?;
        let mut count = 0;
        let result = unsafe {
            (self.abi.read)(
                self.abi.context,
                handle,
                offset,
                bytes.as_mut_ptr(),
                size,
                &mut count,
            )
        };
        if result != 0 {
            return Err(result);
        }
        if count > size {
            return Err(-1);
        }
        Ok(count as usize)
    }
    pub fn write_file(&self, handle: u64, offset: u64, bytes: &[u8]) -> Result<usize, i32> {
        let size = u32::try_from(bytes.len()).map_err(|_| 1)?;
        let mut count = 0;
        let result = unsafe {
            (self.abi.write)(
                self.abi.context,
                handle,
                offset,
                bytes.as_ptr(),
                size,
                &mut count,
            )
        };
        if result != 0 {
            return Err(result);
        }
        if count > size {
            return Err(-1);
        }
        Ok(count as usize)
    }
    pub fn close_file(&self, handle: u64) -> Result<(), i32> {
        let result = unsafe { (self.abi.close)(self.abi.context, handle) };
        if result == 0 { Ok(()) } else { Err(result) }
    }
}
impl Drop for FsRuntime {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.abi.context);
            dlclose(self.library);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_library() {
        assert!(FsRuntime::new("/missing/native-fs.so", "/tmp", "/missing/image").is_err());
    }
    #[test]
    fn path_nul_rejected() {
        assert!(FsRuntime::new("bad\0path", "/tmp", "/missing/image").is_err());
    }
}
