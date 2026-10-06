//! Native in-process execution pump. Platform/service owners handle returned
//! SVC exits; no emulator scheduler, application, or platform object is needed.
use super::{aot::{AotMemory,Library,Stop},arm::{Cpu,Exit}};
use crate::{execution::context::{ThreadContext,THREAD_EXIT_MAGIC},scheduler::{NativeScheduler,ThreadStatus}};

/// Select/wake native guest threads, or advance guest time while all wait.
/// Returns false when no instruction should execute on this pump iteration.
pub fn prepare<R>(cpu:&mut Cpu,kernel:&mut NativeScheduler<ThreadContext,R>,timing:&ProcessTiming,deadline:Option<u64>,audio_running:bool)->bool{
    if kernel.reschedule_pending||kernel.current_thread.is_none(){
        kernel.reschedule_pending=false;
        if kernel.schedule(cpu,cpu.cycles){crate::process::svc::apply_wait_result(kernel,&mut cpu.regs);}
    }
    if kernel.current_thread.is_none(){
        let mut limit=deadline.unwrap_or(cpu.cycles.saturating_add(timing.frame_period));
        if audio_running{limit=limit.min(timing.audio);}
        cpu.cycles=match kernel.next_wakeup(){Some(tick)if tick>cpu.cycles=>tick.min(limit),_=>limit};
        kernel.reschedule_pending=true;return false;
    }
    if cpu.regs[15]==THREAD_EXIT_MAGIC{
        if let Some(thread)=kernel.current_mut(){thread.status=ThreadStatus::Dead;thread.clear_wait();}
        kernel.current_thread=None;kernel.reschedule_pending=true;return false;
    }
    true
}

/// Guest-clock deadlines, independent of host presentation/audio callbacks.
pub struct ProcessTiming{pub frame:u64,pub audio:u64,pub preempt:u64,frame_period:u64,audio_period:u64,preempt_period:u64}
impl ProcessTiming{
    pub fn new(frame_period:u64,audio_period:u64,preempt_period:u64)->Self{
        assert!(frame_period!=0&&audio_period!=0&&preempt_period!=0);
        Self{frame:frame_period,audio:audio_period,preempt:preempt_period,frame_period,audio_period,preempt_period}
    }
    pub fn frame_done(&mut self,tick:u64){self.frame=tick.saturating_add(self.frame_period);}
    pub fn audio_due(&mut self,tick:u64)->bool{if tick<self.audio{return false;}self.audio=self.audio.saturating_add(self.audio_period);true}
    pub fn preempt_due(&mut self,tick:u64)->bool{if tick<self.preempt{return false;}self.preempt=tick.saturating_add(self.preempt_period);true}
    pub fn execution_limit(&self,deadline:Option<u64>)->u64{self.frame.min(self.audio).min(self.preempt).min(deadline.unwrap_or(u64::MAX))}
}

#[cfg(test)]mod timing_tests{
    use super::*;
    #[test]fn audio_deadlines_preserve_cadence_after_overshoot(){let mut timing=ProcessTiming::new(100,10,2);let mut due=0;while timing.audio_due(35){due+=1;}assert_eq!(due,3);assert_eq!(timing.audio,40);assert!(!timing.audio_due(35));}
    #[test]fn guest_execution_obeys_all_deadlines(){let mut timing=ProcessTiming::new(100,10,2);assert_eq!(timing.execution_limit(None),2);assert!(timing.preempt_due(2));assert_eq!(timing.execution_limit(Some(3)),3);timing.frame_done(110);assert_eq!(timing.frame,210);}
    #[test]fn native_prepare_schedules_exits_and_bounds_idle_audio_clock(){
        let mut cpu=Cpu::new();let mut kernel=NativeScheduler::<ThreadContext,()>::new(1,(),0,crate::scheduler::ThreadProfile{tls_base:0x10000000,tls_stride:512,heap_base:0x8000000});
        let timing=ProcessTiming::new(100,10,2);let id=kernel.create_thread("main",0x1000,0x8000,7,48,0);
        assert!(prepare(&mut cpu,&mut kernel,&timing,Some(100),true));assert_eq!(kernel.current_thread,Some(id));assert_eq!(cpu.regs[0],7);
        cpu.regs[15]=THREAD_EXIT_MAGIC;assert!(!prepare(&mut cpu,&mut kernel,&timing,Some(100),true));assert_eq!(kernel.thread(id).status,ThreadStatus::Dead);
        assert!(!prepare(&mut cpu,&mut kernel,&timing,Some(100),true));assert_eq!(cpu.cycles,10);
    }
}

pub struct ExecutionResult{
    pub exit:Option<Exit>,
    pub aot_instructions:u64,
    pub fallback_instructions:u64,
}

pub fn advance(cpu:&mut Cpu,memory:&mut impl AotMemory,library:Option<&Library>,deadline:u64)->ExecutionResult{
    if let Some(library)=library{
        if library.has_code(cpu.regs[15]|cpu.cpsr.thumb as u32){
            let (ran,stop)=library.run(cpu,memory,deadline.saturating_sub(cpu.cycles).max(1));
            if ran!=0||!matches!(stop,Stop::Left){
                let exit=match stop{Stop::Svc(n)=>Some(Exit::Supervisor(n)),Stop::Exit(e)=>Some(e),Stop::Left=>None};
                return ExecutionResult{exit,aot_instructions:ran,fallback_instructions:0};
            }
        }
    }
    ExecutionResult{exit:cpu.step(memory),aot_instructions:0,fallback_instructions:1}
}
