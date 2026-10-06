// SPDX-License-Identifier: MIT
// Copyright (c) 2026 fearkov. CTR cooperative scheduling algorithms adapted
// to independent native context/region contracts. No Zakuro runtime calls.
// TriAevum SourceCtrRuntime thread descriptors/TLS/object namespace are the
// architecture model; AOT continuation scheduling remains cooperative because
// Moon's existing generated callbacks are single-execution-context ABI.
use crate::{GuestAddress as VAddr,kernel::*};
pub use crate::threads::{Thread,ThreadStatus,WaitResult,WaitSyscall};
pub const CPU_CLOCK_HZ:u64=268_111_856;
pub const LOWEST_PRIORITY:u32=0x3f;
const STARVATION_TICKS:u64=4*(CPU_CLOCK_HZ/60);
#[derive(Debug,Clone,Copy)]
pub struct ThreadProfile{pub tls_base:u32,pub tls_stride:u32,pub heap_base:u32}
pub trait ExecutionContext:Clone{
    type Machine;
    fn new(entry:u32,stack:u32,arg:u32,tls:u32)->Self;
    fn save_from(&mut self,machine:&Self::Machine);
    fn restore_to(&self,machine:&mut Self::Machine);
}
pub struct NativeScheduler<C:ExecutionContext,R> {
    profile:ThreadProfile,
    waits:u64,
    pub objects: ObjectStore,
    pub handles: HandleTable,
    pub threads: Vec<Thread<C>>,
    pub current_thread: Option<ThreadId>,

    /// next free slot in the TLS area.
    next_tls_slot: u32,

    pub process_id: u32,
    pub program_id: u64,
    pub memory_region: R,

    /// current top of the svcControlMemory heap.
    pub heap_top: VAddr,
    /// current top of the linear heap.
    pub linear_top: VAddr,
    pub linear_base: VAddr,

    /// set when something happened that might make a different thread
    /// runnable, so the run loop knows to call [Kernel::schedule].
    pub reschedule_pending: bool,
    /// set when every thread is blocked, so the run loop can idle instead of
    /// spinning.
    pub all_blocked: bool,
    /// flips while a thread starves the ones below it, handing every other
    /// slice to them.
    relief: bool,

    /// cached objects backing the pseudo-handles, so that duplicating
    /// CUR_THREAD_HANDLE twice yields the same object.
    thread_objects: std::collections::HashMap<ThreadId, ObjectId>,
    process_object: Option<ObjectId>,
}

impl<C:ExecutionContext,R> NativeScheduler<C,R> {
    pub fn new(program_id: u64, memory_region: R, linear_base: VAddr, profile:ThreadProfile) -> NativeScheduler<C,R> {
        NativeScheduler {
            profile,waits:0,
            objects: ObjectStore::default(),
            handles: HandleTable::default(),
            threads: Vec::new(),
            current_thread: None,
            next_tls_slot: 0,
            process_id: 0x22,
            program_id,
            memory_region,
            heap_top: profile.heap_base,
            linear_top: linear_base,
            linear_base,
            reschedule_pending: false,
            all_blocked: false,
            relief: false,
            thread_objects: std::collections::HashMap::new(),
            process_object: None,
        }
    }

    // -- threads ------------------------------------------------------------

    /// allocates the next TLS page slot for a new thread.
    pub fn allocate_tls(&mut self) -> VAddr {
        let addr=self.next_tls_slot.checked_mul(self.profile.tls_stride).and_then(|n|self.profile.tls_base.checked_add(n)).expect("native TLS namespace exhausted");
        self.next_tls_slot=self.next_tls_slot.checked_add(1).expect("native thread namespace exhausted");
        addr
    }

    pub fn create_thread(
        &mut self,
        name: impl Into<String>,
        entry: VAddr,
        stack_top: VAddr,
        arg: u32,
        priority: u32,
        processor_id: i32,
    ) -> ThreadId {
        let id = self.threads.len() as ThreadId;
        let tls = self.allocate_tls();
        let thread = Thread::new(
            id,
            name,
            entry,
            stack_top,
            arg,
            priority.min(LOWEST_PRIORITY),
            tls,
            processor_id,
        );
        log::debug!(
            "created thread {id} '{}' entry=0x{entry:08X} sp=0x{stack_top:08X} prio={priority}",
            thread.name
        );
        self.threads.push(thread);
        self.reschedule_pending = true;
        id
    }

    pub fn thread(&self, id: ThreadId) -> &Thread<C> {
        &self.threads[id as usize]
    }

    pub fn thread_mut(&mut self, id: ThreadId) -> &mut Thread<C> {
        &mut self.threads[id as usize]
    }

    pub fn current(&self) -> Option<&Thread<C>> {
        self.current_thread.map(|id| self.thread(id))
    }

    pub fn current_mut(&mut self) -> Option<&mut Thread<C>> {
        let id = self.current_thread?;
        Some(self.thread_mut(id))
    }

    /// the object representing a thread, created on first use so that every
    /// handle to the same thread names the same object.
    pub fn thread_object(&mut self, id: ThreadId) -> ObjectId {
        if let Some(&object) = self.thread_objects.get(&id) {
            return object;
        }
        let object = self.objects.insert(KObject::Thread(id));
        self.objects.add_ref(object); // Thread descriptor owns identity beyond guest handle closes.
        self.thread_objects.insert(id, object);
        object
    }

    pub fn process_object(&mut self) -> ObjectId {
        if let Some(object) = self.process_object {
            return object;
        }
        let object = self.objects.insert(KObject::Process);
        self.objects.add_ref(object); // Process lifetime owns its pseudo-handle identity.
        self.process_object = Some(object);
        object
    }

    /// registers a thread object so the guest can hold a handle to it.
    pub fn thread_handle(&mut self, id: ThreadId) -> Handle {
        let object = self.thread_object(id);
        self.handles.create(&mut self.objects, object, "Thread")
    }

    /// resolves a handle, including the two pseudo-handles that name the
    /// calling thread and its process.
    pub fn resolve(&mut self, handle: Handle) -> Option<ObjectId> {
        match handle {
            CURRENT_THREAD => {
                let id = self.current_thread?;
                Some(self.thread_object(id))
            }
            CURRENT_PROCESS => Some(self.process_object()),
            _ => self.handles.resolve(handle),
        }
    }

    // -- scheduling ---------------------------------------------------------

    /// wakes anything whose wait has been satisfied, then switches to the
    /// highest-priority runnable thread.
    pub fn schedule(&mut self, cpu: &mut C::Machine, tick: u64) -> bool {
        self.advance_timers(tick);
        for id in 0..self.threads.len() as ThreadId {
            self.try_satisfy_wait(id, tick);
        }

        if let Some(current) = self.current_thread {
            let thread = &mut self.threads[current as usize];
            if !thread.is_runnable() {
                thread.waited = true;
            }
        }
        let mut next = self.pick_next();
        self.all_blocked = next.is_none();
        // the console runs strictly by priority, but its threads block on
        // every service request while ours are answered at once, so a thread
        // spinning on work another does would hang. one that runs this long
        // without waiting shares the core with the others ready.
        if let Some(chosen) = next {
            let thread = &mut self.threads[chosen as usize];
            if thread.waited {
                thread.waited = false;
                thread.running_since = tick;
            }
            if tick.saturating_sub(thread.running_since) > STARVATION_TICKS {
                let priority = thread.priority;
                self.relief = !self.relief;
                if self.relief {
                    next = self.pick_other(chosen, priority).or(next);
                }
            }
        }
        if let Some(chosen) = next {
            let thread = &mut self.threads[chosen as usize];
            if thread.waited {
                thread.waited = false;
                thread.running_since = tick;
            }
        }

        match (self.current_thread, next) {
            (Some(current), Some(next)) if current == next => {
                self.threads[current as usize].status = ThreadStatus::Running;
                false
            }
            (current, Some(next)) => {
                if let Some(current) = current {
                    let thread = &mut self.threads[current as usize];
                    thread.context.save_from(cpu);
                    if thread.status == ThreadStatus::Running {
                        thread.status = ThreadStatus::Ready;
                    }
                }
                let thread = &mut self.threads[next as usize];
                thread.status = ThreadStatus::Running;
                thread.context.restore_to(cpu);
                self.current_thread = Some(next);
                true
            }
            (Some(current), None) => {
                // everything is blocked.
                let thread = &mut self.threads[current as usize];
                thread.context.save_from(cpu);
                if thread.status == ThreadStatus::Running {
                    thread.status = ThreadStatus::Ready;
                }
                self.current_thread = None;
                true
            }
            (None, None) => false,
        }
    }

    /// the most urgent runnable thread besides one, at its priority or below.
    fn pick_other(&self, chosen: ThreadId, priority: u32) -> Option<ThreadId> {
        self.threads
            .iter()
            .enumerate()
            .filter(|&(id, t)| id as ThreadId != chosen && t.is_runnable() && t.priority >= priority)
            .min_by_key(|(_, t)| t.priority)
            .map(|(id, _)| id as ThreadId)
    }

    /// the highest-priority runnable thread, round-robining within a priority
    /// by preferring the one after the current thread.
    fn pick_next(&self) -> Option<ThreadId> {
        let best = self
            .threads
            .iter()
            .filter(|t| t.is_runnable())
            .map(|t| t.priority)
            .min()?;

        // the running thread keeps the core unless something strictly more
        // urgent is ready, waking a thread of its own priority does not hand
        // over, only waiting or yielding does
        if let Some(current) = self.current_thread {
            let thread = &self.threads[current as usize];
            if thread.status == ThreadStatus::Running && thread.priority <= best {
                return Some(current);
            }
        }

        let count = self.threads.len();
        let start = self.current_thread.map_or(0, |id| id as usize + 1);
        (0..count)
            .map(|offset| (start + offset) % count)
            .find(|&index| {
                let t = &self.threads[index];
                t.is_runnable() && t.priority == best
            })
            .map(|index| index as ThreadId)
    }

    /// checks one blocked thread's condition and unblocks it if satisfied.
    fn try_satisfy_wait(&mut self, id: ThreadId, tick: u64) -> bool {
        let thread = &self.threads[id as usize];
        if !thread.status.is_blocked() {
            return false;
        }

        let timed_out = thread.wakeup_at.is_some_and(|at| tick >= at);

        match thread.status {
            ThreadStatus::Sleeping => {
                if timed_out {
                    let thread = &mut self.threads[id as usize];
                    thread.clear_wait();
                    thread.status = ThreadStatus::Ready;
                    return true;
                }
            }
            ThreadStatus::WaitSync => {
                let objects = thread.wait_objects.clone();
                let wait_all = thread.wait_all;

                let states: Vec<bool> = objects
                    .iter()
                    .map(|&object| self.is_signaled(object, id))
                    .collect();

                let satisfied = if wait_all {
                    states.iter().all(|&s| s)
                } else {
                    states.iter().any(|&s| s)
                };

                if satisfied {
                    let index = if wait_all {
                        for &object in &objects {
                            self.acquire(object, id);
                        }
                        0
                    } else {
                        let index = states.iter().position(|&s| s).unwrap();
                        self.acquire(objects[index], id);
                        index
                    };
                    let thread = &mut self.threads[id as usize];
                    thread.clear_wait();
                    thread.wait_result = Some(WaitResult::Signaled(index));
                    thread.status = ThreadStatus::Ready;
                    return true;
                }

                if timed_out {
                    let thread = &mut self.threads[id as usize];
                    thread.clear_wait();
                    thread.wait_result = Some(WaitResult::TimedOut);
                    thread.status = ThreadStatus::Ready;
                    return true;
                }
            }
            ThreadStatus::WaitArbiter
                // arbiter waits are released explicitly by a signalling
                // thread, only the timeout is handled here.
                if timed_out => {
                    let thread = &mut self.threads[id as usize];
                    thread.clear_wait();
                    thread.wait_result = Some(WaitResult::TimedOut);
                    thread.status = ThreadStatus::Ready;
                    return true;
                }
            _ => {}
        }
        false
    }

    pub fn is_signaled(&self,object:ObjectId,waiter:ThreadId)->bool{
        self.objects.get(object).is_some_and(|o|o.available(waiter,|id|self.threads.get(id as usize).is_some_and(|t|t.status==ThreadStatus::Dead)))
    }
    fn acquire(&mut self,object:ObjectId,waiter:ThreadId){
        self.objects.get_mut(object).expect("native wait object vanished").acquire(waiter);
    }
    pub fn acquire_public(&mut self,object:ObjectId,waiter:ThreadId){self.acquire(object,waiter);}
    /// blocks the current thread on a set of objects.
    pub fn begin_wait(
        &mut self,
        objects: Vec<ObjectId>,
        wait_all: bool,
        timeout_ticks: Option<u64>,
        tick: u64,
    ) {
        let Some(id) = self.current_thread else { return };
        self.waits+=1;
        let thread = &mut self.threads[id as usize];
        thread.wait_objects = objects;
        thread.wait_all = wait_all;
        thread.wakeup_at = timeout_ticks.map(|t| tick.saturating_add(t));
        thread.wait_result = None;
        thread.status = ThreadStatus::WaitSync;
        self.reschedule_pending = true;
    }

    /// whether a thread besides the current one could run.
    pub fn others_ready(&self) -> bool {
        self.threads
            .iter()
            .enumerate()
            .any(|(id, thread)| Some(id as ThreadId) != self.current_thread && thread.is_runnable())
    }

    pub fn sleep_current(&mut self, ticks: u64, tick: u64) {
        self.waits+=1;
        let Some(id) = self.current_thread else { return };
        let thread = &mut self.threads[id as usize];
        thread.clear_wait();
        thread.wakeup_at = Some(tick.saturating_add(ticks));
        thread.status = ThreadStatus::Sleeping;
        self.reschedule_pending = true;
    }

    /// signals an event, waking anything waiting on it at the next scheduling
    /// point.
    pub fn signal_event(&mut self, object: ObjectId) {
        if let Some(KObject::Event(event)) = self.objects.get_mut(object) {
            event.signaled = true;
            self.reschedule_pending = true;
        }
    }

    pub fn clear_event(&mut self, object: ObjectId) {
        if let Some(KObject::Event(event)) = self.objects.get_mut(object) {
            event.signaled = false;
        }
    }

    /// creates an event object and a handle for it in one step, which is what
    /// nearly every caller wants.
    pub fn create_event(&mut self, reset_type: ResetType, name: &str) -> (ObjectId, Handle) {
        let id = self
            .objects
            .insert(KObject::Event(Event::new(reset_type, name)));
        let handle = self.handles.create(&mut self.objects, id, name);
        (id, handle)
    }

    /// describes what a blocked thread is waiting for, for diagnostics.
    pub fn describe_wait(&self, id: ThreadId) -> String {
        let thread = self.thread(id);
        match thread.status {
            ThreadStatus::Sleeping => match thread.wakeup_at {
                Some(at) => format!("sleeping until tick {at}"),
                None => "sleeping forever".into(),
            },
            ThreadStatus::WaitArbiter => format!(
                "arbiter on 0x{:08X}",
                thread.wait_address.unwrap_or(0)
            ),
            ThreadStatus::WaitSync => {
                let objects: Vec<String> = thread
                    .wait_objects
                    .iter()
                    .map(|&object| {
                        let label = self
                            .handles
                            .iter()
                            .find(|(_, id)| *id == object)
                            .map(|(handle, _)| self.handles.label(handle).to_owned())
                            .unwrap_or_else(|| "?".into());
                        let kind = self
                            .objects
                            .get(object)
                            .map_or("missing", |o| o.type_name());
                        let signalled = self.is_signaled(object, id);
                        format!("{label}:{kind}{}", if signalled { "(ready)" } else { "" })
                    })
                    .collect();
                format!(
                    "waiting for {} of [{}]",
                    if thread.wait_all { "all" } else { "any" },
                    objects.join(", ")
                )
            }
            other => format!("{other:?}"),
        }
    }

    /// number of threads that are not dead, for the diagnostics overlay.
    pub fn live_thread_count(&self) -> usize {
        self.threads
            .iter()
            .filter(|t| t.status != ThreadStatus::Dead)
            .count()
    }
    pub fn next_wakeup(&self)->Option<u64>{
        self.threads.iter().filter(|t|t.status.is_blocked()).filter_map(|t|t.wakeup_at)
            .chain(self.objects.iter().filter_map(|(_,o)|if let KObject::Timer(t)=o{t.fire_at}else{None})).min()
    }
    pub fn advance_timers(&mut self,tick:u64){
        let due:Vec<_>=self.objects.iter().filter_map(|(id,o)|if let KObject::Timer(t)=o{t.fire_at.filter(|&at|at<=tick).map(|_|id)}else{None}).collect();
        for id in due{if let Some(KObject::Timer(t))=self.objects.get_mut(id){
            t.signaled=true;t.fire_at=if t.interval==0{None}else{
                let at=t.fire_at.unwrap();let periods=(tick-at)/t.interval+1;
                periods.checked_mul(t.interval).and_then(|n|at.checked_add(n))
            };self.reschedule_pending=true;
        }}
    }
}
impl<C:ExecutionContext,R> Drop for NativeScheduler<C,R>{fn drop(&mut self){if !self.threads.is_empty(){
    let timers=self.objects.iter().filter(|(_,o)|matches!(o,KObject::Timer(_))).count();
    log::info!("NATIVE_SCHEDULER_COUNTERS native_threads={} native_waits={} native_timers={} zakuro_scheduler_calls=0",self.threads.len(),self.waits,timers);
}}}
#[cfg(test)]mod tests{
    use super::*;
    #[derive(Clone)]struct Context(u32);
    impl ExecutionContext for Context{type Machine=u32;
        fn new(entry:u32,_:u32,_:u32,_:u32)->Self{Self(entry)}
        fn save_from(&mut self,v:&u32){self.0=*v;}
        fn restore_to(&self,v:&mut u32){*v=self.0;}
    }
    fn scheduler()->NativeScheduler<Context,()>{NativeScheduler::new(1,(),0x14000000,ThreadProfile{tls_base:0x1ff82000,tls_stride:0x200,heap_base:0x08000000})}
    #[test]fn context_tls_wait_any_and_timeout(){let mut s=scheduler();let a=s.create_thread("a",0x1000,0x8000,0,10,0);let b=s.create_thread("b",0x2000,0x9000,0,11,0);
        assert_ne!(s.thread(a).tls,s.thread(b).tls);let mut cpu=0;s.schedule(&mut cpu,0);assert_eq!(cpu,0x1000);
        let(e,_)=s.create_event(ResetType::OneShot,"e");s.begin_wait(vec![e],false,Some(10),0);s.schedule(&mut cpu,1);assert_eq!(s.current_thread,Some(b));
        s.signal_event(e);s.schedule(&mut cpu,2);assert_eq!(s.current_thread,Some(a));assert_eq!(s.thread(a).wait_result,Some(WaitResult::Signaled(0)));assert!(!s.is_signaled(e,a));
        s.begin_wait(vec![e],false,Some(5),2);s.schedule(&mut cpu,7);assert_eq!(s.thread(a).wait_result,Some(WaitResult::TimedOut));
        let o=s.thread_object(a);let h=s.thread_handle(a);s.handles.close(&mut s.objects,h);assert_eq!(s.thread_object(a),o);assert!(s.objects.get(o).is_some());
    }
    #[test]fn wait_all_and_periodic_timer_phase(){let mut s=scheduler();let a=s.create_thread("a",1,8,0,10,0);let mut cpu=0;s.schedule(&mut cpu,0);
        let(e,_)=s.create_event(ResetType::OneShot,"e");let(t,_)=s.create_event(ResetType::Sticky,"t");s.signal_event(e);s.begin_wait(vec![e,t],true,None,0);s.schedule(&mut cpu,1);assert!(s.current_thread.is_none());assert!(s.is_signaled(e,a));
        s.signal_event(t);s.schedule(&mut cpu,2);assert_eq!(s.current_thread,Some(a));assert!(!s.is_signaled(e,a));assert!(s.is_signaled(t,a));
        let timer=s.objects.insert(KObject::Timer(Timer{name:"timer".into(),reset_type:ResetType::Pulse,signaled:false,fire_at:Some(10),interval:7}));
        s.advance_timers(26);if let Some(KObject::Timer(t))=s.objects.get(timer){assert_eq!(t.fire_at,Some(31));assert!(t.signaled);}else{panic!();}
    }
}
