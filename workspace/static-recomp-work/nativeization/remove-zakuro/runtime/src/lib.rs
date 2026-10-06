//! Title-neutral native CTR process modules. No emulator/runtime dependency.
pub mod process;
pub mod ipc;
pub mod services;
pub mod kernel;
pub mod scheduler;
mod threads;
pub mod execution;
pub mod memory;
pub mod platform;
mod bits;
pub type GuestAddress = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageAllocation { pub addr: u32, pub size: u32 }

/// Memory operations supplied by the native address-space owner. Implementors
/// must validate mapped spans and reject inaccessible addresses, not invent data.
pub trait GuestMemory {
    fn read8(&mut self, address: GuestAddress) -> u8;
    fn read32(&mut self, address: GuestAddress) -> u32;
    fn read_bytes(&mut self, address: GuestAddress, bytes: &mut [u8]);
    fn write8(&mut self, address: GuestAddress, value: u8);
    fn write32(&mut self, address: GuestAddress, value: u32);
    fn write32_privileged(&mut self, address: GuestAddress, value: u32) -> bool;
    fn read_cstring(&mut self, address: GuestAddress, maximum: usize) -> String {
        let mut bytes = Vec::new();
        for i in 0..maximum {
            let value = self.read8(address.checked_add(i as u32).expect("CTR string address overflow"));
            if value == 0 { break; }
            bytes.push(value);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }
}
