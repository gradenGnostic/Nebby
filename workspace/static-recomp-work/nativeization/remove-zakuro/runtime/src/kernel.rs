//! Native CTR object namespace and synchronization state.
//! Architecture: TriAevum CtrIpcRouter/CtrKernelObject (typed shared identity,
//! per-process handles, explicit ownership/count/reset semantics). No emulator
//! CPU, memory implementation, scheduler or platform dependency.
use std::{collections::HashMap,sync::atomic::{AtomicU64,Ordering::Relaxed}};
pub type Handle=u32;
pub type ThreadId=u32;
pub const CURRENT_PROCESS:Handle=0xffff8001;
pub const CURRENT_THREAD:Handle=0xffff8000;
static HANDLES:AtomicU64=AtomicU64::new(0);
static EVENTS:AtomicU64=AtomicU64::new(0);
static WAITS:AtomicU64=AtomicU64::new(0);
static SHARED:AtomicU64=AtomicU64::new(0);
pub fn counters()->[u64;4]{[HANDLES.load(Relaxed),EVENTS.load(Relaxed),WAITS.load(Relaxed),SHARED.load(Relaxed)]}
#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash,PartialOrd,Ord)]pub struct ObjectId(pub u32);
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum ResetType{
    /// A single successful waiter consumes the signal (CTR reset type 0).
    OneShot,
    /// Remains signaled until explicitly cleared (CTR reset type 1).
    Sticky,
    /// Periodic timer mode (CTR reset type 2).
    Pulse,
}
impl ResetType{pub fn from_raw(v:u32)->Self{match v{1=>Self::Sticky,2=>Self::Pulse,_=>Self::OneShot}}}
#[derive(Debug,Clone)]pub struct Event{pub name:String,pub reset_type:ResetType,pub signaled:bool}
impl Event{pub fn new(reset_type:ResetType,name:impl Into<String>)->Self{Self{name:name.into(),reset_type,signaled:false}}}
#[derive(Debug,Clone)]pub struct Mutex{pub name:String,pub owner:Option<ThreadId>,pub lock_count:u32}
impl Mutex{pub fn new(name:impl Into<String>)->Self{Self{name:name.into(),owner:None,lock_count:0}}}
#[derive(Debug,Clone)]pub struct Semaphore{pub name:String,pub count:i32,pub max_count:i32}
#[derive(Debug,Clone)]pub struct Timer{pub name:String,pub reset_type:ResetType,pub signaled:bool,pub fire_at:Option<u64>,pub interval:u64}
#[derive(Debug,Clone,Default)]pub struct AddressArbiter{pub name:String,pub waiters:Vec<(ThreadId,u32)>}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum ArbitrationType{Signal,WaitIfLessThan,DecrementAndWaitIfLessThan,WaitIfLessThanWithTimeout,DecrementAndWaitIfLessThanWithTimeout}
impl ArbitrationType{
    pub fn from_raw(v:u32)->Option<Self>{Some(match v{0=>Self::Signal,1=>Self::WaitIfLessThan,2=>Self::DecrementAndWaitIfLessThan,3=>Self::WaitIfLessThanWithTimeout,4=>Self::DecrementAndWaitIfLessThanWithTimeout,_=>return None})}
    pub fn is_wait(self)->bool{self!=Self::Signal}
    pub fn has_timeout(self)->bool{matches!(self,Self::WaitIfLessThanWithTimeout|Self::DecrementAndWaitIfLessThanWithTimeout)}
    pub fn decrements(self)->bool{matches!(self,Self::DecrementAndWaitIfLessThan|Self::DecrementAndWaitIfLessThanWithTimeout)}
}
#[derive(Debug,Clone)]pub struct SharedMemory{pub name:String,pub address:u32,pub size:u32,pub paddr:u32,pub mapped_at:Option<u32>}
#[derive(Debug,Clone)]pub struct ClientSession{pub service:String,pub subhandle:u32}
#[derive(Debug)]pub enum KObject{Thread(ThreadId),Event(Event),Mutex(Mutex),Semaphore(Semaphore),Timer(Timer),AddressArbiter(AddressArbiter),SharedMemory(SharedMemory),ClientSession(ClientSession),ClientPort(String),Process,ResourceLimit}
impl KObject{
    pub fn type_name(&self)->&'static str{match self{Self::Thread(_)=>"Thread",Self::Event(_)=>"Event",Self::Mutex(_)=>"Mutex",Self::Semaphore(_)=>"Semaphore",Self::Timer(_)=>"Timer",Self::AddressArbiter(_)=>"AddressArbiter",Self::SharedMemory(_)=>"SharedMemory",Self::ClientSession(_)=>"ClientSession",Self::ClientPort(_)=>"ClientPort",Self::Process=>"Process",Self::ResourceLimit=>"ResourceLimit"}}
    fn count_operation(&self){match self{Self::Event(_)|Self::Timer(_)=>{EVENTS.fetch_add(1,Relaxed);},Self::SharedMemory(_)=>{SHARED.fetch_add(1,Relaxed);},_=>{}}}
    /// No scheduler ownership here: thread termination is supplied explicitly.
    pub fn available(&self,waiter:ThreadId,thread_exited:impl FnOnce(ThreadId)->bool)->bool{
        WAITS.fetch_add(1,Relaxed);
        match self{Self::Event(e)=>e.signaled,Self::Timer(t)=>t.signaled,Self::Semaphore(s)=>s.count>0,Self::Mutex(m)=>m.owner.is_none()||m.owner==Some(waiter),Self::Thread(t)=>thread_exited(*t),Self::ClientSession(_)|Self::ClientPort(_)=>false,_=>false}
    }
    pub fn acquire(&mut self,waiter:ThreadId){
        WAITS.fetch_add(1,Relaxed);self.count_operation();
        match self{
            Self::Event(e)=>{assert!(e.signaled);if e.reset_type==ResetType::OneShot{e.signaled=false;}},
            Self::Timer(t)=>{assert!(t.signaled);if t.reset_type==ResetType::OneShot{t.signaled=false;}},
            Self::Semaphore(s)=>{assert!(s.count>0);s.count-=1;},
            Self::Mutex(m)=>{assert!(m.owner.is_none()||m.owner==Some(waiter));m.owner=Some(waiter);m.lock_count=m.lock_count.checked_add(1).expect("CTR mutex recursion overflow");},
            Self::Thread(_)=>{},_=>panic!("non-waitable CTR object {}",self.type_name()),
        }
    }
}
#[derive(Debug)]struct Slot{object:KObject,refs:u32}
#[derive(Debug,Default)]pub struct ObjectStore{slots:Vec<Option<Slot>>,free:Vec<u32>}
impl ObjectStore{
    pub fn insert(&mut self,object:KObject)->ObjectId{object.count_operation();let slot=Some(Slot{object,refs:0});
        if let Some(i)=self.free.pop(){self.slots[i as usize]=slot;ObjectId(i)}else{let i=u32::try_from(self.slots.len()).expect("CTR object namespace exhausted");self.slots.push(slot);ObjectId(i)}}
    pub fn get(&self,id:ObjectId)->Option<&KObject>{self.slots.get(id.0 as usize)?.as_ref().map(|s|{s.object.count_operation();&s.object})}
    pub fn get_mut(&mut self,id:ObjectId)->Option<&mut KObject>{self.slots.get_mut(id.0 as usize)?.as_mut().map(|s|{s.object.count_operation();&mut s.object})}
    pub fn add_ref(&mut self,id:ObjectId){let s=self.slots.get_mut(id.0 as usize).and_then(Option::as_mut).expect("invalid CTR object reference");s.refs=s.refs.checked_add(1).expect("CTR object reference overflow");}
    fn release(&mut self,id:ObjectId){let s=self.slots[id.0 as usize].as_mut().expect("released CTR object missing");assert!(s.refs>0);s.refs-=1;if s.refs==0{self.slots[id.0 as usize]=None;self.free.push(id.0);}}
    pub fn iter(&self)->impl Iterator<Item=(ObjectId,&KObject)>{self.slots.iter().enumerate().filter_map(|(i,s)|s.as_ref().map(|s|(ObjectId(i as u32),&s.object)))}
}
#[derive(Debug)]pub struct HandleTable{entries:HashMap<Handle,ObjectId>,next:Handle,labels:HashMap<Handle,String>}
impl Default for HandleTable{fn default()->Self{Self{entries:HashMap::new(),next:0x10000,labels:HashMap::new()}}}
impl HandleTable{
    pub fn create(&mut self,store:&mut ObjectStore,id:ObjectId,label:&str)->Handle{
        HANDLES.fetch_add(1,Relaxed);let h=self.next;assert!(h<CURRENT_THREAD,"CTR handle namespace exhausted");self.next=self.next.checked_add(1).unwrap();
        assert!(!self.entries.contains_key(&h));store.add_ref(id);self.entries.insert(h,id);self.labels.insert(h,label.into());h
    }
    pub fn resolve(&self,h:Handle)->Option<ObjectId>{HANDLES.fetch_add(1,Relaxed);self.entries.get(&h).copied()}
    pub fn duplicate_object(&mut self,store:&mut ObjectStore,id:ObjectId,label:&str)->Handle{self.create(store,id,label)}
    pub fn close(&mut self,store:&mut ObjectStore,h:Handle)->bool{HANDLES.fetch_add(1,Relaxed);if let Some(id)=self.entries.remove(&h){self.labels.remove(&h);store.release(id);true}else{false}}
    pub fn label(&self,h:Handle)->&str{self.labels.get(&h).map_or("?",String::as_str)}
    pub fn len(&self)->usize{self.entries.len()}
    pub fn is_empty(&self)->bool{self.entries.is_empty()}
    pub fn iter(&self)->impl Iterator<Item=(Handle,ObjectId)>+'_ {self.entries.iter().map(|(&h,&id)|(h,id))}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn duplicate_identity_and_last_close(){let mut s=ObjectStore::default();let mut h=HandleTable::default();let o=s.insert(KObject::Event(Event::new(ResetType::OneShot,"e")));let a=h.create(&mut s,o,"a");let b=h.duplicate_object(&mut s,o,"b");assert_ne!(a,b);assert_eq!(h.resolve(b),Some(o));assert!(h.close(&mut s,a));assert!(s.get(o).is_some());assert!(h.close(&mut s,b));assert!(s.get(o).is_none());let c=s.insert(KObject::Process);let handle=h.create(&mut s,c,"c");assert_ne!(a,handle);assert_eq!(h.resolve(a),None);}
    #[test]fn event_reset_recursive_mutex_and_semaphore(){let mut event=KObject::Event(Event{name:"e".into(),reset_type:ResetType::OneShot,signaled:true});assert!(event.available(1,|_|false));event.acquire(1);assert!(!event.available(1,|_|false));let mut sticky=KObject::Event(Event{name:"s".into(),reset_type:ResetType::Sticky,signaled:true});sticky.acquire(1);assert!(sticky.available(2,|_|false));let mut mutex=KObject::Mutex(Mutex::new("m"));mutex.acquire(1);mutex.acquire(1);assert!(!mutex.available(2,|_|false));if let KObject::Mutex(m)=mutex{assert_eq!(m.lock_count,2);}let mut semaphore=KObject::Semaphore(Semaphore{name:"s".into(),count:1,max_count:2});semaphore.acquire(1);assert!(!semaphore.available(2,|_|false));}
}
