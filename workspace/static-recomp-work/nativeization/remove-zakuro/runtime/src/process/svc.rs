//! Native CTR register ABI dispatch over native typed objects. Architecture:
//! TriAevum CtrIpcRouter CreateEvent/ClearEvent/OpenKernelObject contracts.
//! Unhandled operations are explicit transitional routes, never successes.
use crate::{kernel::{KObject,ResetType,CURRENT_THREAD,AddressArbiter,ArbitrationType},scheduler::{ExecutionContext,NativeScheduler,ThreadStatus,WaitSyscall,WaitResult,CPU_CLOCK_HZ}};
use std::sync::atomic::{AtomicU64,Ordering::Relaxed};
const INVALID_HANDLE:u32=0xd8e007f7;
const INVALID_ENUM:u32=0xd8e00405;
static NATIVE:AtomicU64=AtomicU64::new(0);
static TRANSITIONAL:AtomicU64=AtomicU64::new(0);
static REMAINING:[AtomicU64;128]=[const{AtomicU64::new(0)};128];
pub fn counters()->(u64,u64){(NATIVE.load(Relaxed),TRANSITIONAL.load(Relaxed))}
pub trait ThreadMemory{fn map_tls(&mut self,tls:u32)->Result<(),u32>;}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct MemoryInfo{pub base:u32,pub size:u32,pub permission:u32,pub state:u32}
pub trait QueryMemory{fn query_memory(&self,address:u32)->MemoryInfo;}
pub trait AllocationMemory<R>{
    fn allocate_region(&mut self,region:&R,size:u32)->Result<u32,u32>;
    fn install_allocation(&mut self,address:u32,physical:u32,size:u32,permission:u32,linear:bool);
}
pub fn control_memory<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16],memory:&mut impl AllocationMemory<R>){
    NATIVE.fetch_add(1,Relaxed);
    let operation=registers[0];let size=registers[3];let permission=registers[4];
    if size==0||size&4095!=0{registers[0]=0xe0e01bf2;return;}
    if operation&255!=3||operation&!0x100ff!=0{log::warn!("native ControlMemory unsupported operation=0x{operation:x}");registers[0]=INVALID_ENUM;return;}
    let linear=operation&0x10000!=0;
    let requested=registers[1];
    if requested&4095!=0{registers[0]=0xe0e01bf1;return;}
    let address=if requested==0{kernel.heap_top}else{requested};
    if !linear&&address.checked_add(size).is_none(){registers[0]=0xe0e01bf5;return;}
    let physical=match memory.allocate_region(&kernel.memory_region,size){Ok(p)=>p,Err(result)=>{registers[0]=result;return;}};
    let address=if linear{kernel.linear_base+(physical-0x20000000)}else{address};
    memory.install_allocation(address,physical,size,permission,linear);
    if linear{kernel.linear_top=kernel.linear_top.max(address+size);}else{kernel.heap_top=kernel.heap_top.max(address+size);}
    registers[0]=0;registers[1]=address;
}
pub fn query_memory(registers:&mut [u32;16],memory:&impl QueryMemory){
    NATIVE.fetch_add(1,Relaxed);let info=memory.query_memory(registers[2]);
    registers[0]=0;registers[1]=info.base;registers[2]=info.size;
    registers[3]=info.permission;registers[4]=info.state;registers[5]=0;
}
pub trait SharedBacking{fn shared_backing(&mut self,address:u32,size:u32)->Result<u32,u32>;}
pub trait SharedMapping{fn map_shared(&mut self,address:u32,physical:u32,size:u32,permission:u32)->Result<u32,u32>;}
pub fn resource_current_values<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16],names:&[u32],committed_bytes:u64)->Result<Vec<u64>,u32>{
    NATIVE.fetch_add(1,Relaxed);
    let object=kernel.resolve(registers[1]).ok_or(INVALID_HANDLE)?;
    if !matches!(kernel.objects.get(object),Some(KObject::ResourceLimit)){return Err(INVALID_HANDLE);}
    names.iter().map(|&name|Ok(match name{
        1=>committed_bytes,2=>kernel.live_thread_count()as u64,
        3..=8=>kernel.objects.iter().filter(|(_,object)|matches!((name,object),(3,KObject::Event(_))|(4,KObject::Mutex(_))|(5,KObject::Semaphore(_))|(6,KObject::Timer(_))|(7,KObject::SharedMemory(_))|(8,KObject::AddressArbiter(_)))).count()as u64,
        _=>return Err(INVALID_ENUM),
    })).collect()
}
pub fn process_info<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16],committed_bytes:u64){
    NATIVE.fetch_add(1,Relaxed);
    let Some(object)=kernel.resolve(registers[1])else{registers[0]=INVALID_HANDLE;return;};
    if !matches!(kernel.objects.get(object),Some(KObject::Process)){registers[0]=INVALID_HANDLE;return;}
    let value=match registers[2]{2=>committed_bytes,20=>0x20000000u32.wrapping_sub(kernel.linear_base)as u64,_=>{registers[0]=INVALID_ENUM;return;}};
    registers[0]=0;registers[1]=value as u32;registers[2]=(value>>32)as u32;
}
pub fn connect_to_port<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16],name:&str){
    NATIVE.fetch_add(1,Relaxed);
    if name!="srv:"{registers[0]=0xd8806407;return;}
    let object=kernel.objects.insert(KObject::ClientPort(name.into()));
    registers[1]=kernel.handles.create(&mut kernel.objects,object,name);registers[0]=0;
}
pub fn map_memory_block<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16],memory:&mut impl SharedMapping)->Option<(String,u32,u32)>{
    NATIVE.fetch_add(1,Relaxed);
    let Some(object)=kernel.resolve(registers[0])else{registers[0]=INVALID_HANDLE;return None;};
    let Some(KObject::SharedMemory(block))=kernel.objects.get(object)else{registers[0]=INVALID_HANDLE;return None;};
    let physical=block.paddr;let size=block.size;let name=block.name.clone();
    let address=match memory.map_shared(registers[1],physical,size,registers[2]){Ok(address)=>address,Err(result)=>{registers[0]=result;return None;}};
    if let Some(KObject::SharedMemory(block))=kernel.objects.get_mut(object){block.mapped_at=Some(address);}
    registers[0]=0;Some((name,address,physical))
}
pub fn create_memory_block<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16],memory:&mut impl SharedBacking){
    NATIVE.fetch_add(1,Relaxed);
    let address=registers[1];let size=registers[2];
    let paddr=match memory.shared_backing(address,size){Ok(paddr)=>paddr,Err(result)=>{registers[0]=result;return;}};
    let object=kernel.objects.insert(KObject::SharedMemory(crate::kernel::SharedMemory{name:format!("MemoryBlock@{address:08x}"),address,size,paddr,mapped_at:None}));
    let handle=kernel.handles.create(&mut kernel.objects,object,"MemoryBlock");registers[0]=0;registers[1]=handle;
}
pub fn create_thread<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16],memory:&mut impl ThreadMemory){
    NATIVE.fetch_add(1,Relaxed);
    let id=kernel.create_thread(format!("thread{}",kernel.threads.len()),registers[1],registers[3],registers[2],registers[0],registers[4]as i32);
    if let Err(result)=memory.map_tls(kernel.thread(id).tls){kernel.thread_mut(id).status=ThreadStatus::Dead;registers[0]=result;return;}
    let handle=kernel.thread_handle(id);registers[0]=0;registers[1]=handle;
}
/// Independent CTR arbiter implementation. Behavioral reference: Azahar
/// address_arbiter.cpp (GPL-2.0-or-later); no source expression copied.
pub fn arbitrate<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16],tick:u64,memory:&mut impl crate::ipc::message::IpcMemory){
    NATIVE.fetch_add(1,Relaxed);
    let Some(kind)=ArbitrationType::from_raw(registers[2])else{registers[0]=INVALID_ENUM;return;};
    let Some(object)=kernel.resolve(registers[0]).filter(|o|matches!(kernel.objects.get(*o),Some(KObject::AddressArbiter(_))))else{registers[0]=INVALID_HANDLE;return;};
    let address=registers[1];let value=registers[3]as i32;
    registers[0]=if kind.has_timeout(){0x09401bfe}else{0};
    if kind==ArbitrationType::Signal{
        let candidates=if let Some(KObject::AddressArbiter(a))=kernel.objects.get(object){a.waiters.clone()}else{unreachable!()};
        let mut valid:Vec<_>=candidates.into_iter().filter(|&(id,_)|kernel.threads.get(id as usize).is_some_and(|t|t.status==ThreadStatus::WaitArbiter&&t.wait_arbiter==Some(object))).collect();
        let mut waking:Vec<_>=valid.iter().filter(|(_,a)|*a==address).map(|(id,_)|*id).collect();
        waking.sort_by_key(|&id|kernel.thread(id).priority);
        if value>=0{waking.truncate(value as usize);}
        valid.retain(|(id,_)|!waking.contains(id));
        if let Some(KObject::AddressArbiter(a))=kernel.objects.get_mut(object){a.waiters=valid;}
        for id in waking{let thread=kernel.thread_mut(id);thread.clear_wait();thread.wait_result=Some(WaitResult::Signaled(0));thread.status=ThreadStatus::Ready;}
        kernel.reschedule_pending=true;return;
    }
    let word=memory.read_word(address)as i32;if word>=value{return;}
    if kind.decrements(){memory.write_word(address,word.wrapping_sub(1)as u32);}
    let Some(id)=kernel.current_thread else{return;};
    if let Some(KObject::AddressArbiter(a))=kernel.objects.get_mut(object){a.waiters.retain(|(t,_)|*t!=id);a.waiters.push((id,address));}
    let timeout=(registers[4]as u64|((registers[5]as u64)<<32))as i64;
    let thread=kernel.thread_mut(id);thread.clear_wait();thread.wait_address=Some(address);thread.wait_arbiter=Some(object);
    thread.wakeup_at=if kind.has_timeout()&&timeout>=0{Some(tick.saturating_add(nanos_to_ticks(timeout as u64)))}else{None};
    thread.status=ThreadStatus::WaitArbiter;thread.wait_syscall=Some(WaitSyscall::ArbitrateAddress);kernel.reschedule_pending=true;
}
pub fn remaining_calls()->Vec<(u32,u64)>{REMAINING.iter().enumerate().filter_map(|(number,counter)|{let count=counter.load(Relaxed);(count!=0).then_some((number as u32,count))}).collect()}
pub fn session_target<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16])->Option<crate::ipc::Target>{
    NATIVE.fetch_add(1,Relaxed);
    let target=kernel.resolve(registers[0]).and_then(|object|match kernel.objects.get(object){
        Some(KObject::ClientPort(name))=>Some(crate::ipc::Target::port(name.clone())),
        Some(KObject::ClientSession(session))=>Some(crate::ipc::Target::service(session.service.clone(),session.subhandle)),
        _=>None,
    });
    registers[0]=if target.is_some(){0}else{INVALID_HANDLE};target
}
fn nanos_to_ticks(nanos:u64)->u64{(nanos/1_000_000_000)*CPU_CLOCK_HZ+(nanos%1_000_000_000)*CPU_CLOCK_HZ/1_000_000_000}
pub fn wait<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16],tick:u64,handles:&[u32],wait_all:bool,timeout:i64,syscall:WaitSyscall){
    NATIVE.fetch_add(1,Relaxed);
    let mut objects=Vec::with_capacity(handles.len());
    for &handle in handles{
        let Some(object)=kernel.resolve(handle)else{registers[0]=INVALID_HANDLE;return;};
        if !matches!(kernel.objects.get(object),Some(KObject::Event(_)|KObject::Mutex(_)|KObject::Semaphore(_)|KObject::Timer(_)|KObject::Thread(_)|KObject::ClientSession(_)|KObject::ClientPort(_))){registers[0]=INVALID_HANDLE;return;}
        objects.push(object);
    }
    let Some(id)=kernel.current_thread else{registers[0]=0;return;};
    let states:Vec<bool>=objects.iter().map(|&o|kernel.is_signaled(o,id)).collect();
    if if wait_all{states.iter().all(|s|*s)}else{states.iter().any(|s|*s)}{
        let index=if wait_all{for &o in &objects{kernel.acquire_public(o,id);}0}else{
            let index=states.iter().position(|s|*s).unwrap();kernel.acquire_public(objects[index],id);index
        };
        registers[0]=0;if syscall==WaitSyscall::WaitSynchronizationN{registers[1]=index as u32;}return;
    }
    if timeout==0{registers[0]=0x09401bfe;return;}
    kernel.begin_wait(objects,wait_all,(timeout>0).then(||nanos_to_ticks(timeout as u64)),tick);
    kernel.thread_mut(id).wait_syscall=Some(syscall);
}
/// Marshal guest wait registers through checked native memory, not host slices
/// or application-owned emulator ABI types.
pub fn wait_registers<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16],tick:u64,memory:&mut impl crate::services::router::ServiceMemory,number:u32)->bool{
    let (handles,all,timeout,syscall)=match number{
        0x24=>(vec![registers[0]],false,(registers[2]as u64|((registers[3]as u64)<<32))as i64,WaitSyscall::WaitSynchronization1),
        0x25=>{
            let count=registers[2]as usize;
            if count>64{registers[0]=0xe0e01bfd;return true;}
            let mut bytes=vec![0;count*4];
            if !memory.read_span(registers[1],&mut bytes){registers[0]=0xe0e01bf5;return true;}
            let handles=bytes.chunks_exact(4).map(|word|u32::from_le_bytes(word.try_into().unwrap())).collect();
            (handles,registers[3]!=0,(registers[0]as u64|((registers[4]as u64)<<32))as i64,WaitSyscall::WaitSynchronizationN)
        }
        _=>return false,
    };
    wait(kernel,registers,tick,&handles,all,timeout,syscall);true
}
pub fn apply_wait_result<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16]){
    let Some(id)=kernel.current_thread else{return;};let thread=kernel.thread_mut(id);
    let(Some(result),Some(syscall))=(thread.wait_result.take(),thread.wait_syscall.take())else{return;};
    match(syscall,result){
        (WaitSyscall::ArbitrateAddress,WaitResult::Signaled(_))=>{},
        (WaitSyscall::WaitSynchronizationN,WaitResult::Signaled(index))=>{registers[0]=0;registers[1]=index as u32;}
        (_,WaitResult::TimedOut)=>registers[0]=0x09401bfe,
        _=>registers[0]=0,
    }
}
fn thread<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,handle:u32)->Option<u32>{
    if handle==CURRENT_THREAD{return kernel.current_thread;}
    match kernel.objects.get(kernel.handles.resolve(handle)?){Some(KObject::Thread(id))=>Some(*id),_=>None}
}
pub fn dispatch<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16],tick:u64,exited:&mut bool,number:u32)->bool{
    match number{
        0x38=>{
            let valid=registers[1]==crate::kernel::CURRENT_PROCESS||kernel.resolve(registers[1]).is_some_and(|object|matches!(kernel.objects.get(object),Some(KObject::Process)));
            if !valid{registers[0]=INVALID_HANDLE;}else{
                let object=kernel.objects.insert(KObject::ResourceLimit);
                registers[1]=kernel.handles.create(&mut kernel.objects,object,"ResourceLimit");registers[0]=0;
            }
        }
        0x13=>{
            let mut mutex=crate::kernel::Mutex::new("Mutex");
            if registers[1]!=0{mutex.owner=kernel.current_thread;mutex.lock_count=1;}
            let object=kernel.objects.insert(KObject::Mutex(mutex));
            registers[1]=kernel.handles.create(&mut kernel.objects,object,"Mutex");registers[0]=0;
        }
        0x14=>{
            let Some(object)=kernel.resolve(registers[0])else{registers[0]=INVALID_HANDLE;return true;};
            let Some(KObject::Mutex(mutex))=kernel.objects.get_mut(object)else{registers[0]=INVALID_HANDLE;return true;};
            mutex.lock_count=mutex.lock_count.saturating_sub(1);
            if mutex.lock_count==0{mutex.owner=None;kernel.reschedule_pending=true;}
            registers[0]=0;
        }
        0x21=>{
            let object=kernel.objects.insert(KObject::AddressArbiter(AddressArbiter{name:"Arbiter".into(),waiters:Vec::new()}));
            let handle=kernel.handles.create(&mut kernel.objects,object,"Arbiter");registers[0]=0;registers[1]=handle;
        }
        0x03=>{
            for t in &mut kernel.threads{t.status=ThreadStatus::Dead;t.clear_wait();}
            kernel.current_thread=None;kernel.reschedule_pending=true;*exited=true;
        }
        0x09=>{
            if let Some(t)=kernel.current_mut(){t.status=ThreadStatus::Dead;t.clear_wait();}
            kernel.current_thread=None;kernel.reschedule_pending=true;
        }
        0x0a=>{
            let nanos=registers[0]as u64|((registers[1]as u64)<<32);
            let ticks=if nanos!=0{nanos_to_ticks(nanos)}else if kernel.others_ready(){20_000}else{0};
            kernel.sleep_current(ticks,tick);registers[0]=0;
        }
        0x0b|0x0c|0x37=>{
            let handle=if number==0x0c{registers[0]}else{registers[1]};
            if let Some(id)=thread(kernel,handle){
                if number==0x0c{kernel.thread_mut(id).priority=registers[1].min(0x3f);kernel.reschedule_pending=true;}
                else{registers[1]=if number==0x37{kernel.thread(id).guest_id}else{kernel.thread(id).priority};}
                registers[0]=0;
            }else{registers[0]=INVALID_HANDLE;}
        }
        0x28=>{registers[0]=tick as u32;registers[1]=(tick>>32)as u32;}
        _=>return dispatch_objects(kernel,registers,number),
    }
    NATIVE.fetch_add(1,Relaxed);true
}
pub fn dispatch_objects<C:ExecutionContext,R>(kernel:&mut NativeScheduler<C,R>,registers:&mut [u32;16],number:u32)->bool{
    match number{
        0x17=>{
            if registers[1]>2{registers[0]=INVALID_ENUM;}else{
                let (_,handle)=kernel.create_event(ResetType::from_raw(registers[1]),"Event");
                registers[0]=0;registers[1]=handle;
            }
        }
        0x18|0x19=>{
            let object=kernel.resolve(registers[0]);
            if let Some(object)=object.filter(|o|matches!(kernel.objects.get(*o),Some(KObject::Event(_)))){
                if number==0x18{kernel.signal_event(object);}else{kernel.clear_event(object);}
                registers[0]=0;
            }else{registers[0]=INVALID_HANDLE;}
        }
        0x23=>{
            registers[0]=if kernel.handles.close(&mut kernel.objects,registers[0]){0}else{INVALID_HANDLE};
        }
        0x27=>{
            let handle=registers[1];
            if let Some(object)=kernel.resolve(handle){
                let label=kernel.handles.label(handle).to_owned();
                let new_handle=kernel.handles.duplicate_object(&mut kernel.objects,object,&label);
                registers[0]=0;registers[1]=new_handle;
            }else{registers[0]=INVALID_HANDLE;}
        }
        _=>{
            TRANSITIONAL.fetch_add(1,Relaxed);
            if let Some(count)=REMAINING.get(number as usize){if count.fetch_add(1,Relaxed)==0{log::info!("NATIVE_SVC_TRANSITIONAL number=0x{number:02x}");}}
            return false;
        }
    }
    NATIVE.fetch_add(1,Relaxed);true
}
#[cfg(test)]mod tests{
    use super::*;
    #[derive(Clone)]struct Context;
    impl ExecutionContext for Context{type Machine=();fn new(_:u32,_:u32,_:u32,_:u32)->Self{Self}fn save_from(&mut self,_:&()){}fn restore_to(&self,_:&mut ()){} }
    fn kernel()->NativeScheduler<Context,()>{NativeScheduler::new(1,(),0,crate::scheduler::ThreadProfile{tls_base:0,tls_stride:512,heap_base:0})}
    #[test]fn shared_mapping_updates_only_successful_native_object(){
        struct Mapping(bool);
        impl SharedMapping for Mapping{fn map_shared(&mut self,address:u32,physical:u32,size:u32,permission:u32)->Result<u32,u32>{assert_eq!((address,physical,size,permission),(0x8000,0x20000000,4096,3));if self.0{Ok(address)}else{Err(0xe0e01bf5)}}}
        let mut k=kernel();let object=k.objects.insert(KObject::SharedMemory(crate::kernel::SharedMemory{name:"HID".into(),address:0,size:4096,paddr:0x20000000,mapped_at:None}));
        let handle=k.handles.create(&mut k.objects,object,"HID");let mut r=[0;16];r[0]=handle;r[1]=0x8000;r[2]=3;
        assert!(map_memory_block(&mut k,&mut r,&mut Mapping(false)).is_none());assert_eq!(r[0],0xe0e01bf5);
        assert!(matches!(k.objects.get(object),Some(KObject::SharedMemory(b))if b.mapped_at.is_none()));
        r[0]=handle;assert_eq!(map_memory_block(&mut k,&mut r,&mut Mapping(true)),Some(("HID".into(),0x8000,0x20000000)));assert_eq!(r[0],0);
    }
    #[test]fn event_handle_alias_and_last_close(){
        let mut k=kernel();let mut r=[0;16];r[1]=1;
        assert!(dispatch_objects(&mut k,&mut r,0x17));assert_eq!(r[0],0);let a=r[1];let object=k.resolve(a).unwrap();
        dispatch_objects(&mut k,&mut r,0x27);let b=r[1];assert_ne!(a,b);assert_eq!(k.resolve(b),Some(object));
        r[0]=a;dispatch_objects(&mut k,&mut r,0x18);assert!(matches!(k.objects.get(object),Some(KObject::Event(e)) if e.signaled));
        r[0]=b;dispatch_objects(&mut k,&mut r,0x19);assert!(matches!(k.objects.get(object),Some(KObject::Event(e)) if !e.signaled));
        r[0]=a;dispatch_objects(&mut k,&mut r,0x23);assert_eq!(r[0],0);assert!(k.objects.get(object).is_some());
        r[0]=b;dispatch_objects(&mut k,&mut r,0x23);assert!(k.objects.get(object).is_none());
    }
    #[test]fn resource_limit_requires_native_process_identity(){
        let mut k=kernel();let mut r=[0;16];let mut exited=false;
        r[1]=123;assert!(dispatch(&mut k,&mut r,0,&mut exited,0x38));assert_eq!(r[0],INVALID_HANDLE);
        r[1]=crate::kernel::CURRENT_PROCESS;dispatch(&mut k,&mut r,0,&mut exited,0x38);assert_eq!(r[0],0);
        let object=k.resolve(r[1]).unwrap();assert!(matches!(k.objects.get(object),Some(KObject::ResourceLimit)));
    }
    #[test]fn native_port_profile_rejects_unregistered_endpoints(){
        let mut k=kernel();let mut r=[0;16];let before=k.handles.len();
        connect_to_port(&mut k,&mut r,"unknown");assert_eq!(r[0],0xd8806407);assert_eq!(k.handles.len(),before);
        connect_to_port(&mut k,&mut r,"srv:");assert_eq!(r[0],0);
        let target=session_target(&mut k,&mut [r[1],0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]).unwrap();assert_eq!(target.name(),"srv:");
    }
    #[test]fn process_info_validates_identity_and_preserves_u64(){
        let mut k=kernel();let mut r=[0;16];r[1]=crate::kernel::CURRENT_PROCESS;r[2]=2;
        process_info(&mut k,&mut r,0x123456789);assert_eq!((r[0],r[1],r[2]),(0,0x23456789,1));
        r[1]=crate::kernel::CURRENT_PROCESS;r[2]=20;process_info(&mut k,&mut r,0);assert_eq!(r[1],0x20000000u32.wrapping_sub(k.linear_base));
        r[1]=crate::kernel::CURRENT_PROCESS;r[2]=999;process_info(&mut k,&mut r,0);assert_eq!(r[0],INVALID_ENUM);
        r[1]=123;r[2]=2;process_info(&mut k,&mut r,0);assert_eq!(r[0],INVALID_HANDLE);
    }
    #[test]fn resource_queries_use_native_counts_and_reject_unsupported(){
        let mut k=kernel();let mut r=[0;16];let object=k.objects.insert(KObject::ResourceLimit);r[1]=k.handles.create(&mut k.objects,object,"limit");
        k.objects.insert(KObject::Event(crate::kernel::Event{name:"event".into(),reset_type:ResetType::Sticky,signaled:false}));
        assert_eq!(resource_current_values(&mut k,&mut r,&[1,3],0x123456789),Ok(vec![0x123456789,1]));
        assert_eq!(resource_current_values(&mut k,&mut r,&[99],0),Err(INVALID_ENUM));
        r[1]=123;assert_eq!(resource_current_values(&mut k,&mut r,&[1],0),Err(INVALID_HANDLE));
    }
    #[test]fn query_memory_preserves_native_mapping_register_abi(){
        struct Mapping;
        impl QueryMemory for Mapping{fn query_memory(&self,address:u32)->MemoryInfo{assert_eq!(address,0x1234);MemoryInfo{base:0x1000,size:0x4000,permission:3,state:5}}}
        let mut r=[0;16];r[2]=0x1234;r[5]=99;query_memory(&mut r,&Mapping);assert_eq!(&r[..6],&[0,0x1000,0x4000,3,5,0]);
    }
    #[test]fn control_memory_owns_heap_and_linear_allocation_contract(){
        #[derive(Default)]struct Memory{calls:Vec<(u32,u32,u32,u32,bool)>}
        impl AllocationMemory<()> for Memory{
            fn allocate_region(&mut self,_:&(),_:u32)->Result<u32,u32>{Ok(0x20004000)}
            fn install_allocation(&mut self,a:u32,p:u32,n:u32,perm:u32,linear:bool){self.calls.push((a,p,n,perm,linear));}
        }
        let mut k=kernel();k.heap_top=0x8000;k.linear_base=0x14000000;let mut memory=Memory::default();let mut r=[0;16];r[0]=3;r[3]=4096;r[4]=3;
        control_memory(&mut k,&mut r,&mut memory);assert_eq!((r[0],r[1],k.heap_top),(0,0x8000,0x9000));
        r[0]=0x10003;r[1]=0;control_memory(&mut k,&mut r,&mut memory);assert_eq!((r[0],r[1],k.linear_top),(0,0x14004000,0x14005000));assert_eq!(memory.calls.len(),2);
        r[0]=3;r[3]=1;control_memory(&mut k,&mut r,&mut memory);assert_eq!(r[0],0xe0e01bf2);assert_eq!(memory.calls.len(),2);
    }
    #[test]fn bad_type_and_unhandled_are_not_success(){
        let mut k=kernel();let mut r=[0;16];let object=k.objects.insert(KObject::Process);
        r[0]=k.handles.create(&mut k.objects,object,"p");dispatch_objects(&mut k,&mut r,0x18);assert_eq!(r[0],INVALID_HANDLE);
        r[1]=9;dispatch_objects(&mut k,&mut r,0x17);assert_eq!(r[0],INVALID_ENUM);
        r[0]=0xdead;assert!(!dispatch_objects(&mut k,&mut r,0x7f));assert_eq!(r[0],0xdead);
    }
    #[test]fn thread_clock_priority_sleep_and_exit(){
        // CTR-visible thread IDs remain native handles, not host thread IDs.
        let mut k=kernel();let a=k.create_thread("a",0,0,0,10,0);let b=k.create_thread("b",0,0,0,11,0);
        let mut machine=();k.schedule(&mut machine,0);assert_eq!(k.current_thread,Some(a));
        let mut r=[0;16];let mut exited=false;r[1]=CURRENT_THREAD;
        dispatch(&mut k,&mut r,0,&mut exited,0x0b);assert_eq!(r[1],10);
        r[0]=CURRENT_THREAD;r[1]=9;dispatch(&mut k,&mut r,0,&mut exited,0x0c);assert_eq!(k.thread(a).priority,9);
        let tick=0x1234567887654321;dispatch(&mut k,&mut r,tick,&mut exited,0x28);
        assert_eq!(r[0],0x87654321);assert_eq!(r[1],0x12345678);
        r[0]=0;r[1]=0;dispatch(&mut k,&mut r,100,&mut exited,0x0a);
        assert_eq!(k.thread(a).wakeup_at,Some(20100));k.schedule(&mut machine,101);assert_eq!(k.current_thread,Some(b));
        dispatch(&mut k,&mut r,102,&mut exited,0x09);assert_eq!(k.thread(b).status,ThreadStatus::Dead);assert!(!exited);
        dispatch(&mut k,&mut r,103,&mut exited,0x03);assert!(exited);assert!(k.threads.iter().all(|t|t.status==ThreadStatus::Dead));
    }
    #[test]fn session_dispatch_uses_typed_native_identity(){
        let mut k=kernel();let mut r=[0;16];let object=k.objects.insert(KObject::ClientPort("srv:".into()));
        r[0]=k.handles.create(&mut k.objects,object,"directory");
        let target=session_target(&mut k,&mut r).unwrap();assert_eq!(target.name(),"srv:");assert_eq!(r[0],0);
        let object=k.objects.insert(KObject::Process);r[0]=k.handles.create(&mut k.objects,object,"process");
        assert!(session_target(&mut k,&mut r).is_none());assert_eq!(r[0],INVALID_HANDLE);
    }
    #[test]fn checked_wait_register_array(){
        struct Memory(Vec<u8>);
        impl crate::services::router::ServiceMemory for Memory{
            fn check_span(&self,a:u32,n:usize,_:bool)->bool{(a as usize).checked_add(n).is_some_and(|end|end<=self.0.len())}
            fn read_span(&mut self,a:u32,b:&mut[u8])->bool{if !self.check_span(a,b.len(),false){return false;}b.copy_from_slice(&self.0[a as usize..a as usize+b.len()]);true}
            fn write_span(&mut self,_:u32,_:&[u8])->bool{false}
        }
        let mut k=kernel();k.create_thread("main",0,0,0,10,0);k.schedule(&mut (),0);
        let(_,handle)=k.create_event(ResetType::Sticky,"ready");let object=k.resolve(handle).unwrap();k.signal_event(object);
        let mut m=Memory(handle.to_le_bytes().to_vec());let mut r=[0;16];r[2]=1;
        assert!(wait_registers(&mut k,&mut r,0,&mut m,0x25));assert_eq!((r[0],r[1]),(0,0));
        r[2]=65;assert!(wait_registers(&mut k,&mut r,0,&mut m,0x25));assert_eq!(r[0],0xe0e01bfd);
        r[2]=1;r[1]=1;assert!(wait_registers(&mut k,&mut r,0,&mut m,0x25));assert_eq!(r[0],0xe0e01bf5);
        r[0]=handle;r[2]=0;r[3]=0;assert!(wait_registers(&mut k,&mut r,0,&mut m,0x24));assert_eq!(r[0],0);
        assert!(!wait_registers(&mut k,&mut r,0,&mut m,0x32));
    }
    #[test]fn wait_selected_index_and_timeout_registers(){
        let mut k=kernel();let a=k.create_thread("a",0,0,0,10,0);let mut machine=();k.schedule(&mut machine,0);
        let(_,first)=k.create_event(ResetType::OneShot,"first");let(second_object,second)=k.create_event(ResetType::OneShot,"second");
        let mut r=[0;16];k.signal_event(second_object);
        wait(&mut k,&mut r,0,&[first,second],false,0,WaitSyscall::WaitSynchronizationN);
        assert_eq!(r[0],0);assert_eq!(r[1],1);assert!(!k.is_signaled(second_object,a));
        wait(&mut k,&mut r,0,&[first,second],false,0,WaitSyscall::WaitSynchronizationN);assert_eq!(r[0],0x09401bfe);
        wait(&mut k,&mut r,0,&[second],false,1000,WaitSyscall::WaitSynchronization1);
        k.schedule(&mut machine,0);assert!(k.current_thread.is_none());k.schedule(&mut machine,269);
        assert_eq!(k.current_thread,Some(a));apply_wait_result(&mut k,&mut r);assert_eq!(r[0],0x09401bfe);
        wait(&mut k,&mut r,300,&[second],false,-1,WaitSyscall::WaitSynchronization1);
        k.schedule(&mut machine,300);assert!(k.current_thread.is_none());k.signal_event(second_object);k.schedule(&mut machine,301);
        apply_wait_result(&mut k,&mut r);assert_eq!(r[0],0);assert!(!k.is_signaled(second_object,a));
    }
    struct Word(i32);
    struct TlsMemory{mapped:Vec<u32>,fail:bool}
    struct Backing{last:Option<(u32,u32)>,error:Option<u32>}
    impl SharedBacking for Backing{fn shared_backing(&mut self,a:u32,n:u32)->Result<u32,u32>{self.last=Some((a,n));if let Some(error)=self.error{Err(error)}else{Ok(0x20004000)}}}
    #[test]fn shared_block_tracks_checked_native_backing_and_errors(){
        let mut k=kernel();let mut r=[0;16];let mut memory=Backing{last:None,error:None};r[1]=0x8000;r[2]=4096;
        create_memory_block(&mut k,&mut r,&mut memory);assert_eq!(r[0],0);assert_eq!(memory.last,Some((0x8000,4096)));
        let object=k.handles.resolve(r[1]).unwrap();assert!(matches!(k.objects.get(object),Some(KObject::SharedMemory(b))if b.paddr==0x20004000&&b.address==0x8000&&b.size==4096));
        let handles=k.handles.len();memory.error=Some(0xe0e01bf5);create_memory_block(&mut k,&mut r,&mut memory);assert_eq!(r[0],0xe0e01bf5);assert_eq!(k.handles.len(),handles);
    }
    impl ThreadMemory for TlsMemory{fn map_tls(&mut self,tls:u32)->Result<(),u32>{if self.fail{Err(0xd86007f3)}else{self.mapped.push(tls);Ok(())}}}
    #[test]fn creation_owns_native_context_tls_and_handle(){
        use crate::execution::{context::ThreadContext,arm::Cpu};
        let mut k=NativeScheduler::<ThreadContext,()>::new(1,(),0,crate::scheduler::ThreadProfile{tls_base:0x2000,tls_stride:512,heap_base:0});
        let mut memory=TlsMemory{mapped:Vec::new(),fail:false};let mut r=[0;16];r[0]=10;r[1]=0x1001;r[2]=7;r[3]=0x8000;
        create_thread(&mut k,&mut r,&mut memory);assert_eq!(r[0],0);assert!(matches!(k.objects.get(k.handles.resolve(r[1]).unwrap()),Some(KObject::Thread(0))));
        let mut cpu=Cpu::new();k.schedule(&mut cpu,0);assert_eq!(cpu.regs[0],7);assert_eq!(cpu.regs[13],0x8000);assert!(cpu.cpsr.thumb);assert_eq!(cpu.cp15.thread_id_ro,0x2000);assert_eq!(memory.mapped,vec![0x2000]);
        let handles=k.handles.len();memory.fail=true;r[1]=0x1000;create_thread(&mut k,&mut r,&mut memory);
        assert_eq!(r[0],0xd86007f3);assert_eq!(k.handles.len(),handles);assert_eq!(k.thread(1).status,ThreadStatus::Dead);
    }
    impl crate::ipc::message::IpcMemory for Word{fn read_word(&mut self,_:u32)->u32{self.0 as u32}fn write_word(&mut self,_:u32,v:u32){self.0=v as i32;}}
    #[test]fn arbiter_priority_wakeup_and_conditional_decrement(){
        let mut k=kernel();let low=k.create_thread("low",0,0,0,20,0);let high=k.create_thread("high",0,0,0,10,0);
        let mut machine=();k.schedule(&mut machine,0);assert_eq!(k.current_thread,Some(high));
        let mut r=[0;16];let mut exited=false;dispatch(&mut k,&mut r,0,&mut exited,0x21);let handle=r[1];
        let mut memory=Word(0);r[0]=handle;r[1]=0x1000;r[2]=2;r[3]=1;
        arbitrate(&mut k,&mut r,0,&mut memory);assert_eq!(memory.0,-1);k.schedule(&mut machine,1);assert_eq!(k.current_thread,Some(low));
        r[0]=handle;r[2]=1;arbitrate(&mut k,&mut r,1,&mut memory);k.schedule(&mut machine,2);assert!(k.current_thread.is_none());
        r[0]=handle;r[2]=0;r[3]=1;arbitrate(&mut k,&mut r,3,&mut memory);assert_eq!(k.thread(high).status,ThreadStatus::Ready);assert_eq!(k.thread(low).status,ThreadStatus::WaitArbiter);
        k.schedule(&mut machine,3);r[0]=handle;r[2]=2;r[3]=0;memory.0=2;arbitrate(&mut k,&mut r,4,&mut memory);assert_eq!(memory.0,2);assert_eq!(k.thread(high).status,ThreadStatus::Running);
    }
    #[test]fn expired_arbiter_cannot_wake_another_object_wait(){
        let mut k=kernel();let id=k.create_thread("worker",0,0,0,10,0);let mut machine=();k.schedule(&mut machine,0);
        let mut r=[0;16];let mut exited=false;dispatch(&mut k,&mut r,0,&mut exited,0x21);let a=r[1];
        dispatch(&mut k,&mut r,0,&mut exited,0x21);let b=r[1];let mut memory=Word(0);
        r[0]=a;r[1]=0x1000;r[2]=3;r[3]=1;r[4]=1000;arbitrate(&mut k,&mut r,0,&mut memory);k.schedule(&mut machine,0);
        k.schedule(&mut machine,269);assert_eq!(k.current_thread,Some(id));apply_wait_result(&mut k,&mut r);assert_eq!(r[0],0x09401bfe);
        r[0]=b;r[2]=1;arbitrate(&mut k,&mut r,300,&mut memory);k.schedule(&mut machine,300);
        r[0]=a;r[2]=0;r[3]=-1i32 as u32;arbitrate(&mut k,&mut r,301,&mut memory);assert_eq!(k.thread(id).status,ThreadStatus::WaitArbiter);
        r[0]=b;arbitrate(&mut k,&mut r,302,&mut memory);assert_eq!(k.thread(id).status,ThreadStatus::Ready);
    }
}
