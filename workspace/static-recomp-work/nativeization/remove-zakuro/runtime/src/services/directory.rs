//! Native service namespace and typed handle allocation. Title profiles supply
//! endpoint names; no emulator dispatcher or concrete process shell is used.
use super::BackgroundStorage;
use crate::{
    execution::context::ThreadContext,
    ipc::{DirectoryAction, ServiceDirectory, message::Header},
    kernel::{ClientSession, KObject, Semaphore},
    scheduler::NativeScheduler,
};
pub struct ServiceNamespace {
    directory: ServiceDirectory,
    semaphore: Option<u32>,
    boss: BackgroundStorage,
}
#[derive(Default)]
pub struct DirectoryOutcome {
    pub opened: Option<String>,
    pub unavailable: Option<String>,
}
impl ServiceNamespace {
    pub fn new<'a>(ports: impl IntoIterator<Item = &'a str>) -> Self {
        let mut directory = ServiceDirectory::default();
        for name in ports {
            directory.register(name);
        }
        Self {
            directory,
            semaphore: None,
            boss: BackgroundStorage::default(),
        }
    }
    pub fn dispatch<R>(
        &mut self,
        words: &mut [u32; 64],
        kernel: &mut NativeScheduler<ThreadContext, R>,
        boss: bool,
    ) -> Result<DirectoryOutcome, String> {
        let mut outcome = DirectoryOutcome::default();
        if boss {
            self.boss.dispatch(words).map_err(str::to_owned)?;
            return Ok(outcome);
        }
        match self.directory.dispatch(words).map_err(str::to_owned)? {
            DirectoryAction::Reply => {}
            DirectoryAction::Unavailable(name) => {
                outcome.unavailable = Some(name);
            }
            DirectoryAction::Open(name) => {
                let object = kernel.objects.insert(KObject::ClientSession(ClientSession {
                    service: name.clone(),
                    subhandle: 0,
                }));
                let handle = kernel.handles.create(&mut kernel.objects, object, &name);
                words[0] = 0x00050042;
                words[1] = 0;
                words[2] = 0x10;
                words[3] = handle;
                outcome.opened = Some(name);
            }
            DirectoryAction::NotificationSemaphore => {
                let handle = if let Some(handle) = self.semaphore {
                    handle
                } else {
                    let object = kernel.objects.insert(KObject::Semaphore(Semaphore {
                        name: "native srv notifications".into(),
                        count: 0,
                        max_count: 16,
                    }));
                    let handle = kernel.handles.create(
                        &mut kernel.objects,
                        object,
                        "native srv notifications",
                    );
                    self.semaphore = Some(handle);
                    handle
                };
                words[0] = 0x00020042;
                words[1] = 0;
                words[2] = 0;
                words[3] = handle;
            }
        }
        let header = Header(words[0]);
        if 1 + header.normal_params() + header.translate_params() > 64 {
            return Err("native service response exceeds IPC buffer".into());
        }
        Ok(outcome)
    }
    pub fn notify<R>(&mut self, kernel: &mut NativeScheduler<ThreadContext, R>, id: u32) {
        if !self.directory.notify(id) {
            return;
        }
        if let Some(handle) = self.semaphore {
            if let Some(object) = kernel.handles.resolve(handle) {
                if let Some(KObject::Semaphore(semaphore)) = kernel.objects.get_mut(object) {
                    semaphore.count = (semaphore.count + 1).min(semaphore.max_count);
                    kernel.reschedule_pending = true;
                }
            }
        }
    }
}
impl Drop for ServiceNamespace {
    fn drop(&mut self) {
        if self.directory.calls > 0 {
            log::info!(
                "NATIVE_DIRECTORY_COUNTERS calls={} unknown_ports={} boss_calls={} zakuro_directory_calls=0",
                self.directory.calls,
                self.directory.unknown_ports,
                self.boss.calls
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::ThreadProfile;
    #[test]
    fn directory_opens_native_objects_only_and_notifies() {
        let mut directory = ServiceNamespace::new(["cfg:u"]);
        let mut kernel = NativeScheduler::<ThreadContext, u32>::new(
            1,
            0,
            0,
            ThreadProfile {
                tls_base: 0x10000000,
                tls_stride: 512,
                heap_base: 0x8000000,
            },
        );
        let mut words = [0; 64];
        words[0] = 0x10002;
        words[1] = 0x20;
        directory.dispatch(&mut words, &mut kernel, false).unwrap();
        words[0] = 0x50100;
        words[1] = u32::from_le_bytes(*b"cfg:");
        words[2] = b'u' as u32;
        words[3] = 5;
        assert_eq!(
            directory
                .dispatch(&mut words, &mut kernel, false)
                .unwrap()
                .opened
                .as_deref(),
            Some("cfg:u")
        );
        let object = kernel.handles.resolve(words[3]).unwrap();
        assert!(
            matches!(kernel.objects.get(object),Some(KObject::ClientSession(session))if session.service=="cfg:u")
        );
        words[0] = 0x20000;
        directory.dispatch(&mut words, &mut kernel, false).unwrap();
        let semaphore = words[3];
        words[0] = 0x90040;
        words[1] = 0x209;
        directory.dispatch(&mut words, &mut kernel, false).unwrap();
        directory.notify(&mut kernel, 0x209);
        let object = kernel.handles.resolve(semaphore).unwrap();
        assert!(
            matches!(kernel.objects.get(object),Some(KObject::Semaphore(value))if value.count==1)
        );
    }
}
