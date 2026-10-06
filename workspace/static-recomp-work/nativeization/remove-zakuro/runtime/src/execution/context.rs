// SPDX-License-Identifier: MIT
// Copyright (c) 2026 fearkov. Narrow portable register-context extraction.
use super::arm::{Cpu,Psr};
pub const THREAD_EXIT_MAGIC:u32=0xfffffff0;
#[derive(Debug,Clone)]pub struct ThreadContext{
    pub regs:[u32;16],pub cpsr:Psr,pub vfp_regs:[u32;32],pub fpscr:u32,pub fpexc:u32,pub tls_pointer:u32,
}
impl ThreadContext{
    pub fn new(entry:u32,stack:u32,arg:u32,tls:u32)->Self{
        let mut regs=[0;16];regs[0]=arg;regs[13]=stack;regs[14]=THREAD_EXIT_MAGIC;regs[15]=entry&!1;
        Self{regs,cpsr:Psr{thumb:entry&1!=0,..Psr::default()},vfp_regs:[0;32],fpscr:0,fpexc:1<<30,tls_pointer:tls}
    }
    pub fn save_from(&mut self,cpu:&Cpu){self.regs=cpu.regs;self.cpsr=cpu.cpsr;self.vfp_regs=cpu.vfp.regs;self.fpscr=cpu.vfp.fpscr;self.fpexc=cpu.vfp.fpexc;self.tls_pointer=cpu.cp15.thread_id_ro;}
    pub fn restore_to(&self,cpu:&mut Cpu){cpu.regs=self.regs;cpu.cpsr=self.cpsr;cpu.vfp.regs=self.vfp_regs;cpu.vfp.fpscr=self.fpscr;cpu.vfp.fpexc=self.fpexc;cpu.cp15.thread_id_ro=self.tls_pointer;cpu.exclusive_addr=None;}
}
impl crate::scheduler::ExecutionContext for ThreadContext{
    type Machine=Cpu;
    fn new(entry:u32,stack:u32,arg:u32,tls:u32)->Self{Self::new(entry,stack,arg,tls)}
    fn save_from(&mut self,cpu:&Cpu){Self::save_from(self,cpu);}
    fn restore_to(&self,cpu:&mut Cpu){Self::restore_to(self,cpu);}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn native_context_roundtrip_and_exclusive_reset(){
        let mut context=ThreadContext::new(0x1001,0x8000,7,0x2000);let mut cpu=Cpu::new();context.restore_to(&mut cpu);
        assert_eq!(cpu.regs[15],0x1000);assert!(cpu.cpsr.thumb);assert_eq!(cpu.regs[0],7);assert_eq!(cpu.cp15.thread_id_ro,0x2000);
        cpu.regs[3]=9;cpu.vfp.regs[4]=42;cpu.exclusive_addr=Some(0x3000);context.save_from(&cpu);
        let mut resumed=Cpu::new();resumed.exclusive_addr=Some(1);context.restore_to(&mut resumed);assert_eq!(resumed.regs[3],9);assert_eq!(resumed.vfp.regs[4],42);assert_eq!(resumed.exclusive_addr,None);
    }
}
