// SPDX-License-Identifier: MIT
// Copyright (c) 2026 fearkov. Native continuation descriptor extraction.
// No CPU/emulator dependency; register context is an explicit generic contract.
use crate::{GuestAddress as VAddr,kernel::{ThreadId,ObjectId},scheduler::ExecutionContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadStatus {
    Running,
    Ready,
    /// sleeping until a tick, with no object involved.
    Sleeping,
    /// waiting on one or more kernel objects.
    WaitSync,
    /// parked on an address arbiter.
    WaitArbiter,
    /// created but not started.
    Dormant,
    Dead,
}

impl ThreadStatus {
    pub fn is_blocked(self) -> bool {
        matches!(
            self,
            ThreadStatus::Sleeping | ThreadStatus::WaitSync | ThreadStatus::WaitArbiter
        )
    }
}

#[derive(Debug, Clone)]
pub struct Thread<C:ExecutionContext> {
    pub id: ThreadId,
    pub name: String,
    pub context: C,
    /// 0 is the highest priority, 0x3F the lowest.
    pub priority: u32,
    pub status: ThreadStatus,

    pub entry: VAddr,
    pub stack_top: VAddr,
    pub tls: VAddr,
    pub processor_id: i32,

    /// objects this thread is waiting on, and whether all of them are needed.
    pub wait_objects: Vec<ObjectId>,
    pub wait_all: bool,
    /// tick at which a timeout expires. None means wait forever.
    pub wakeup_at: Option<u64>,
    /// the tick the thread last started running after a wait, and whether
    /// it has waited since.
    pub running_since: u64,
    pub waited: bool,
    /// address this thread is parked on, for arbiter waits.
    pub wait_address: Option<VAddr>,
    pub wait_arbiter:Option<ObjectId>,
    /// set when the wait was satisfied, so the syscall knows what to return.
    pub wait_result: Option<WaitResult>,
    /// which syscall is waiting for that result.
    pub wait_syscall: Option<WaitSyscall>,

    /// handed to svcGetThreadId.
    pub guest_id: u32,
}

/// which syscall put a thread to sleep, so the scheduler knows how to write
/// the result back when it wakes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitSyscall {
    WaitSynchronization1,
    WaitSynchronizationN,
    ArbitrateAddress,
    SleepThread,
}

/// why a blocked thread woke up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitResult {
    /// the wait succeeded, the value is the index of the object that did it.
    Signaled(usize),
    TimedOut,
}

impl<C:ExecutionContext> Thread<C> {
    // every one of these comes from svcCreateThread's arguments, so bundling
    // them into a struct would only move the same list somewhere else.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ThreadId,
        name: impl Into<String>,
        entry: VAddr,
        stack_top: VAddr,
        arg: u32,
        priority: u32,
        tls: VAddr,
        processor_id: i32,
    ) -> Thread<C> {
        Thread {
            id,
            name: name.into(),
            context: C::new(entry, stack_top, arg, tls),
            priority,
            status: ThreadStatus::Ready,
            entry,
            stack_top,
            tls,
            processor_id,
            wait_objects: Vec::new(),
            wait_all: false,
            wakeup_at: None,
            running_since: 0,
            waited: true,
            wait_address: None,
            wait_arbiter:None,
            wait_result: None,
            wait_syscall: None,
            guest_id: id,
        }
    }

    pub fn is_runnable(&self) -> bool {
        matches!(self.status, ThreadStatus::Ready | ThreadStatus::Running)
    }

    /// clears everything a wait left behind.
    pub fn clear_wait(&mut self) {
        self.wait_objects.clear();
        self.wait_all = false;
        self.wakeup_at = None;
        self.wait_address = None;
        self.wait_arbiter=None;
    }
}

