//! Native CTR HID producer; no process-shell or emulator types.
use std::ffi::{c_char,c_void,CString};
type Dispatch=unsafe extern "C" fn(*mut c_void,*mut u32,usize)->i32;
type Sample=unsafe extern "C" fn(*mut c_void,u32,i16,i16,u16,u16,u32,u64,*mut u8,usize,*mut u32)->i32;
#[link(name="dl")]
unsafe extern "C" {
    fn dlopen(path:*const c_char,flags:i32)->*mut c_void;
    fn dlsym(handle:*mut c_void,name:*const c_char)->*mut c_void;
    fn dlclose(handle:*mut c_void)->i32;
}
/// Owns the production title-neutral CTR HID implementation.
pub struct HidProducer {
    library:*mut c_void,context:*mut c_void,dispatch:Dispatch,sample:Sample,
    destroy:unsafe extern "C" fn(*mut c_void),
}
unsafe impl Send for HidProducer {}
impl HidProducer {
    pub fn open(path:&str)->Result<Self,String> {
        let path=CString::new(path).map_err(|_|"invalid HID library path")?;
        let library=unsafe{dlopen(path.as_ptr(),2)};
        if library.is_null(){return Err("cannot load native HID library".into());}
        let create=unsafe{dlsym(library,c"ctr_native_hid_create".as_ptr())};
        let dispatch=unsafe{dlsym(library,c"ctr_native_hid_dispatch".as_ptr())};
        let sample=unsafe{dlsym(library,c"ctr_native_hid_sample".as_ptr())};
        let destroy=unsafe{dlsym(library,c"ctr_native_hid_destroy".as_ptr())};
        if [create,dispatch,sample,destroy].iter().any(|p|p.is_null()) {
            unsafe{dlclose(library);}return Err("native HID ABI missing".into());
        }
        let create:unsafe extern "C" fn()->*mut c_void=unsafe{std::mem::transmute(create)};
        let context=unsafe{create()};
        if context.is_null(){unsafe{dlclose(library);}return Err("native HID creation failed".into());}
        Ok(Self{library,context,dispatch:unsafe{std::mem::transmute(dispatch)},sample:unsafe{std::mem::transmute(sample)},destroy:unsafe{std::mem::transmute(destroy)}})
    }
    pub fn dispatch(&mut self,words:&mut[u32])->Result<(),i32> {
        let result=unsafe{(self.dispatch)(self.context,words.as_mut_ptr(),words.len())};
        if result<0{Err(result)}else{Ok(())}
    }
    pub fn sample(&mut self,input:HidInput,tick:u64,output:&mut[u8;0x1000])->Result<(usize,u32),i32> {
        let (x,y)=input.touch.unwrap_or((0,0));let mut signals=0;
        let size=unsafe{(self.sample)(self.context,input.buttons,axis(input.circle_x),axis(input.circle_y),x.min(319),y.min(239),u32::from(input.touch.is_some()),tick,output.as_mut_ptr(),output.len(),&mut signals)};
        if size<0{return Err(size);}
        if size as usize>output.len(){return Err(-1);}
        Ok((size as usize,signals))
    }
}
impl Drop for HidProducer {fn drop(&mut self){unsafe{(self.destroy)(self.context);dlclose(self.library);}}}
#[derive(Default,Clone,Copy)]
pub struct HidInput {pub buttons:u32,pub circle_x:f32,pub circle_y:f32,pub touch:Option<(u16,u16)>}
fn axis(value:f32)->i16 {if value.is_finite(){(value.clamp(-1.,1.)*156.)as i16}else{0}}
/// Native process-owned HID shared memory and synchronization resources.
pub struct HidSession {
    producer:HidProducer,
    shared:Option<(crate::kernel::ObjectId,u32)>,
    events:Vec<crate::kernel::ObjectId>,
    pub calls:u64,pub samples:u64,
}
impl HidSession {
    pub fn open(path:&str)->Result<Self,String>{Ok(Self{producer:HidProducer::open(path)?,shared:None,events:Vec::new(),calls:0,samples:0})}
    pub fn dispatch<C:crate::scheduler::ExecutionContext,R>(&mut self,words:&mut[u32;64],memory:&mut crate::memory::address_space::AddressSpace,kernel:&mut crate::scheduler::NativeScheduler<C,R>)->Result<(),String>{
        use crate::kernel::{KObject,SharedMemory,ResetType};
        let command=words[0]>>16;
        self.producer.dispatch(words).map_err(|result|format!("native HID command {command:04x} rejected: {result}"))?;
        self.calls+=1;
        if command==0xa {
            if self.shared.is_none(){
                let physical=memory.allocate(2,4096).ok_or("native HID shared allocation failed")?;
                let object=kernel.objects.insert(KObject::SharedMemory(SharedMemory{name:"HID".into(),address:0,size:4096,paddr:physical,mapped_at:None}));
                kernel.objects.add_ref(object);self.shared=Some((object,physical));
                for _ in 0..5 {
                    let(object,handle)=kernel.create_event(ResetType::OneShot,"native HID event");
                    kernel.objects.add_ref(object);kernel.handles.close(&mut kernel.objects,handle);self.events.push(object);
                }
            }
            words[3]=kernel.handles.create(&mut kernel.objects,self.shared.unwrap().0,"native HID shared memory");
            for (index,&object)in self.events.iter().enumerate(){words[4+index]=kernel.handles.create(&mut kernel.objects,object,"native HID event");}
        }
        Ok(())
    }
    pub fn sample<C:crate::scheduler::ExecutionContext,R>(&mut self,input:HidInput,tick:u64,memory:&mut crate::memory::address_space::AddressSpace,kernel:&mut crate::scheduler::NativeScheduler<C,R>)->Result<(),String>{
        let Some((_,physical))=self.shared else{return Ok(());};
        let mut bytes=[0;4096];let(size,mask)=self.producer.sample(input,tick,&mut bytes).map_err(|result|format!("native HID sample failed: {result}"))?;
        let pointer=memory.physical_pointer(physical,size as u32);if pointer.is_null(){return Err("native HID backing unavailable".into());}
        unsafe{std::ptr::copy_nonoverlapping(bytes.as_ptr(),pointer,size);}
        for(index,&object)in self.events.iter().enumerate(){if mask&(1<<index)!=0{kernel.signal_event(object);}}
        self.samples+=1;Ok(())
    }
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn normalized_axes(){assert_eq!(axis(2.),156);assert_eq!(axis(-2.),-156);assert_eq!(axis(f32::NAN),0);}
    #[test]fn missing_library_rejected(){assert!(HidProducer::open("/nonexistent/native-hid.so").is_err());}
}
