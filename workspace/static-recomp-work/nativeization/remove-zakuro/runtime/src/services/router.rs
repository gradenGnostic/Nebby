//! Production TriAevum CTR service backend ownership. Synchronous memory
//! callbacks borrow checked guest spans; no application/runtime object ABI.
use std::{ffi::{c_void,c_char,CStr},path::Path};
pub trait ServiceMemory{
    fn check_span(&self,address:u32,size:usize,writable:bool)->bool;
    fn read_span(&mut self,address:u32,bytes:&mut[u8])->bool;
    fn write_span(&mut self,address:u32,bytes:&[u8])->bool;
}
type Read=unsafe extern "C" fn(*mut c_void,u32,*mut u8,usize)->bool;
type Write=unsafe extern "C" fn(*mut c_void,u32,*const u8,usize)->bool;
type Dispatch=unsafe extern "C" fn(*mut c_void,u32,*mut u32,usize,*mut c_void,Read,Write,u32)->i32;
type Object=unsafe extern "C" fn(*mut c_void,u32,*mut u32)->i32;
#[link(name="dl")]
unsafe extern "C"{fn dlopen(path:*const c_char,flags:i32)->*mut c_void;fn dlsym(handle:*mut c_void,name:*const c_char)->*mut c_void;fn dlclose(handle:*mut c_void)->i32;}
pub struct ServiceRouter{library:*mut c_void,context:*mut c_void,dispatch:Dispatch,object:Object,identity:unsafe extern "C" fn(*mut c_void,u32)->u32,destroy:unsafe extern "C" fn(*mut c_void)}
// Exclusively borrowed by the cooperative guest execution owner.
unsafe impl Send for ServiceRouter{}
unsafe extern "C" fn read<M:ServiceMemory>(owner:*mut c_void,address:u32,bytes:*mut u8,size:usize)->bool{
    if bytes.is_null(){return unsafe{(&*owner.cast::<M>()).check_span(address,size,false)};}if size==0{return true;}
    unsafe{(&mut*owner.cast::<M>()).read_span(address,std::slice::from_raw_parts_mut(bytes,size))}
}
unsafe extern "C" fn write<M:ServiceMemory>(owner:*mut c_void,address:u32,bytes:*const u8,size:usize)->bool{
    if bytes.is_null(){return unsafe{(&*owner.cast::<M>()).check_span(address,size,true)};}if size==0{return true;}
    unsafe{(&mut*owner.cast::<M>()).write_span(address,std::slice::from_raw_parts(bytes,size))}
}
impl ServiceRouter{
    pub fn new(path:&Path,region:u32,language:u32,model:u32,identity:u64)->Result<Self,&'static str>{
        use std::os::unix::ffi::OsStrExt;
        let path=std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_|"NUL in CTR backend path")?;
        let library=unsafe{dlopen(path.as_ptr(),2)};if library.is_null(){return Err("cannot load native CTR backend");}
        let symbol=|name:&CStr|unsafe{dlsym(library,name.as_ptr())};
        let create=symbol(c"ctr_native_services_create");let dispatch=symbol(c"ctr_native_services_dispatch");
        let object=symbol(c"ctr_native_services_object");let destroy=symbol(c"ctr_native_services_destroy");
        let identity_symbol=symbol(c"ctr_native_services_object_identity");
        if [create,dispatch,object,destroy,identity_symbol].iter().any(|s|s.is_null()){unsafe{dlclose(library);}return Err("native CTR backend ABI missing");}
        let create:unsafe extern "C" fn(u32,u32,u32,u64)->*mut c_void=unsafe{std::mem::transmute(create)};
        let context=unsafe{create(region,language,model,identity)};
        if context.is_null(){unsafe{dlclose(library);}return Err("native CTR backend construction failed");}
        Ok(Self{library,context,dispatch:unsafe{std::mem::transmute(dispatch)},object:unsafe{std::mem::transmute(object)},identity:unsafe{std::mem::transmute(identity_symbol)},destroy:unsafe{std::mem::transmute(destroy)}})
    }
    pub fn dispatch<M:ServiceMemory>(&mut self,service:u32,words:&mut[u32],memory:&mut M,static_table:u32)->i32{
        unsafe{(self.dispatch)(self.context,service,words.as_mut_ptr(),words.len(),(memory as *mut M).cast(),read::<M>,write::<M>,static_table)}
    }
    pub fn object(&self,handle:u32)->Result<[u32;3],i32>{
        let mut metadata=[0;3];let result=unsafe{(self.object)(self.context,handle,metadata.as_mut_ptr())};
        if result==0{Ok(metadata)}else{Err(result)}
    }
    pub fn object_identity(&mut self,handle:u32)->Result<u32,String>{let id=unsafe{(self.identity)(self.context,handle)};if id==0{Err(format!("native object identity unavailable for {handle:08x}"))}else{Ok(id)}}
}
impl Drop for ServiceRouter{fn drop(&mut self){unsafe{(self.destroy)(self.context);dlclose(self.library);}}}
#[cfg(test)]mod tests{
    use super::*;
    struct Memory([u8;16]);
    impl ServiceMemory for Memory{
        fn check_span(&self,a:u32,n:usize,_:bool)->bool{(a as usize).checked_add(n).is_some_and(|end|end<=16)}
        fn read_span(&mut self,a:u32,b:&mut[u8])->bool{if !self.check_span(a,b.len(),false){return false;}b.copy_from_slice(&self.0[a as usize..a as usize+b.len()]);true}
        fn write_span(&mut self,a:u32,b:&[u8])->bool{if !self.check_span(a,b.len(),true){return false;}self.0[a as usize..a as usize+b.len()].copy_from_slice(b);true}
    }
    #[test]fn checked_borrow_callbacks_and_permission_probes(){
        let mut m=Memory([0;16]);let owner=(&mut m as *mut Memory).cast();
        unsafe{
            assert!(read::<Memory>(owner,4,std::ptr::null_mut(),4));
            assert!(write::<Memory>(owner,4,std::ptr::null(),4));
            assert!(!read::<Memory>(owner,15,std::ptr::null_mut(),2));
            let bytes=[1,2,3,4];assert!(write::<Memory>(owner,4,bytes.as_ptr(),4));
            let mut out=[0;4];assert!(read::<Memory>(owner,4,out.as_mut_ptr(),4));assert_eq!(out,bytes);
            assert!(!write::<Memory>(owner,16,bytes.as_ptr(),4));
        }
    }
}
