//! Native CTR address-space owner. No reference runtime or emulator types.
use std::ffi::{c_char,c_void,CString};
#[repr(C)]
#[derive(Default,Debug,Clone,Copy,PartialEq,Eq)]
pub struct Mapping {pub base:u32,pub size:u32,pub physical:u32,pub permission:u32,pub state:u32}
type Physical=unsafe extern "C" fn(*mut c_void,u32,u32)->*mut u8;
type Region=unsafe extern "C" fn(*mut c_void,u32,u32)->u32;
type Allocate=unsafe extern "C" fn(*mut c_void,u32,u32)->u32;
type Range=unsafe extern "C" fn(*mut c_void,u32,u32)->bool;
type Map=unsafe extern "C" fn(*mut c_void,u32,u32,u32,u32,u32)->bool;
type Query=unsafe extern "C" fn(*mut c_void,u32,*mut Mapping)->bool;
type Snapshot=unsafe extern "C" fn(*mut c_void,*mut Mapping,u32)->u32;
#[link(name="dl")]
unsafe extern "C"{fn dlopen(p:*const c_char,f:i32)->*mut c_void;fn dlsym(h:*mut c_void,p:*const c_char)->*mut c_void;fn dlclose(h:*mut c_void)->i32;}
pub struct AddressSpaceOwner {
    library:*mut c_void,context:*mut c_void,physical:Physical,region:Region,
    allocate:Allocate,free:Range,map:Map,unmap:Range,query:Query,snapshot:Snapshot,
    destroy:unsafe extern "C" fn(*mut c_void),
}
// Exclusive control-plane owner; borrowed backing never moves until Drop.
unsafe impl Send for AddressSpaceOwner {}
impl AddressSpaceOwner {
    pub fn open(path:&str,total:u32,application:u32)->Result<Self,String> {
        let path=CString::new(path).map_err(|_|"invalid memory library path")?;
        let library=unsafe{dlopen(path.as_ptr(),2)};
        if library.is_null(){return Err("native address space library load failed".into());}
        let names=[c"ctr_native_memory_create",c"ctr_native_memory_physical",c"ctr_native_memory_region",c"ctr_native_memory_allocate",c"ctr_native_memory_free",c"ctr_native_memory_map",c"ctr_native_memory_unmap",c"ctr_native_memory_query",c"ctr_native_memory_snapshot",c"ctr_native_memory_destroy"];
        let symbols=names.map(|name|unsafe{dlsym(library,name.as_ptr())});
        if symbols.iter().any(|p|p.is_null()){unsafe{dlclose(library);}return Err("native address space ABI missing".into());}
        let create:unsafe extern "C" fn(u32,u32)->*mut c_void=unsafe{std::mem::transmute(symbols[0])};
        let context=unsafe{create(total,application)};
        if context.is_null(){unsafe{dlclose(library);}return Err("native address space construction failed".into());}
        Ok(Self{library,context,physical:unsafe{std::mem::transmute(symbols[1])},region:unsafe{std::mem::transmute(symbols[2])},allocate:unsafe{std::mem::transmute(symbols[3])},free:unsafe{std::mem::transmute(symbols[4])},map:unsafe{std::mem::transmute(symbols[5])},unmap:unsafe{std::mem::transmute(symbols[6])},query:unsafe{std::mem::transmute(symbols[7])},snapshot:unsafe{std::mem::transmute(symbols[8])},destroy:unsafe{std::mem::transmute(symbols[9])}})
    }
    /// Checked physical span. Caller must retain this owner and enforce exclusive
    /// writes while dereferencing returned pointers; backing is never relocated.
    pub fn pointer(&mut self,address:u32,size:u32)->*mut u8 {
        unsafe{(self.physical)(self.context,address,size)}
    }
    pub fn region_size(&mut self,region:u32)->u32 {unsafe{(self.region)(self.context,region,0)}}
    pub fn region_used(&mut self,region:u32)->u32 {unsafe{(self.region)(self.context,region,1)}}
    pub fn allocate(&mut self,region:u32,size:u32)->Option<u32> {
        let address=unsafe{(self.allocate)(self.context,region,size)};(address!=0).then_some(address)
    }
    pub fn free(&mut self,address:u32,size:u32)->bool {unsafe{(self.free)(self.context,address,size)}}
    pub fn map(&mut self,mapping:Mapping)->bool {
        unsafe{(self.map)(self.context,mapping.base,mapping.physical,mapping.size,mapping.permission,mapping.state)}
    }
    pub fn unmap(&mut self,address:u32,size:u32)->bool {unsafe{(self.unmap)(self.context,address,size)}}
    pub fn query(&mut self,address:u32)->(bool,Mapping) {
        let mut mapping=Mapping::default();let found=unsafe{(self.query)(self.context,address,&mut mapping)};(found,mapping)
    }
    pub fn mappings(&mut self)->Vec<Mapping> {
        let count=unsafe{(self.snapshot)(self.context,std::ptr::null_mut(),0)};
        let mut mappings=vec![Mapping::default();count as usize];
        assert_eq!(unsafe{(self.snapshot)(self.context,mappings.as_mut_ptr(),count)},count,"address-space snapshot changed during exclusive borrow");mappings
    }
}
impl Drop for AddressSpaceOwner {fn drop(&mut self){unsafe{(self.destroy)(self.context);dlclose(self.library);}}}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn mapping_abi(){assert_eq!(std::mem::size_of::<Mapping>(),20);assert_eq!(std::mem::align_of::<Mapping>(),4);}
    #[test]fn missing_library(){assert!(AddressSpaceOwner::open("/missing/native-memory.so",0x8000000,0x4000000).is_err());}
}
