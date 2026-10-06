//! Direct native PICA frontend ABI. No reference GPU or rendered-frame copy.
use crate::memory::address_space::AddressSpace;
use std::{ffi::{c_void,c_char,CString},path::Path};
type Read=unsafe extern "C" fn(*mut c_void,u32,*mut u8,usize)->bool;
type Submit=unsafe extern "C" fn(u32,*const u8,u32,*mut c_void,Read)->u64;
type Transfer=unsafe extern "C" fn(u32,u32,u32,u32,u32,*mut c_void,Read)->bool;
#[link(name="dl")]
unsafe extern "C"{fn dlopen(path:*const c_char,flags:i32)->*mut c_void;fn dlsym(handle:*mut c_void,name:*const c_char)->*mut c_void;fn dlclose(handle:*mut c_void)->i32;}
pub struct PicaFrontend {
    library:*mut c_void,submit:Submit,transfer:Transfer,fill:unsafe extern "C" fn(u32,u32,u32,u32),
    pub command_lists:u64,
}
unsafe extern "C" fn read(context:*mut c_void,address:u32,output:*mut u8,size:usize)->bool{
    if context.is_null()||output.is_null()||size>64<<20{return false;}
    let memory=unsafe{&mut*context.cast::<AddressSpace>()};
    memory.read_gpu(address,unsafe{std::slice::from_raw_parts_mut(output,size)})
}
impl PicaFrontend {
    pub fn open(path:&Path)->Result<Self,String>{
        use std::os::unix::ffi::OsStrExt;
        let path=CString::new(path.as_os_str().as_bytes()).map_err(|_|"invalid PICA library path")?;
        let library=unsafe{dlopen(path.as_ptr(),2)};if library.is_null(){return Err("native PICA frontend load failed".into());}
        let submit=unsafe{dlsym(library,c"moon_triaevum_submit_pica".as_ptr())};
        let transfer=unsafe{dlsym(library,c"moon_triaevum_display_transfer".as_ptr())};
        let fill=unsafe{dlsym(library,c"moon_triaevum_memory_fill".as_ptr())};
        if [submit,transfer,fill].iter().any(|symbol|symbol.is_null()){unsafe{dlclose(library);}return Err("native PICA frontend ABI missing".into());}
        Ok(Self{library,submit:unsafe{std::mem::transmute(submit)},transfer:unsafe{std::mem::transmute(transfer)},fill:unsafe{std::mem::transmute(fill)},command_lists:0})
    }
    pub fn submit(&mut self,address:u32,size:u32,memory:&mut AddressSpace)->Result<u64,String>{
        if size==0||size%4!=0||size>1<<20{return Err("invalid native PICA command list size".into());}
        let mut bytes=vec![0;size as usize];
        if !unsafe{read((memory as *mut AddressSpace).cast(),address,bytes.as_mut_ptr(),bytes.len())}{return Err("native PICA command list backing unavailable".into());}
        let draws=unsafe{(self.submit)(address,bytes.as_ptr(),size,(memory as *mut AddressSpace).cast(),read)};
        if draws==u64::MAX{return Err("native PICA frontend rejected command list".into());}
        self.command_lists+=1;Ok(draws)
    }
    pub fn display_transfer(&mut self,input:u32,output:u32,input_size:u32,output_size:u32,flags:u32,memory:&mut AddressSpace)->bool{
        unsafe{(self.transfer)(input,output,input_size,output_size,flags,(memory as *mut AddressSpace).cast(),read)}
    }
    pub fn memory_fill(&mut self,start:u32,end:u32,value:u32,width:u32){unsafe{(self.fill)(start,end,value,width);}}
}
impl Drop for PicaFrontend{fn drop(&mut self){unsafe{dlclose(self.library);}}}
