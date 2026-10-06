// SPDX-License-Identifier: MIT
// Copyright (c) 2026 fearkov. Generic ABI-4 execution bridge extraction.
// Memory/CPU ownership is native; no emulator/runtime crate dependency.
//! running code that 3dsrecomp turned into a library ahead of time. the
//! library's functions work on a context holding the registers and reach
//! memory through the same page tables the interpreter uses, calling back
//! here for anything else. whatever the library has no code for stays with
//! the interpreter.

use std::cell::Cell;
use std::ffi::c_void;
use std::path::Path;

use recomp_abi::{Code, Context, Host, EXIT_BUDGET, EXIT_SVC, EXIT_UNWIND};
use crate::execution::arm::{Bus,Cpu,Exit};

/// Page tables must remain valid throughout the mutable execution borrow.
pub trait AotMemory:Bus{fn page_tables(&self)->(*const *mut u8,*const *mut u8);}

pub use recomp_abi::Linked;

/// the library 3dsrecomp build installed for a title, if there is one.
pub fn installed(program_id: u64) -> Option<std::path::PathBuf> {
    recomp_abi::installed(program_id)
}

/// recompiled code, and how much of its work it handed back.
pub struct Library {
    code: recomp_abi::Library,
    /// instructions the code handed to the interpreter one at a time.
    fallbacks: Cell<u64>,
}

/// why a run of recompiled code ended.
pub enum Stop {
    /// an svc, the cpu is already past it.
    Svc(u32),
    /// the interpreter stopped on an instruction the code handed it.
    Exit(Exit),
    /// the budget ran out, or execution left for code the library does not
    /// have.
    Left,
}

/// what the callbacks reach through the context.
struct Machine<'a,M:AotMemory> {
    cpu: &'a mut Cpu,
    memory: &'a mut M,
    library: &'a Library,
    /// what the interpreter stopped with inside a callback.
    pending: Option<Exit>,
}

/// # Safety
///
/// ctx has to be a context run set up, whose user field points at a live
/// Machine.
unsafe fn machine<'a,M:AotMemory>(ctx:*mut Context)->&'a mut Machine<'a,M> {
    unsafe { &mut *((*ctx).user as *mut Machine<M>) }
}

unsafe extern "C" fn read8<M:AotMemory>(ctx: *mut Context, address: u32) -> u8 {
    unsafe { machine::<M>(ctx).memory.read8(address) }
}

unsafe extern "C" fn read16<M:AotMemory>(ctx: *mut Context, address: u32) -> u16 {
    unsafe { machine::<M>(ctx).memory.read16(address) }
}

unsafe extern "C" fn read32<M:AotMemory>(ctx: *mut Context, address: u32) -> u32 {
    unsafe { machine::<M>(ctx).memory.read32(address) }
}

unsafe extern "C" fn write8<M:AotMemory>(ctx: *mut Context, address: u32, value: u8) {
    unsafe { machine::<M>(ctx).memory.write8(address, value) }
}

unsafe extern "C" fn write16<M:AotMemory>(ctx: *mut Context, address: u32, value: u16) {
    unsafe { machine::<M>(ctx).memory.write16(address, value) }
}

unsafe extern "C" fn write32<M:AotMemory>(ctx: *mut Context, address: u32, value: u32) {
    unsafe { machine::<M>(ctx).memory.write32(address, value) }
}

/// runs one instruction the code left to the interpreter.
unsafe extern "C" fn interpret<M:AotMemory>(ctx: *mut Context, address: u32, _opcode: u32) {
    unsafe {
        let machine = machine::<M>(ctx);
        machine.library.fallbacks.set(machine.library.fallbacks.get() + 1);
        let ctx = &mut *ctx;
        load(ctx, machine.cpu);
        machine.cpu.regs[15] = address;
        let exit = machine.cpu.step(machine.memory);
        store(machine.cpu, ctx);
        let step = if ctx.thumb != 0 { 2 } else { 4 };
        if exit.is_some() || ctx.r[15] != address.wrapping_add(step) {
            machine.pending = exit;
            ctx.exit = EXIT_UNWIND;
        }
    }
}

unsafe extern "C" fn lookup<M:AotMemory>(ctx: *mut Context, address: u32) -> Option<Code> {
    unsafe { machine::<M>(ctx).library.lookup(address) }
}



fn load(ctx: &Context, cpu: &mut Cpu) {
    cpu.regs = ctx.r;
    cpu.cpsr.n = ctx.n != 0;
    cpu.cpsr.z = ctx.z != 0;
    cpu.cpsr.c = ctx.c != 0;
    cpu.cpsr.v = ctx.v != 0;
    cpu.cpsr.q = ctx.q != 0;
    cpu.cpsr.ge = ctx.ge;
    cpu.cpsr.thumb = ctx.thumb != 0;
    cpu.exclusive_addr = (ctx.exclusive != 0).then_some(ctx.exclusive_address);
}

fn store(cpu: &Cpu, ctx: &mut Context) {
    ctx.r = cpu.regs;
    ctx.n = cpu.cpsr.n as u8;
    ctx.z = cpu.cpsr.z as u8;
    ctx.c = cpu.cpsr.c as u8;
    ctx.v = cpu.cpsr.v as u8;
    ctx.q = cpu.cpsr.q as u8;
    ctx.ge = cpu.cpsr.ge;
    ctx.thumb = cpu.cpsr.thumb as u8;
    ctx.exclusive = cpu.exclusive_addr.is_some() as u8;
    ctx.exclusive_address = cpu.exclusive_addr.unwrap_or(0);
    ctx.tls = cpu.cp15.thread_id_ro;
}

impl Library {
    /// a library 3dsrecomp built.
    pub fn open(path: &Path) -> Result<Library, String> {
        recomp_abi::Library::open(path).map(Library::new)
    }

    /// the code linked into the program.
    pub fn linked(linked: &Linked) -> Result<Library, String> {
        recomp_abi::Library::linked(linked).map(Library::new)
    }

    fn new(code: recomp_abi::Library) -> Library {
        Library { code, fallbacks: Cell::new(0) }
    }

    /// how many instructions the code has handed to the interpreter.
    pub fn fallbacks(&self) -> u64 {
        self.fallbacks.get()
    }

    /// how many functions and modules there is code for.
    pub fn describe(&self) -> String {
        self.code.describe()
    }

    /// the code that can run from address, bit 0 set for Thumb, in the
    /// executable or in a module that is loaded.
    fn lookup(&self, address: u32) -> Option<Code> {
        self.code.lookup(address)
    }

    pub fn has_code(&self, address: u32) -> bool {
        self.lookup(address).is_some()
    }

    /// tells the code of the module called name where the title loaded it,
    /// zero when it unloads it.
    pub fn place(&mut self, name: &str, base: u32) {
        let Some(index) = self.code.module_index(name) else { return };
        self.code.place(index, base);
        if base != 0 {
            log::info!("recompiled code for {name} runs at 0x{base:08X}");
        }
    }

    /// runs recompiled code from the cpu's pc for about budget instructions,
    /// returning how many ran and why it stopped.
    pub fn run<M:AotMemory>(&self,cpu:&mut Cpu,memory:&mut M,budget:u64) -> (u64, Stop) {
        let host=Host{read8:read8::<M>,read16:read16::<M>,read32:read32::<M>,write8:write8::<M>,write16:write16::<M>,write32:write32::<M>,interpret:interpret::<M>,lookup:lookup::<M>};
        let cycles = cpu.cycles;
        let (read_pages, write_pages) = memory.page_tables();
        let mut ctx = Context {
            r: [0; 16],
            n: 0,
            z: 0,
            c: 0,
            v: 0,
            q: 0,
            thumb: 0,
            ge: 0,
            exclusive: 0,
            budget: budget.min(i32::MAX as u64) as i32,
            exit: 0,
            svc: 0,
            depth: 0,
            exclusive_address: 0,
            tls: 0,
            read_pages,
            write_pages,
            vfp: std::ptr::null_mut(),
            fpscr: std::ptr::null_mut(),
            host: &host,
            user: std::ptr::null_mut(),
        };
        store(cpu, &mut ctx);
        let mut machine = Machine { cpu, memory, library: self, pending: None };
        // the code works on the VFP registers where they are, so the
        // interpreter sees its changes without copying them around
        ctx.vfp = machine.cpu.vfp.regs.as_mut_ptr();
        ctx.fpscr = &mut machine.cpu.vfp.fpscr;
        ctx.user = &mut machine as *mut Machine<M> as *mut c_void;

        let stop = loop {
            if ctx.budget <= 0 {
                break Stop::Left;
            }
            let Some(code) = self.lookup(ctx.r[15] | ctx.thumb as u32) else {
                break Stop::Left;
            };
            ctx.exit = 0;
            ctx.depth = 0;
            // SAFETY: the code came from the library and ctx is set up the
            // way it expects, with user pointing at machine
            unsafe { code(&mut ctx) };
            if let Some(exit) = machine.pending.take() {
                break Stop::Exit(exit);
            }
            match ctx.exit {
                EXIT_SVC => break Stop::Svc(ctx.svc),
                EXIT_BUDGET => break Stop::Left,
                _ => {}
            }
        };

        load(&ctx, machine.cpu);
        let ran = budget.saturating_sub(ctx.budget.max(0) as u64);
        // the interpreter counted the instructions it ran for the code, which
        // the budget already has
        machine.cpu.cycles = cycles + ran;
        (ran, stop)
    }
}
#[cfg(test)]mod tests{
    use super::*;
    struct Memory([u8;16]);
    impl Bus for Memory{
        fn read8(&mut self,a:u32)->u8{self.0[a as usize]}
        fn read16(&mut self,a:u32)->u16{u16::from_le_bytes(self.0[a as usize..a as usize+2].try_into().unwrap())}
        fn read32(&mut self,a:u32)->u32{u32::from_le_bytes(self.0[a as usize..a as usize+4].try_into().unwrap())}
        fn write8(&mut self,a:u32,v:u8){self.0[a as usize]=v;}
        fn write16(&mut self,a:u32,v:u16){self.0[a as usize..a as usize+2].copy_from_slice(&v.to_le_bytes());}
        fn write32(&mut self,a:u32,v:u32){self.0[a as usize..a as usize+4].copy_from_slice(&v.to_le_bytes());}
    }
    impl AotMemory for Memory{fn page_tables(&self)->(*const *mut u8,*const *mut u8){(std::ptr::null(),std::ptr::null())}}
    unsafe extern "C" fn code(ctx:*mut Context){unsafe{
        let host=&*(*ctx).host;(host.write32)(ctx,4,0x12345678);(*ctx).r[0]=(host.read32)(ctx,4);
        (*ctx).r[15]+=4;(*ctx).budget-=2;(*ctx).exit=EXIT_SVC;(*ctx).svc=8;
    }}
    #[test]fn real_abi_library_and_native_callbacks(){
        static ENTRIES:[recomp_abi::Entry;1]=[recomp_abi::Entry{address:0x1000,code}];
        let linked=unsafe{Linked::new(1,recomp_abi::ABI,ENTRIES.as_ptr().cast(),1,std::ptr::null(),0)};
        let library=Library::linked(&linked).unwrap();let mut cpu=Cpu::new();cpu.regs[15]=0x1000;
        let mut memory=Memory([0;16]);let(ran,stop)=library.run(&mut cpu,&mut memory,20);
        assert_eq!(ran,2);assert!(matches!(stop,Stop::Svc(8)));assert_eq!(cpu.regs[0],0x12345678);
        assert_eq!(cpu.regs[15],0x1004);assert_eq!(cpu.cycles,2);assert_eq!(library.fallbacks(),0);
    }
    #[test]fn native_pump_routes_actual_abi_svc(){
        static ENTRIES:[recomp_abi::Entry;1]=[recomp_abi::Entry{address:0x1000,code}];
        let linked=unsafe{Linked::new(1,recomp_abi::ABI,ENTRIES.as_ptr().cast(),1,std::ptr::null(),0)};
        let library=Library::linked(&linked).unwrap();let mut cpu=Cpu::new();cpu.regs[15]=0x1000;
        let mut memory=Memory([0;16]);
        let result=super::super::pump::advance(&mut cpu,&mut memory,Some(&library),20);
        assert_eq!(result.aot_instructions,2);assert_eq!(result.fallback_instructions,0);
        assert!(matches!(result.exit,Some(Exit::Supervisor(8))));
        assert_eq!(cpu.regs[0],0x12345678);
    }
    #[test]fn native_pump_routes_missing_aot_to_independent_arm(){
        let mut cpu=Cpu::new();cpu.regs[15]=0;
        let mut memory=Memory([0;16]);memory.write32(0,0xe3a00007);
        let result=super::super::pump::advance(&mut cpu,&mut memory,None,20);
        assert_eq!(result.aot_instructions,0);assert_eq!(result.fallback_instructions,1);
        assert!(result.exit.is_none());assert_eq!(cpu.regs[0],7);assert_eq!(cpu.regs[15],4);
    }
}
