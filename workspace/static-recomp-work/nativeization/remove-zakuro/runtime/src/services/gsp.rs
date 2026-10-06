//! Native CTR GSP service owner, independent of any application/process shell.
use super::router::ServiceMemory;
use std::ffi::{c_void,c_char,CString};
pub trait GspHost:ServiceMemory {
    /// Synchronous PICA/display/memory work from the production GSP frontend.
    fn gpu_action(&mut self,operation:u32,argument:u32,words:&[u32])->bool;
}
type Read=unsafe extern "C" fn(*mut c_void,u32,*mut u8,usize)->bool;
type Write=unsafe extern "C" fn(*mut c_void,u32,*const u8,usize)->bool;
type Action=unsafe extern "C" fn(*mut c_void,u32,u32,*const u32,usize)->bool;
type Dispatch=unsafe extern "C" fn(*mut c_void,*mut u32,usize,u32,u32,*mut c_void,Read,Write,Action,*mut u32)->i32;
#[link(name="dl")]
unsafe extern "C"{fn dlopen(p:*const c_char,f:i32)->*mut c_void;fn dlsym(h:*mut c_void,p:*const c_char)->*mut c_void;fn dlclose(h:*mut c_void)->i32;}
unsafe extern "C" fn read<H:GspHost>(owner:*mut c_void,address:u32,out:*mut u8,size:usize)->bool {
    let host=unsafe{&mut *owner.cast::<H>()};
    if !host.check_span(address,size,false){return false;}
    if out.is_null(){return true;}if size>16<<20{return false;}
    host.read_span(address,unsafe{std::slice::from_raw_parts_mut(out,size)})
}
unsafe extern "C" fn write<H:GspHost>(owner:*mut c_void,address:u32,input:*const u8,size:usize)->bool {
    let host=unsafe{&mut *owner.cast::<H>()};
    if !host.check_span(address,size,true){return false;}
    if input.is_null(){return true;}if size>16<<20{return false;}
    host.write_span(address,unsafe{std::slice::from_raw_parts(input,size)})
}
unsafe extern "C" fn action<H:GspHost>(owner:*mut c_void,operation:u32,argument:u32,data:*const u32,count:usize)->bool {
    if count>4096||count!=0&&data.is_null(){return false;}
    let words=if count==0{&[]}else{unsafe{std::slice::from_raw_parts(data,count)}};
    unsafe{(&mut *owner.cast::<H>()).gpu_action(operation,argument,words)}
}
pub struct GspRuntime {library:*mut c_void,context:*mut c_void,dispatch:Dispatch,destroy:unsafe extern "C" fn(*mut c_void)}
unsafe impl Send for GspRuntime {}
impl GspRuntime {
    pub fn open(path:&str)->Result<Self,String> {
        let path=CString::new(path).map_err(|_|"invalid GSP path")?;
        let library=unsafe{dlopen(path.as_ptr(),2)};
        if library.is_null(){return Err("native GSP library load failed".into());}
        let create=unsafe{dlsym(library,c"ctr_native_gsp_create".as_ptr())};
        let dispatch=unsafe{dlsym(library,c"ctr_native_gsp_dispatch".as_ptr())};
        let destroy=unsafe{dlsym(library,c"ctr_native_gsp_destroy".as_ptr())};
        if [create,dispatch,destroy].iter().any(|p|p.is_null()){unsafe{dlclose(library);}return Err("native GSP ABI missing".into());}
        let create:unsafe extern "C" fn()->*mut c_void=unsafe{std::mem::transmute(create)};
        let context=unsafe{create()};
        if context.is_null(){unsafe{dlclose(library);}return Err("native GSP construction failed".into());}
        Ok(Self{library,context,dispatch:unsafe{std::mem::transmute(dispatch)},destroy:unsafe{std::mem::transmute(destroy)}})
    }
    pub fn dispatch<H:GspHost>(&mut self,words:&mut[u32],shared_address:u32,vblank:bool,host:&mut H)->Result<u32,i32> {
        let mut signals=0;
        let result=unsafe{(self.dispatch)(self.context,words.as_mut_ptr(),words.len(),shared_address,u32::from(vblank),(host as *mut H).cast(),read::<H>,write::<H>,action::<H>,&mut signals)};
        if result<0{Err(result)}else{Ok(signals)}
    }
}
impl Drop for GspRuntime {fn drop(&mut self){unsafe{(self.destroy)(self.context);dlclose(self.library);}}}
/// Native process GSP session resources; kernel identities survive guest closes.
pub struct GspSession {
    runtime:GspRuntime,shared:Option<crate::kernel::ObjectId>,interrupt:Option<crate::kernel::ObjectId>,
    pub calls:u64,pub vblanks:u64,
}
impl GspSession {
    pub fn open(path:&str)->Result<Self,String>{Ok(Self{runtime:GspRuntime::open(path)?,shared:None,interrupt:None,calls:0,vblanks:0})}
    pub fn prepare<C:crate::scheduler::ExecutionContext,R>(&mut self,words:&[u32;64],memory:&mut crate::memory::address_space::AddressSpace,kernel:&mut crate::scheduler::NativeScheduler<C,R>)->Result<(),String>{
        use crate::kernel::{KObject,SharedMemory};
        if words[0]>>16==0x13 {
            let interrupt=kernel.resolve(words[3]).ok_or("native GSP relay event handle invalid")?;
            if !matches!(kernel.objects.get(interrupt),Some(KObject::Event(_))){return Err("native GSP relay object is not an event".into());}
            if self.interrupt!=Some(interrupt){kernel.objects.add_ref(interrupt);self.interrupt=Some(interrupt);}
            if self.shared.is_none(){
                let physical=memory.allocate(2,4096).ok_or("native GSP shared allocation failed")?;
                let object=kernel.objects.insert(KObject::SharedMemory(SharedMemory{name:"GSP".into(),address:0,size:4096,paddr:physical,mapped_at:None}));
                kernel.objects.add_ref(object);self.shared=Some(object);
            }
        }
        Ok(())
    }
    fn mapped<C:crate::scheduler::ExecutionContext,R>(&self,kernel:&crate::scheduler::NativeScheduler<C,R>)->u32{
        self.shared.and_then(|object|match kernel.objects.get(object){Some(crate::kernel::KObject::SharedMemory(block))=>block.mapped_at,_=>None}).unwrap_or(0)
    }
    pub fn dispatch<C:crate::scheduler::ExecutionContext,R,H:GspHost>(&mut self,words:&mut[u32;64],host:&mut H,kernel:&mut crate::scheduler::NativeScheduler<C,R>)->Result<(),String>{
        let relay=words[0]>>16==0x13;let mapped=self.mapped(kernel);
        let signals=self.runtime.dispatch(words,mapped,false,host).map_err(|result|format!("native GSP IPC rejected: {result}"))?;
        self.calls+=1;
        if relay{words[4]=kernel.handles.create(&mut kernel.objects,self.shared.ok_or("native GSP relay resources not prepared")?,"native GSP shared memory");}
        if signals!=0{if let Some(event)=self.interrupt{kernel.signal_event(event);}}
        Ok(())
    }
    pub fn vblank<C:crate::scheduler::ExecutionContext,R,H:GspHost>(&mut self,host:&mut H,kernel:&mut crate::scheduler::NativeScheduler<C,R>)->Result<(),String>{
        let mapped=self.mapped(kernel);if mapped==0{return Ok(());}
        let signals=self.runtime.dispatch(&mut[],mapped,true,host).map_err(|result|format!("native GSP vblank rejected: {result}"))?;
        self.vblanks+=1;if signals!=0{if let Some(event)=self.interrupt{kernel.signal_event(event);}}Ok(())
    }
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn missing_library(){assert!(GspRuntime::open("/missing/native-gsp.so").is_err());}
}
