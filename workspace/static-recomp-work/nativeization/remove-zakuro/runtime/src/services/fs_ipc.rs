//! Title-neutral CTR IPC adapter backed by TriAevum's native filesystem.
//! No HostArchive or Zakuro FS dispatch is used in strict native mode.
// Source provenance: local native_fs.rs, narrow native IPC extraction.
use super::router::ServiceMemory;
use crate::{
    execution::context::ThreadContext,
    ipc::{
        Target,
        message::{CommandBuffer, Header, IpcMemory},
    },
    kernel::{ClientSession, KObject},
    scheduler::NativeScheduler,
};
use std::path::Path;
pub trait FsMemory: IpcMemory + ServiceMemory {
    fn query(&self, address: u32) -> crate::process::svc::MemoryInfo;
    fn read_bytes(&mut self, address: u32, bytes: &mut [u8]) {
        assert!(
            self.read_span(address, bytes),
            "invalid native FS read span"
        );
    }
    fn write_bytes(&mut self, address: u32, bytes: &[u8]) {
        assert!(
            self.write_span(address, bytes),
            "invalid native FS write span"
        );
    }
}
pub struct Context<'a, M: FsMemory, R> {
    pub memory: &'a mut M,
    pub kernel: &'a mut NativeScheduler<ThreadContext, R>,
    pub fs: &'a mut NativeFsState,
    pub image_path: &'a Path,
    pub romfs_range: Option<(u64, u64)>,
    pub tick: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResultCode(u32);
mod errors {
    use super::ResultCode;
    pub const FS_NOT_FOUND: ResultCode = ResultCode(0xc8804478);
    pub const FS_ARCHIVE_NOT_MOUNTED: ResultCode = ResultCode(0xc8a0445a);
    pub const FS_NOT_FOUND_INVALID_STATE: ResultCode = ResultCode(0xc8a04478);
    pub const FS_NOT_FORMATTED: ResultCode = ResultCode(0xc8a04554);
    pub const FS_UNEXPECTED_FILE_OR_DIRECTORY: ResultCode = ResultCode(0xe0c04702);
    pub const FS_INVALID_PATH: ResultCode = ResultCode(0xe0e046be);
}
fn nanos_to_ticks(nanos: u64) -> u64 {
    (nanos / 1_000_000_000) * crate::scheduler::CPU_CLOCK_HZ
        + (nanos % 1_000_000_000) * crate::scheduler::CPU_CLOCK_HZ / 1_000_000_000
}
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};

use super::filesystem::{FsAbi, FsRuntime};
struct Backend {
    native: FsRuntime,
}
impl std::ops::Deref for Backend {
    type Target = FsAbi;
    fn deref(&self) -> &FsAbi {
        &self.native
    }
}
impl Backend {
    fn new(system: &Context<'_, impl FsMemory, impl Sized>) -> Result<Self, String> {
        let library_path = std::env::var("POKEMOON_NATIVE_FS_LIB")
            .map_err(|_| "POKEMOON_NATIVE_FS_LIB missing")?;
        let root = std::env::var("POKEMOON_NATIVE_FS_ROOT")
            .map_err(|_| "POKEMOON_NATIVE_FS_ROOT missing")?;
        let image = system.image_path.to_string_lossy();
        let native = FsRuntime::new(&library_path, &root, &image)?;
        log::info!("NATIVE_FS_CREATED root={} ZAKURO_FS_BYPASS=1", root);
        Ok(Self { native })
    }
    fn open(&self, root: u32, path: &str, flags: u32) -> Result<(u64, u64), ResultCode> {
        match self.native.open_file(root, path, flags) {
            Ok(file) => Ok(file),
            Err(code) => {
                status(code)?;
                unreachable!()
            }
        }
    }
}
fn status(code: i32) -> Result<(), ResultCode> {
    match code {
        0 => Ok(()),
        1 => Err(errors::FS_INVALID_PATH),
        4 => Err(errors::FS_NOT_FOUND),
        _ => Err(errors::FS_UNEXPECTED_FILE_OR_DIRECTORY),
    }
}
#[derive(Clone)]
struct Archive {
    id: u32,
    prefix: String,
}
#[derive(Clone)]
struct File {
    native: u64,
    base: u64,
    size: u64,
    writable: bool,
}
struct DirectoryEntry {
    name: String,
    directory: bool,
    size: u64,
}
struct Directory {
    entries: Vec<DirectoryEntry>,
    cursor: usize,
}
unsafe extern "C" fn collect_entry(
    user: *mut c_void,
    name: *const c_char,
    directory: u32,
    size: u64,
) {
    // Callback is invoked synchronously by the trusted bridge with a live
    // UTF-8 filename and the caller's exclusively borrowed entries vector.
    let entries = unsafe { &mut *user.cast::<Vec<DirectoryEntry>>() };
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    if name == ".format" {
        return;
    }
    entries.push(DirectoryEntry {
        name: name.into_owned(),
        directory: directory != 0,
        size,
    });
}
#[derive(Default)]
pub struct NativeFsState {
    backend: Option<Backend>,
    archives: HashMap<u64, Archive>,
    files: HashMap<u32, File>,
    next: u64,
    calls: u64,
    reads: u64,
    writes: u64,
    commits: u64,
    rejected: u64,
    priority: u32,
    directories: HashMap<u32, Directory>,
}
impl Drop for NativeFsState {
    fn drop(&mut self) {
        if self.backend.is_some() {
            log::info!(
                "NATIVE_FS_COUNTERS calls={} reads={} writes={} commits={} rejected={} zakuro_filesystem_calls={}",
                self.calls,
                self.reads,
                self.writes,
                self.commits,
                self.rejected,
                0_u64
            );
        }
    }
}
pub fn enabled() -> bool {
    matches!(
        std::env::var("POKEMOON_FILESYSTEM").as_deref(),
        Ok("native") | Ok("native-strict")
    )
}
fn report_save_counters(s: &Context<'_, impl FsMemory, impl Sized>) {
    let state = &s.fs;
    log::info!(
        "NATIVE_FS_COUNTERS calls={} reads={} writes={} commits={} rejected={} zakuro_filesystem_calls={}",
        state.calls,
        state.reads,
        state.writes,
        state.commits,
        state.rejected,
        0_u64
    );
}
fn word(s: &mut Context<'_, impl FsMemory, impl Sized>, b: &CommandBuffer, i: u32) -> u32 {
    b.get(s.memory, i)
}
fn wide(s: &mut Context<'_, impl FsMemory, impl Sized>, b: &CommandBuffer, i: u32) -> u64 {
    word(s, b, i) as u64 | ((word(s, b, i + 1) as u64) << 32)
}
fn checked_span(
    s: &Context<'_, impl FsMemory, impl Sized>,
    ptr: u32,
    size: u32,
    writable: bool,
) -> Result<(), ResultCode> {
    let end = ptr.checked_add(size).ok_or(errors::FS_INVALID_PATH)?;
    let mut cursor = ptr;
    while cursor < end {
        let info = s.memory.query(cursor);
        if info.permission & if writable { 2 } else { 1 } == 0 {
            return Err(errors::FS_INVALID_PATH);
        }
        let limit = info
            .base
            .checked_add(info.size)
            .ok_or(errors::FS_INVALID_PATH)?;
        if limit <= cursor {
            return Err(errors::FS_INVALID_PATH);
        }
        cursor = limit.min(end);
    }
    Ok(())
}
fn bytes(
    s: &mut Context<'_, impl FsMemory, impl Sized>,
    size: u32,
    ptr: u32,
) -> Result<Vec<u8>, ResultCode> {
    if size > 16 * 1024 * 1024 || ptr.checked_add(size).is_none() {
        return Err(errors::FS_INVALID_PATH);
    }
    checked_span(s, ptr, size, false)?;
    let mut out = vec![0; size as usize];
    s.memory.read_bytes(ptr, &mut out);
    Ok(out)
}
fn path(
    s: &mut Context<'_, impl FsMemory, impl Sized>,
    kind: u32,
    size: u32,
    ptr: u32,
) -> Result<String, ResultCode> {
    if size > 0x400 {
        return Err(errors::FS_INVALID_PATH);
    }
    let raw = bytes(s, size, ptr)?;
    let text = match kind {
        1 => String::new(),
        3 => {
            let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
            if raw[end..].iter().any(|&c| c != 0) {
                return Err(errors::FS_INVALID_PATH);
            }
            std::str::from_utf8(&raw[..end])
                .map_err(|_| errors::FS_INVALID_PATH)?
                .to_owned()
        }
        4 => {
            if raw.len() % 2 != 0 {
                return Err(errors::FS_INVALID_PATH);
            }
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
            if units[end..].iter().any(|&c| c != 0) {
                return Err(errors::FS_INVALID_PATH);
            }
            String::from_utf16(&units[..end]).map_err(|_| errors::FS_INVALID_PATH)?
        }
        _ => return Err(errors::FS_INVALID_PATH),
    };
    // CTR paths begin with /; this is a guest-root marker, not a host path.
    let relative = text.strip_prefix('/').unwrap_or(&text);
    if relative.starts_with('/')
        || relative.contains('\\')
        || relative.contains(':')
        || relative.split('/').any(|c| c == ".." || c == ".")
    {
        return Err(errors::FS_INVALID_PATH);
    }
    Ok(relative.to_owned())
}
fn archive(
    s: &Context<'_, impl FsMemory, impl Sized>,
    id: u32,
    raw: &[u8],
) -> Result<Archive, ResultCode> {
    let w = |i: usize| {
        raw.get(i..i + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .unwrap_or(0)
    };
    let prefix = match id {
        3 | 0x2345678a => "content".to_owned(),
        4 => std::env::var("POKEMOON_SAVEDATA_SUBDIR")
            .unwrap_or_else(|_| format!("savedata/{:016X}", s.kernel.program_id)),
        6 | 7 => format!(
            "{}/{:016X}",
            if id == 6 { "extdata" } else { "shared-extdata" },
            w(4) as u64 | ((w(8) as u64) << 32)
        ),
        8 => format!("system-savedata/{:08X}{:08X}", w(0), w(4)),
        9 => "sdmc".to_owned(),
        _ => return Err(errors::FS_NOT_FOUND),
    };
    Ok(Archive { id, prefix })
}
fn session(s: &mut Context<'_, impl FsMemory, impl Sized>, name: &str, id: u32) -> u32 {
    let object = s
        .kernel
        .objects
        .insert(KObject::ClientSession(ClientSession {
            service: name.into(),
            subhandle: id,
        }));
    s.kernel.handles.create(&mut s.kernel.objects, object, name)
}
fn result(
    s: &mut Context<'_, impl FsMemory, impl Sized>,
    b: &CommandBuffer,
    c: u16,
    r: Result<(), ResultCode>,
) {
    match r {
        Ok(()) => b.reply(s.memory, c, &[]),
        Err(e) => b.reply_error(s.memory, c, e.0),
    }
}
fn open_file(
    s: &mut Context<'_, impl FsMemory, impl Sized>,
    a: Archive,
    kind: u32,
    size: u32,
    ptr: u32,
    flags: u32,
) -> Result<u32, ResultCode> {
    let (root, name, base, limit, writable) = if a.id == 3 || a.id == 0x2345678a {
        let raw = bytes(s, size, ptr)?;
        let selected = if kind == 2 && raw.len() >= 4 {
            u32::from_le_bytes(raw[..4].try_into().unwrap())
        } else {
            0
        };
        if selected != 0 && selected != 5 {
            return Err(errors::FS_NOT_FOUND);
        }
        let (base, size) = s.romfs_range.ok_or(errors::FS_NOT_FOUND)?;
        (1, "image".to_owned(), base, size, false)
    } else {
        let name = path(s, kind, size, ptr)?;
        if name.is_empty() {
            return Err(errors::FS_INVALID_PATH);
        }
        (
            2,
            format!("{}/{}", a.prefix, name),
            0,
            u64::MAX,
            flags & 2 != 0,
        )
    };
    let (native, total) =
        s.fs.backend
            .as_ref()
            .unwrap()
            .open(root, &name, if root == 1 { 1 } else { flags })?;
    let state = &mut s.fs;
    state.next += 1;
    let id = state.next as u32;
    state.files.insert(
        id,
        File {
            native,
            base,
            size: limit.min(total.saturating_sub(base)),
            writable,
        },
    );
    Ok(id)
}
fn format(
    s: &mut Context<'_, impl FsMemory, impl Sized>,
    a: &Archive,
    info: &[u32],
) -> Result<(), ResultCode> {
    let backend = s.fs.backend.as_ref().unwrap();
    let prefix = CString::new(a.prefix.as_str()).map_err(|_| errors::FS_INVALID_PATH)?;
    // Real formatting semantics, retaining old data in a recovery sibling.
    status(unsafe { (backend.format)(backend.context, prefix.as_ptr()) })?;
    let (h, _) = backend.open(2, &format!("{}/.format", a.prefix), 7)?;
    let data: Vec<u8> = info.iter().flat_map(|w| w.to_le_bytes()).collect();
    let mut count = 0;
    status(unsafe {
        (backend.write)(
            backend.context,
            h,
            0,
            data.as_ptr(),
            data.len() as u32,
            &mut count,
        )
    })?;
    status(unsafe { (backend.flush)(backend.context, h) })?;
    status(unsafe { (backend.close)(backend.context, h) })?;
    status(unsafe { (backend.commit)(backend.context) })
}
pub fn handle(
    s: &mut Context<'_, impl FsMemory, impl Sized>,
    b: &CommandBuffer,
    h: Header,
    target: &Target,
) -> bool {
    if s.fs.backend.is_none() {
        match Backend::new(s) {
            Ok(backend) => s.fs.backend = Some(backend),
            Err(e) => panic!("NATIVE_FS_REQUIRED: {e}"),
        }
    }
    s.fs.calls += 1;
    if s.fs.calls % 1000 == 1 {
        let state = &s.fs;
        log::info!(
            "NATIVE_FS_COUNTERS calls={} reads={} writes={} commits={} rejected={} zakuro_filesystem_calls={}",
            state.calls,
            state.reads,
            state.writes,
            state.commits,
            state.rejected,
            0_u64
        );
    }
    if let Target::Service { name, subhandle } = target {
        if name == "FSFile" {
            return file_command(s, b, h, *subhandle);
        }
        if name == "FSDirectory" {
            return directory_command(s, b, h, *subhandle);
        }
    }
    let c = h.command_id();
    log::debug!("NATIVE_FS cmd=0x{c:04X}");
    let r: Result<(), ResultCode> = (|| {
        match c {
            0x0801 | 0x0861 | 0x0840 | 0x085a | 0x085d => {}
            0x0862 => {
                s.fs.priority = word(s, b, 1);
            }
            0x0863 => {
                b.reply(s.memory, c, &[s.fs.priority]);
                return Ok(());
            }
            // CTR local anti-rollback bookkeeping, not an online operation.
            // IPC verified against Azahar FS_USER Get/SetThisSaveDataSecureValue.
            0x086e | 0x086f => {
                let slot = word(s, b, 1);
                let name = format!("secure-values/{:016X}/{slot:08X}", s.kernel.program_id);
                let value = if c == 0x086e {
                    Some(wide(s, b, 2))
                } else {
                    None
                };
                let backend = s.fs.backend.as_ref().unwrap();
                if let Some(value) = value {
                    let (handle, _) = backend.open(2, &name, 7)?;
                    let data = value.to_le_bytes();
                    let mut count = 0;
                    let written = status(unsafe {
                        (backend.write)(backend.context, handle, 0, data.as_ptr(), 8, &mut count)
                    });
                    let flushed = status(unsafe { (backend.flush)(backend.context, handle) });
                    let closed = status(unsafe { (backend.close)(backend.context, handle) });
                    written?;
                    flushed?;
                    closed?;
                    if count != 8 {
                        return Err(errors::FS_UNEXPECTED_FILE_OR_DIRECTORY);
                    }
                    status(unsafe { (backend.commit)(backend.context) })?;
                    s.fs.commits += 1;
                    log::info!("NATIVE_SECURE_VALUE_STORED slot={slot} value={value:016X}");
                    report_save_counters(s);
                } else {
                    let (exists, value) = match backend.open(2, &name, 1) {
                        Ok((handle, size)) => {
                            let mut data = [0; 8];
                            let mut count = 0;
                            let read = status(unsafe {
                                (backend.read)(
                                    backend.context,
                                    handle,
                                    0,
                                    data.as_mut_ptr(),
                                    8,
                                    &mut count,
                                )
                            });
                            let closed =
                                status(unsafe { (backend.close)(backend.context, handle) });
                            read?;
                            closed?;
                            if count != 8 || size != 8 {
                                return Err(errors::FS_UNEXPECTED_FILE_OR_DIRECTORY);
                            }
                            (true, u64::from_le_bytes(data))
                        }
                        Err(error) if error == errors::FS_NOT_FOUND => (false, 0),
                        Err(error) => return Err(error),
                    };
                    // Native storage is not a physical gamecard; do not bypass
                    // secure-value comparisons by falsely setting that flag.
                    b.reply(
                        s.memory,
                        c,
                        &[exists as u32, 0, value as u32, (value >> 32) as u32],
                    );
                    log::info!("NATIVE_SECURE_VALUE_READ slot={slot} exists={exists}");
                    report_save_counters(s);
                    return Ok(());
                }
            }
            0x080c => {
                let id = word(s, b, 1);
                let size = word(s, b, 3);
                let ptr = word(s, b, 5);
                let raw = bytes(s, size, ptr)?;
                let a = archive(s, id, &raw)?;
                if !matches!(id, 3 | 9 | 0x2345678a) {
                    let backend = s.fs.backend.as_ref().unwrap();
                    let (handle, _) = backend
                        .open(2, &format!("{}/.format", a.prefix), 1)
                        .map_err(|_| {
                            if id == 6 {
                                errors::FS_NOT_FOUND_INVALID_STATE
                            } else {
                                errors::FS_NOT_FORMATTED
                            }
                        })?;
                    status(unsafe { (backend.close)(backend.context, handle) })?;
                }
                let state = &mut s.fs;
                state.next += 1;
                let id = state.next;
                state.archives.insert(id, a);
                b.reply(s.memory, c, &[id as u32, (id >> 32) as u32]);
                return Ok(());
            }
            0x0802 | 0x0803 => {
                let (a, kind, size, ptr, flags) = if c == 0x0802 {
                    let handle = wide(s, b, 2);
                    let a =
                        s.fs.archives
                            .get(&handle)
                            .cloned()
                            .ok_or(errors::FS_ARCHIVE_NOT_MOUNTED)?;
                    (
                        a,
                        word(s, b, 4),
                        word(s, b, 5),
                        word(s, b, 9),
                        word(s, b, 6),
                    )
                } else {
                    let id = word(s, b, 2);
                    let size = word(s, b, 4);
                    let ptr = word(s, b, 10);
                    let raw = bytes(s, size, ptr)?;
                    (
                        archive(s, id, &raw)?,
                        word(s, b, 5),
                        word(s, b, 6),
                        word(s, b, 12),
                        word(s, b, 7),
                    )
                };
                let id = open_file(s, a, kind, size, ptr, flags)?;
                let handle = session(s, "FSFile", id);
                b.reply_with_handle(s.memory, c, handle);
                return Ok(());
            }
            0x0808 => {
                let handle = wide(s, b, 2);
                let a =
                    s.fs.archives
                        .get(&handle)
                        .cloned()
                        .ok_or(errors::FS_ARCHIVE_NOT_MOUNTED)?;
                let kind = word(s, b, 4);
                let len = word(s, b, 5);
                let ptr = word(s, b, 10);
                let name = path(s, kind, len, ptr)?;
                let size = wide(s, b, 7);
                let backend = s.fs.backend.as_ref().unwrap();
                let (h, _) = backend.open(2, &format!("{}/{}", a.prefix, name), 7)?;
                let resized = status(unsafe { (backend.resize)(backend.context, h, size) });
                let closed = status(unsafe { (backend.close)(backend.context, h) });
                resized?;
                closed?;
            }
            0x0804 | 0x0809 => {
                let handle = wide(s, b, 2);
                let a =
                    s.fs.archives
                        .get(&handle)
                        .cloned()
                        .ok_or(errors::FS_ARCHIVE_NOT_MOUNTED)?;
                if matches!(a.id, 3 | 0x2345678a) {
                    return Err(errors::FS_UNEXPECTED_FILE_OR_DIRECTORY);
                }
                let kind = word(s, b, 4);
                let len = word(s, b, 5);
                let ptr = word(s, b, if c == 0x0804 { 7 } else { 8 });
                let name = path(s, kind, len, ptr)?;
                let name = CString::new(format!("{}/{}", a.prefix, name))
                    .map_err(|_| errors::FS_INVALID_PATH)?;
                let backend = s.fs.backend.as_ref().unwrap();
                status(unsafe {
                    if c == 0x0804 {
                        (backend.remove)(backend.context, name.as_ptr())
                    } else {
                        (backend.mkdir)(backend.context, name.as_ptr())
                    }
                })?;
            }
            0x0805 => {
                let source_archive = wide(s, b, 2);
                let dest_archive = wide(s, b, 6);
                if source_archive != dest_archive {
                    return Err(errors::FS_INVALID_PATH);
                }
                let a =
                    s.fs.archives
                        .get(&source_archive)
                        .cloned()
                        .ok_or(errors::FS_ARCHIVE_NOT_MOUNTED)?;
                if matches!(a.id, 3 | 0x2345678a) {
                    return Err(errors::FS_UNEXPECTED_FILE_OR_DIRECTORY);
                }
                let kind = word(s, b, 4);
                let len = word(s, b, 5);
                let ptr = word(s, b, 11);
                let source = path(s, kind, len, ptr)?;
                let kind = word(s, b, 8);
                let len = word(s, b, 9);
                let ptr = word(s, b, 13);
                let destination = path(s, kind, len, ptr)?;
                let source = CString::new(format!("{}/{}", a.prefix, source))
                    .map_err(|_| errors::FS_INVALID_PATH)?;
                let destination = CString::new(format!("{}/{}", a.prefix, destination))
                    .map_err(|_| errors::FS_INVALID_PATH)?;
                let backend = s.fs.backend.as_ref().unwrap();
                status(unsafe {
                    (backend.rename)(backend.context, source.as_ptr(), destination.as_ptr())
                })?;
            }
            0x080b => {
                let handle = wide(s, b, 1);
                let a =
                    s.fs.archives
                        .get(&handle)
                        .cloned()
                        .ok_or(errors::FS_ARCHIVE_NOT_MOUNTED)?;
                let kind = word(s, b, 3);
                let size = word(s, b, 4);
                let ptr = word(s, b, 6);
                let relative = path(s, kind, size, ptr)?;
                let name = CString::new(if relative.is_empty() {
                    a.prefix
                } else {
                    format!("{}/{}", a.prefix, relative)
                })
                .map_err(|_| errors::FS_INVALID_PATH)?;
                let mut entries = Vec::<DirectoryEntry>::new();
                let backend = s.fs.backend.as_ref().unwrap();
                status(unsafe {
                    (backend.list)(
                        backend.context,
                        name.as_ptr(),
                        collect_entry,
                        (&mut entries as *mut Vec<DirectoryEntry>).cast(),
                    )
                })?;
                s.fs.next += 1;
                let id = s.fs.next as u32;
                s.fs.directories
                    .insert(id, Directory { entries, cursor: 0 });
                let handle = session(s, "FSDirectory", id);
                b.reply_with_handle(s.memory, c, handle);
                return Ok(());
            }
            0x080d => {
                let backend = s.fs.backend.as_ref().unwrap();
                status(unsafe { (backend.commit)(backend.context) })?;
                s.fs.commits += 1;
                let descriptors: Vec<u32> = (6..10).map(|i| word(s, b, i)).collect();
                b.set(s.memory, 0, Header::new(c, 1, 4).0);
                b.set(s.memory, 1, 0);
                for (i, w) in descriptors.into_iter().enumerate() {
                    b.set(s.memory, 2 + i as u32, w);
                }
                return Ok(());
            }
            0x080e => {
                let handle = wide(s, b, 1);
                s.fs.archives.remove(&handle);
            }
            0x080f => {
                let info = [
                    word(s, b, 1).wrapping_mul(512),
                    word(s, b, 2),
                    word(s, b, 3),
                    word(s, b, 6) & 0xff,
                ];
                format(s, &archive(s, 4, &[])?, &info)?;
            }
            0x084c => {
                let id = word(s, b, 1);
                let len = word(s, b, 3);
                let ptr = word(s, b, 11);
                let raw = bytes(s, len, ptr)?;
                let info = [
                    word(s, b, 4).wrapping_mul(512),
                    word(s, b, 5),
                    word(s, b, 6),
                    word(s, b, 9) & 0xff,
                ];
                format(s, &archive(s, id, &raw)?, &info)?;
            }
            0x0845 => {
                let id = word(s, b, 1);
                let len = word(s, b, 3);
                let ptr = word(s, b, 5);
                let raw = bytes(s, len, ptr)?;
                let a = archive(s, id, &raw)?;
                let backend = s.fs.backend.as_ref().unwrap();
                let (h, _) = backend
                    .open(2, &format!("{}/.format", a.prefix), 1)
                    .map_err(|_| errors::FS_NOT_FORMATTED)?;
                let mut data = [0; 16];
                let mut count = 0;
                let read = status(unsafe {
                    (backend.read)(backend.context, h, 0, data.as_mut_ptr(), 16, &mut count)
                });
                status(unsafe { (backend.close)(backend.context, h) })?;
                read?;
                if count != 16 {
                    return Err(errors::FS_NOT_FORMATTED);
                }
                let words: Vec<u32> = data
                    .chunks_exact(4)
                    .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
                    .collect();
                b.reply(s.memory, c, &words);
                return Ok(());
            }
            0x0851 => {
                let raw: Vec<u8> = (1..4).flat_map(|i| word(s, b, i).to_le_bytes()).collect();
                let a = archive(s, 6, &raw)?;
                let info = [0, word(s, b, 5), word(s, b, 6), 0];
                let backend = s.fs.backend.as_ref().unwrap();
                if let Ok((handle, _)) = backend.open(2, &format!("{}/.format", a.prefix), 1) {
                    status(unsafe { (backend.close)(backend.context, handle) })?;
                } else {
                    format(s, &a, &info)?;
                }
                let descriptor = word(s, b, 10);
                let ptr = word(s, b, 11);
                b.set(s.memory, 0, Header::new(c, 1, 2).0);
                b.set(s.memory, 1, 0);
                b.set(s.memory, 2, descriptor);
                b.set(s.memory, 3, ptr);
                return Ok(());
            }
            0x0813 => {
                b.reply(s.memory, c, &[0]);
                return Ok(());
            }
            0x0817 | 0x0818 | 0x0821 | 0x083d => {
                b.reply(s.memory, c, &[1]);
                return Ok(());
            }
            0x0814 | 0x0815 | 0x0849 => {
                b.reply(s.memory, c, &[0x200, 0x4000, 0x80000, 0x80000]);
                return Ok(());
            }
            _ => {
                s.fs.rejected += 1;
                log::error!("NATIVE_FS_UNSUPPORTED command=0x{c:04X}");
                return Err(errors::FS_UNEXPECTED_FILE_OR_DIRECTORY);
            }
        }
        b.reply(s.memory, c, &[]);
        Ok(())
    })();
    if let Err(e) = r {
        log::debug!("NATIVE_FS_RESULT command=0x{c:04X} error={:08X}", e.0);
        b.reply_error(s.memory, c, e.0);
    }
    true
}
fn file_command(
    s: &mut Context<'_, impl FsMemory, impl Sized>,
    b: &CommandBuffer,
    h: Header,
    id: u32,
) -> bool {
    let c = h.command_id();
    let file = s.fs.files.get(&id).cloned();
    let r: Result<(), ResultCode> = (|| {
        let f = file.ok_or(errors::FS_NOT_FOUND)?;
        match c {
            0x0802 | 0x0803 => {
                let offset = wide(s, b, 1);
                let requested = word(s, b, 3);
                let ptr = word(s, b, if c == 0x0802 { 5 } else { 6 });
                if requested > 16 * 1024 * 1024 || ptr.checked_add(requested).is_none() {
                    return Err(errors::FS_INVALID_PATH);
                }
                checked_span(s, ptr, requested, c == 0x0802)?;
                let mut data = if c == 0x0803 {
                    bytes(s, requested, ptr)?
                } else {
                    vec![0; requested as usize]
                };
                let mut count = 0;
                let backend = s.fs.backend.as_ref().unwrap();
                if c == 0x0802 {
                    let length = (requested as u64).min(f.size.saturating_sub(offset)) as u32;
                    if length > 0 {
                        let position = f.base.checked_add(offset).ok_or(errors::FS_INVALID_PATH)?;
                        status(unsafe {
                            (backend.read)(
                                backend.context,
                                f.native,
                                position,
                                data.as_mut_ptr(),
                                length,
                                &mut count,
                            )
                        })?;
                    }
                    s.memory.write_bytes(ptr, &data[..count as usize]);
                    s.fs.reads += 1;
                } else {
                    if !f.writable {
                        return Err(errors::FS_UNEXPECTED_FILE_OR_DIRECTORY);
                    }
                    status(unsafe {
                        (backend.write)(
                            backend.context,
                            f.native,
                            offset,
                            data.as_ptr(),
                            requested,
                            &mut count,
                        )
                    })?;
                    let flags = word(s, b, 4);
                    if flags & 1 != 0 {
                        let backend = s.fs.backend.as_ref().unwrap();
                        status(unsafe { (backend.flush)(backend.context, f.native) })?;
                    }
                    s.fs.writes += 1;
                    for file in
                        s.fs.files
                            .values_mut()
                            .filter(|file| file.native == f.native)
                    {
                        file.size = file.size.max(offset.saturating_add(count as u64));
                    }
                }
                b.set(s.memory, 0, Header::new(c, 2, 2).0);
                b.set(s.memory, 1, 0);
                b.set(s.memory, 2, count);
                b.set(
                    s.memory,
                    3,
                    (requested << 4) | if c == 0x0802 { 0xc } else { 0xa },
                );
                b.set(s.memory, 4, ptr);
                // Preserve asynchronous game-card timing expected by retail callers.
                if c == 0x0802 {
                    let wait = nanos_to_ticks((count as u64 * 94 + 582778).max(663124));
                    s.kernel.sleep_current(wait, s.tick);
                }
                return Ok(());
            }
            0x0804 => {
                b.reply(s.memory, c, &[f.size as u32, (f.size >> 32) as u32]);
                return Ok(());
            }
            0x0805 => {
                let size = wide(s, b, 1);
                let backend = s.fs.backend.as_ref().unwrap();
                if !f.writable {
                    return Err(errors::FS_UNEXPECTED_FILE_OR_DIRECTORY);
                }
                status(unsafe { (backend.resize)(backend.context, f.native, size) })?;
                for file in
                    s.fs.files
                        .values_mut()
                        .filter(|file| file.native == f.native)
                {
                    file.size = size;
                }
            }
            0x0808 => {
                let other =
                    s.fs.files
                        .iter()
                        .any(|(&other, file)| other != id && file.native == f.native);
                if !other {
                    let backend = s.fs.backend.as_ref().unwrap();
                    status(unsafe { (backend.close)(backend.context, f.native) })?;
                }
                s.fs.files.remove(&id);
            }
            0x080c => {
                s.fs.next += 1;
                let alias = s.fs.next as u32;
                s.fs.files.insert(alias, f);
                let handle = session(s, "FSFile", alias);
                b.reply_with_handle(s.memory, c, handle);
                return Ok(());
            }
            0x0809 => {
                let backend = s.fs.backend.as_ref().unwrap();
                status(unsafe { (backend.flush)(backend.context, f.native) })?;
            }
            0x0806 | 0x080b => {
                b.reply(s.memory, c, &[0]);
                return Ok(());
            }
            0x0807 | 0x080a => {}
            _ => {
                s.fs.rejected += 1;
                log::error!("NATIVE_FS_FILE_UNSUPPORTED command=0x{c:04X}");
                return Err(errors::FS_UNEXPECTED_FILE_OR_DIRECTORY);
            }
        }
        Ok(())
    })();
    result(s, b, c, r);
    true
}
fn directory_command(
    s: &mut Context<'_, impl FsMemory, impl Sized>,
    b: &CommandBuffer,
    h: Header,
    id: u32,
) -> bool {
    let c = h.command_id();
    let r: Result<(), ResultCode> = (|| {
        match c {
            0x0801 => {
                let count = word(s, b, 1);
                let descriptor = word(s, b, 2);
                let ptr = word(s, b, 3);
                let length = count.checked_mul(0x228).ok_or(errors::FS_INVALID_PATH)?;
                if length > 16 * 1024 * 1024 {
                    return Err(errors::FS_INVALID_PATH);
                }
                checked_span(s, ptr, length, true)?;
                let directory = s.fs.directories.get_mut(&id).ok_or(errors::FS_NOT_FOUND)?;
                let end = directory
                    .cursor
                    .saturating_add(count as usize)
                    .min(directory.entries.len());
                let returned = end - directory.cursor;
                for (i, entry) in directory.entries[directory.cursor..end].iter().enumerate() {
                    let mut raw = [0u8; 0x228];
                    for (j, unit) in entry.name.encode_utf16().take(0x105).enumerate() {
                        raw[j * 2..j * 2 + 2].copy_from_slice(&unit.to_le_bytes());
                    }
                    let (stem, extension) = entry
                        .name
                        .rsplit_once('.')
                        .filter(|_| !entry.directory)
                        .unwrap_or((&entry.name, ""));
                    for (j, byte) in stem.bytes().take(8).enumerate() {
                        raw[0x20c + j] = byte.to_ascii_uppercase();
                    }
                    for (j, byte) in extension.bytes().take(3).enumerate() {
                        raw[0x216 + j] = byte.to_ascii_uppercase();
                    }
                    raw[0x21a] = 1;
                    raw[0x21c] = entry.directory as u8;
                    raw[0x21e] = !entry.directory as u8;
                    raw[0x220..0x228].copy_from_slice(&entry.size.to_le_bytes());
                    s.memory.write_bytes(ptr + i as u32 * 0x228, &raw);
                }
                directory.cursor = end;
                b.set(s.memory, 0, Header::new(c, 2, 2).0);
                b.set(s.memory, 1, 0);
                b.set(s.memory, 2, returned as u32);
                b.set(s.memory, 3, descriptor);
                b.set(s.memory, 4, ptr);
                return Ok(());
            }
            0x0802 => {
                s.fs.directories.remove(&id);
            }
            _ => {
                s.fs.rejected += 1;
                log::error!("NATIVE_FS_DIRECTORY_UNSUPPORTED command=0x{c:04X}");
                return Err(errors::FS_UNEXPECTED_FILE_OR_DIRECTORY);
            }
        }
        Ok(())
    })();
    result(s, b, c, r);
    true
}
