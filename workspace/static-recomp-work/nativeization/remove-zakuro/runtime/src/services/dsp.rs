//! Production native CTR DSP owner. Memory is borrowed only during calls.
use super::router::ServiceMemory;
use std::ffi::{c_char,c_void,CString};
type Read=unsafe extern "C" fn(*mut c_void,u32,*mut u8,usize)->bool;
type Write=unsafe extern "C" fn(*mut c_void,u32,*const u8,usize)->bool;
type Physical=unsafe extern "C" fn(*mut c_void,u32)->*const u8;
type Dispatch=unsafe extern "C" fn(*mut c_void,*mut u32,usize,u32,u32,*mut c_void,Read,Write,*mut u32,usize,*mut u32)->i32;
type Frame=unsafe extern "C" fn(*mut c_void,*mut c_void,Read,Write,Physical,*mut i16,usize,*mut u32,usize)->i32;
#[link(name="dl")]
unsafe extern "C"{fn dlopen(p:*const c_char,f:i32)->*mut c_void;fn dlsym(h:*mut c_void,p:*const c_char)->*mut c_void;fn dlclose(h:*mut c_void)->i32;}
/// Physical sample backing must remain stable throughout a synchronous frame.
///
/// # Safety
/// Non-null pointers must cover the DSP source's valid physical sample range;
/// the producer must never retain them beyond the frame. Invalid addresses
/// return null. The memory owner must exclude concurrent mutation/reallocation.
pub unsafe trait DspMemory:ServiceMemory {
    fn sample_pointer(&mut self,address:u32)->*const u8;
}
unsafe extern "C" fn read<M:ServiceMemory>(owner:*mut c_void,address:u32,out:*mut u8,size:usize)->bool {
    let memory=unsafe{&mut *owner.cast::<M>()};
    if !memory.check_span(address,size,false){return false;}
    if out.is_null(){return true;}
    if size>16<<20{return false;}
    memory.read_span(address,unsafe{std::slice::from_raw_parts_mut(out,size)})
}
unsafe extern "C" fn write<M:ServiceMemory>(owner:*mut c_void,address:u32,input:*const u8,size:usize)->bool {
    let memory=unsafe{&mut *owner.cast::<M>()};
    if !memory.check_span(address,size,true){return false;}
    if input.is_null(){return true;}
    if size>16<<20{return false;}
    memory.write_span(address,unsafe{std::slice::from_raw_parts(input,size)})
}
unsafe extern "C" fn physical<M:DspMemory>(owner:*mut c_void,address:u32)->*const u8 {
    unsafe{(&mut *owner.cast::<M>()).sample_pointer(address)}
}
pub struct DspRuntime {
    library:*mut c_void,context:*mut c_void,dispatch:Dispatch,frame:Frame,
    destroy:unsafe extern "C" fn(*mut c_void),
}
// Owned exclusively by the cooperative native process execution thread.
unsafe impl Send for DspRuntime {}
pub struct DspReply {pub signals:[u32;64],pub running:bool}
pub struct DspFrame {pub pcm:Vec<[i16;2]>,pub signals:[u32;64]}
impl DspRuntime {
    pub fn open(path:&str)->Result<Self,String> {
        let path=CString::new(path).map_err(|_|"invalid DSP path")?;
        let library=unsafe{dlopen(path.as_ptr(),2)};
        if library.is_null(){return Err("native DSP library load failed".into());}
        let create=unsafe{dlsym(library,c"ctr_native_dsp_create".as_ptr())};
        let dispatch=unsafe{dlsym(library,c"ctr_native_dsp_dispatch".as_ptr())};
        let frame=unsafe{dlsym(library,c"ctr_native_dsp_frame".as_ptr())};
        let destroy=unsafe{dlsym(library,c"ctr_native_dsp_destroy".as_ptr())};
        if [create,dispatch,frame,destroy].iter().any(|p|p.is_null()) {
            unsafe{dlclose(library);}return Err("native DSP ABI missing".into());
        }
        let create:unsafe extern "C" fn()->*mut c_void=unsafe{std::mem::transmute(create)};
        let context=unsafe{create()};
        if context.is_null(){unsafe{dlclose(library);}return Err("native DSP creation failed".into());}
        Ok(Self{library,context,dispatch:unsafe{std::mem::transmute(dispatch)},frame:unsafe{std::mem::transmute(frame)},destroy:unsafe{std::mem::transmute(destroy)}})
    }
    pub fn dispatch<M:ServiceMemory>(&mut self,words:&mut[u32],static_table:u32,semaphore:u32,memory:&mut M)->Result<DspReply,i32> {
        let mut signals=[0;64];let mut running=0;
        let result=unsafe{(self.dispatch)(self.context,words.as_mut_ptr(),words.len(),static_table,semaphore,(memory as *mut M).cast(),read::<M>,write::<M>,signals.as_mut_ptr(),signals.len(),&mut running)};
        if result<0{Err(result)}else{Ok(DspReply{signals,running:running!=0})}
    }
    pub fn frame<M:DspMemory>(&mut self,memory:&mut M)->Result<DspFrame,i32> {
        let mut pcm=[0i16;320];let mut signals=[0;64];
        let size=unsafe{(self.frame)(self.context,(memory as *mut M).cast(),read::<M>,write::<M>,physical::<M>,pcm.as_mut_ptr(),pcm.len(),signals.as_mut_ptr(),signals.len())};
        if size<0{return Err(size);}
        if size as usize>pcm.len()||size%2!=0{return Err(i32::MIN);}
        Ok(DspFrame{pcm:pcm[..size as usize].chunks_exact(2).map(|p|[p[0],p[1]]).collect(),signals})
    }
}
impl Drop for DspRuntime {fn drop(&mut self){unsafe{(self.destroy)(self.context);dlclose(self.library);}}}
/// Native process DSP lifecycle and kernel signaling. PCM ownership passes
/// directly to the platform sink; no application-owned emulator queue.
pub struct DspSession {
    runtime:DspRuntime,semaphore:Option<u32>,
    pub running:bool,pub calls:u64,pub frames:u64,
}
impl DspSession {
    pub fn open(path:&str)->Result<Self,String>{Ok(Self{runtime:DspRuntime::open(path)?,semaphore:None,running:false,calls:0,frames:0})}
    fn signal<C:crate::scheduler::ExecutionContext,R>(kernel:&mut crate::scheduler::NativeScheduler<C,R>,handles:&[u32]){
        for &handle in handles{if handle==0{break;}if let Some(object)=kernel.resolve(handle){kernel.signal_event(object);}}
    }
    pub fn dispatch<C:crate::scheduler::ExecutionContext,R,M:ServiceMemory>(&mut self,words:&mut[u32;64],tls:u32,memory:&mut M,kernel:&mut crate::scheduler::NativeScheduler<C,R>)->Result<(),String>{
        let command=words[0]>>16;
        let semaphore=if command==0x16{
            if let Some(handle)=self.semaphore{handle}else{
                let(_,handle)=kernel.create_event(crate::kernel::ResetType::OneShot,"native DSP semaphore");self.semaphore=Some(handle);handle
            }
        }else{0};
        let table=tls.checked_add(0x180).ok_or("native DSP static buffer table overflow")?;
        let reply=self.runtime.dispatch(words,table,semaphore,memory).map_err(|result|format!("native DSP command {command:04x} rejected: {result}"))?;
        self.calls+=1;self.running=reply.running;Self::signal(kernel,&reply.signals);Ok(())
    }
    pub fn frame<C:crate::scheduler::ExecutionContext,R,M:DspMemory>(&mut self,memory:&mut M,kernel:&mut crate::scheduler::NativeScheduler<C,R>)->Result<Vec<[i16;2]>,String>{
        let frame=self.runtime.frame(memory).map_err(|result|format!("native DSP frame failed: {result}"))?;
        self.frames+=1;Self::signal(kernel,&frame.signals);Ok(frame.pcm)
    }
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn missing_library_rejected(){assert!(DspRuntime::open("/nonexistent/native-dsp.so").is_err());}
}
