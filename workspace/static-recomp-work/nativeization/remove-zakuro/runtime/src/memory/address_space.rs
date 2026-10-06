//! Native guest memory facade and AOT page tables; no emulator dependencies.
use super::{owner::{AddressSpaceOwner,Mapping},page_span};
use crate::{GuestMemory,execution::{arm::Bus,aot::AotMemory},services::router::ServiceMemory};
use std::collections::BTreeMap;
const PAGE_SIZE:u32=4096;
pub struct AddressSpace {
    owner:AddressSpaceOwner,
    read:Box<[*mut u8]>,write:Box<[*mut u8]>,mappings:BTreeMap<u32,Mapping>,
    image_region:u32,
}
// The process execution thread exclusively owns this memory and its page tables.
unsafe impl Send for AddressSpace {}
impl AddressSpace {
    pub fn new(owner:AddressSpaceOwner)->Self {
        Self{owner,read:vec![std::ptr::null_mut();1<<20].into_boxed_slice(),write:vec![std::ptr::null_mut();1<<20].into_boxed_slice(),mappings:BTreeMap::new(),image_region:0}
    }
    pub fn set_image_region(&mut self,region:u32)->Result<(),String>{if region>2{return Err("invalid image allocation region".into());}self.image_region=region;Ok(())}
    pub fn allocate(&mut self,region:u32,size:u32)->Option<u32>{self.owner.allocate(region,size)}
    pub fn region_used(&mut self,region:u32)->u32{self.owner.region_used(region)}
    pub fn region_size(&mut self,region:u32)->u32{self.owner.region_size(region)}
    pub fn physical_pointer(&mut self,address:u32,size:u32)->*mut u8{self.owner.pointer(address,size)}
    /// GPU resources may use physical addresses or mapped guest addresses.
    /// Every copy is bounded by the native backing owner; no pointer escapes.
    pub fn read_gpu(&mut self,address:u32,bytes:&mut[u8])->bool{
        let Ok(size)=u32::try_from(bytes.len())else{return false;};
        let pointer=self.physical_pointer(address,size);
        if pointer.is_null(){return AddressSpace::read_bytes(self,address,bytes);}
        unsafe{std::ptr::copy_nonoverlapping(pointer,bytes.as_mut_ptr(),bytes.len());}true
    }
    pub fn write_gpu(&mut self,address:u32,bytes:&[u8])->bool{
        let Ok(size)=u32::try_from(bytes.len())else{return false;};
        let pointer=self.physical_pointer(address,size);
        if pointer.is_null(){return self.write_bytes(address,bytes);}
        unsafe{std::ptr::copy_nonoverlapping(bytes.as_ptr(),pointer,bytes.len());}true
    }
    pub fn map(&mut self,mapping:Mapping)->Result<(),String> {
        let pages=page_span(mapping.base,mapping.size).ok_or("mapping overflows address space")?;
        if mapping.base%PAGE_SIZE!=0||mapping.size%PAGE_SIZE!=0||mapping.physical%PAGE_SIZE!=0{return Err("unaligned mapping".into());}
        let pointer=self.owner.pointer(mapping.physical,mapping.size);
        if pointer.is_null(){return Err("mapping has no physical backing".into());}
        if !self.owner.map(mapping){return Err("native mapping rejected".into());}
        for (offset,page) in pages.enumerate() {
            let pointer=unsafe{pointer.add(offset*PAGE_SIZE as usize)};
            self.read[page]=if mapping.permission&1!=0{pointer}else{std::ptr::null_mut()};
            self.write[page]=if mapping.permission&2!=0{pointer}else{std::ptr::null_mut()};
        }
        self.mappings.insert(mapping.base,mapping);Ok(())
    }
    pub fn unmap(&mut self,address:u32,size:u32)->Result<(),String> {
        if address%4096!=0||size%4096!=0||page_span(address,size).is_none(){return Err("invalid unmapping range".into());}
        if !self.owner.unmap(address,size){return Err("native unmapping rejected".into());}
        let mappings=self.owner.mappings();
        self.read.fill(std::ptr::null_mut());self.write.fill(std::ptr::null_mut());self.mappings.clear();
        for mapping in mappings {
            let pointer=self.owner.pointer(mapping.physical,mapping.size);
            if pointer.is_null(){return Err("native mapping lost backing".into());}
            let pages=page_span(mapping.base,mapping.size).ok_or("native mapping overflow")?;
            for(offset,page)in pages.enumerate(){
                let pointer=unsafe{pointer.add(offset*PAGE_SIZE as usize)};
                self.read[page]=if mapping.permission&1!=0{pointer}else{std::ptr::null_mut()};
                self.write[page]=if mapping.permission&2!=0{pointer}else{std::ptr::null_mut()};
            }
            self.mappings.insert(mapping.base,mapping);
        }Ok(())
    }
    pub fn mapping_at(&self,address:u32)->Option<Mapping> {
        let mapping=*self.mappings.range(..=address).next_back()?.1;
        ((address as u64)<mapping.base as u64+mapping.size as u64).then_some(mapping)
    }
    pub fn query(&self,address:u32)->crate::process::svc::MemoryInfo {
        use crate::process::svc::MemoryInfo;
        if let Some(mapping)=self.mapping_at(address){return MemoryInfo{base:mapping.base,size:mapping.size,permission:mapping.permission,state:mapping.state};}
        let base=self.mappings.range(..=address).next_back().map_or(0,|(_,m)|m.base.saturating_add(m.size));
        let end=self.mappings.range(address..).next().map_or(0xfffff000,|(&base,_)|base);
        MemoryInfo{base,size:end.saturating_sub(base),permission:0,state:0}
    }
    pub fn read_bytes(&self,address:u32,bytes:&mut[u8])->bool {
        if !self.check_span(address,bytes.len(),false){return false;}
        let mut copied=0;
        while copied<bytes.len(){
            let position=address+copied as u32;let offset=(position&4095)as usize;
            let count=(4096-offset).min(bytes.len()-copied);
            unsafe{std::ptr::copy_nonoverlapping(self.read[(position>>12)as usize].add(offset),bytes[copied..].as_mut_ptr(),count);}
            copied+=count;
        }true
    }
    pub fn write_bytes(&mut self,address:u32,bytes:&[u8])->bool {
        if !self.check_span(address,bytes.len(),true){return false;}
        let mut copied=0;
        while copied<bytes.len(){
            let position=address+copied as u32;let offset=(position&4095)as usize;
            let count=(4096-offset).min(bytes.len()-copied);
            unsafe{std::ptr::copy_nonoverlapping(bytes[copied..].as_ptr(),self.write[(position>>12)as usize].add(offset),count);}
            copied+=count;
        }true
    }
    fn read_value<const N:usize>(&self,address:u32)->[u8;N] {
        let mut bytes=[0;N];assert!(self.read_bytes(address,&mut bytes),"unmapped native read at {address:08x}");bytes
    }
}
impl ServiceMemory for AddressSpace {
    fn check_span(&self,address:u32,size:usize,writable:bool)->bool {
        let Some(pages)=u32::try_from(size).ok().and_then(|size|page_span(address,size))else{return false;};
        let table=if writable{&self.write}else{&self.read};pages.into_iter().all(|page|!table[page].is_null())
    }
    fn read_span(&mut self,address:u32,bytes:&mut[u8])->bool{AddressSpace::read_bytes(self,address,bytes)}
    fn write_span(&mut self,address:u32,bytes:&[u8])->bool{self.write_bytes(address,bytes)}
}
impl Bus for AddressSpace {
    fn read8(&mut self,a:u32)->u8{self.read_value::<1>(a)[0]}
    fn read16(&mut self,a:u32)->u16{u16::from_le_bytes(self.read_value(a))}
    fn read32(&mut self,a:u32)->u32{u32::from_le_bytes(self.read_value(a))}
    fn write8(&mut self,a:u32,v:u8){assert!(self.write_bytes(a,&[v]),"unmapped native byte write");}
    fn write16(&mut self,a:u32,v:u16){assert!(self.write_bytes(a,&v.to_le_bytes()),"unmapped native halfword write");}
    fn write32(&mut self,a:u32,v:u32){assert!(self.write_bytes(a,&v.to_le_bytes()),"unmapped native word write");}
}
impl AotMemory for AddressSpace {
    fn page_tables(&self)->(*const *mut u8,*const *mut u8){(self.read.as_ptr(),self.write.as_ptr())}
}
impl crate::ipc::message::IpcMemory for AddressSpace {
    fn read_word(&mut self,address:u32)->u32 {
        assert!(address%4==0&&self.check_span(address,4,false),"invalid native IPC read");
        Bus::read32(self,address)
    }
    fn write_word(&mut self,address:u32,value:u32) {
        assert!(address%4==0&&self.check_span(address,4,true),"invalid native IPC write");
        Bus::write32(self,address,value)
    }
}
impl crate::services::fs_ipc::FsMemory for AddressSpace {
    fn query(&self,address:u32)->crate::process::svc::MemoryInfo{AddressSpace::query(self,address)}
}
impl GuestMemory for AddressSpace {
    fn read8(&mut self,a:u32)->u8{Bus::read8(self,a)}
    fn read32(&mut self,a:u32)->u32{Bus::read32(self,a)}
    fn read_bytes(&mut self,a:u32,bytes:&mut[u8]){assert!(AddressSpace::read_bytes(self,a,bytes),"invalid native byte span");}
    fn write8(&mut self,a:u32,v:u8){Bus::write8(self,a,v)}
    fn write32(&mut self,a:u32,v:u32){Bus::write32(self,a,v)}
    fn write32_privileged(&mut self,a:u32,v:u32)->bool {
        let mut pointers=[std::ptr::null_mut();4];
        for (offset,pointer) in pointers.iter_mut().enumerate(){
            let Some(address)=a.checked_add(offset as u32)else{return false;};
            let Some(mapping)=self.mapping_at(address)else{return false;};
            *pointer=self.owner.pointer(mapping.physical+(address-mapping.base),1);
            if pointer.is_null(){return false;}
        }
        for(pointer,byte)in pointers.into_iter().zip(v.to_le_bytes()){unsafe{*pointer=byte;}}
        true
    }
}
impl crate::process::image::ImageMemory for AddressSpace {
    fn install(&mut self,segment:&crate::process::image::ImageSegment,bytes:&[u8])->Result<(),crate::process::image::ImageError> {
        use crate::process::image::ImageError;
        if bytes.len()>segment.mapped_size as usize{return Err(ImageError("image bytes exceed mapping".into()));}
        if segment.mapped_size==0{return Ok(());}
        let physical=self.allocate(self.image_region,segment.mapped_size).ok_or_else(||ImageError("native image allocation failed".into()))?;
        let pointer=self.physical_pointer(physical,segment.mapped_size);
        if pointer.is_null(){return Err(ImageError("native image backing absent".into()));}
        unsafe{std::ptr::write_bytes(pointer,0,segment.mapped_size as usize);std::ptr::copy_nonoverlapping(bytes.as_ptr(),pointer,bytes.len());}
        let state=match segment.name{".text"|".rodata"|".data"=>4,"stack"=>11,_=>5};
        self.map(Mapping{base:segment.address,size:segment.mapped_size,physical,permission:segment.permissions,state}).map_err(ImageError)
    }
}
impl crate::process::svc::QueryMemory for AddressSpace {
    fn query_memory(&self,address:u32)->crate::process::svc::MemoryInfo{self.query(address)}
}
impl crate::process::svc::ThreadMemory for AddressSpace {
    fn map_tls(&mut self,tls:u32)->Result<(),u32>{
        let page=tls&!4095;
        if self.mapping_at(page).is_some(){return Ok(());}
        let physical=self.allocate(2,4096).ok_or(0xd86007f3u32)?;
        let pointer=self.physical_pointer(physical,4096);
        if pointer.is_null(){self.owner.free(physical,4096);return Err(0xe0e01bf5);}
        unsafe{std::ptr::write_bytes(pointer,0,4096);}
        if self.map(Mapping{base:page,size:4096,physical,permission:3,state:11}).is_err(){
            self.owner.free(physical,4096);return Err(0xe0e01bf5);
        }
        Ok(())
    }
}
impl crate::process::svc::SharedBacking for AddressSpace {
    fn shared_backing(&mut self,address:u32,size:u32)->Result<u32,u32>{
        if size==0||size%4096!=0{return Err(0xe0e01bf2);}
        if address%4096!=0{return Err(0xe0e01bf1);}
        if address==0{return self.allocate(2,size).ok_or(0xd86007f3);}
        let end=address.checked_add(size).ok_or(0xe0e01bf5u32)?;
        let first=self.mapping_at(address).ok_or(0xe0e01bf5u32)?;
        let physical=first.physical.checked_add(address-first.base).ok_or(0xe0e01bf5u32)?;
        let mut cursor=address;
        while cursor<end{
            let mapping=self.mapping_at(cursor).ok_or(0xe0e01bf5u32)?;
            if mapping.physical.checked_add(cursor-mapping.base)!=physical.checked_add(cursor-address){return Err(0xe0e01bf5);}
            let limit=mapping.base.checked_add(mapping.size).ok_or(0xe0e01bf5u32)?;
            if limit<=cursor{return Err(0xe0e01bf5);}cursor=limit.min(end);
        }
        Ok(physical)
    }
}
unsafe impl crate::services::dsp::DspMemory for AddressSpace {
    fn sample_pointer(&mut self,address:u32)->*const u8{self.physical_pointer(address,1)}
}
impl crate::process::svc::SharedMapping for AddressSpace {
    fn map_shared(&mut self,address:u32,physical:u32,size:u32,permission:u32)->Result<u32,u32>{
        if size==0||size%4096!=0{return Err(0xe0e01bf2);}
        if address%4096!=0{return Err(0xe0e01bf1);}
        let mut chosen=address;
        if chosen==0 {
            chosen=0x10000000;
            loop {
                let end=chosen.checked_add(size).ok_or(0xe0e01bf5u32)?;
                if (chosen..end).step_by(4096).all(|page|self.mapping_at(page).is_none()){break;}
                chosen=chosen.checked_add(4096).ok_or(0xe0e01bf5u32)?;
            }
        }
        chosen.checked_add(size).ok_or(0xe0e01bf5u32)?;
        self.map(Mapping{base:chosen,physical,size,permission:(permission&7)|1,state:6}).map_err(|_|0xe0e01bf5u32)?;
        Ok(chosen)
    }
}
impl crate::process::svc::AllocationMemory<u32> for AddressSpace {
    fn allocate_region(&mut self,region:&u32,size:u32)->Result<u32,u32>{
        self.allocate(*region,size).ok_or(0xd86007f3)
    }
    fn install_allocation(&mut self,address:u32,physical:u32,size:u32,permission:u32,linear:bool){
        self.map(Mapping{base:address,physical,size,permission:(permission&7)|1,state:if linear{7}else{5}})
            .expect("native process allocation mapping rejected");
    }
}
impl crate::process::loader::ProcessMemory for AddressSpace {
    fn mirror_module(&mut self,destination:u32,source:u32,size:u32)->bool {
        let Some(size)=size.checked_add(4095).map(|size|size&!4095)else{return false;};
        if destination%4096!=0||source%4096!=0||destination.checked_add(size).is_none()||!self.check_span(source,size as usize,false){return false;}
        let mut copied=0;
        while copied<size {
            let Some(mapping)=self.mapping_at(source+copied)else{return false;};
            let offset=source+copied-mapping.base;let chunk=(mapping.size-offset).min(size-copied);
            if self.map(Mapping{base:destination+copied,physical:mapping.physical+offset,size:chunk,permission:mapping.permission,state:mapping.state}).is_err(){return false;}
            copied+=chunk;
        }true
    }
    fn copy_module(&mut self,destination:u32,source:u32,size:u32)->Option<crate::PageAllocation> {
        if !self.check_span(source,size as usize,false){return None;}
        let mapped=size.checked_add(4095)?&!4095;
        let physical=self.allocate(1,mapped)?;
        let mut bytes=vec![0;size as usize];if !AddressSpace::read_bytes(self,source,&mut bytes){return None;}
        let pointer=self.physical_pointer(physical,mapped);if pointer.is_null(){return None;}
        unsafe{std::ptr::write_bytes(pointer,0,mapped as usize);std::ptr::copy_nonoverlapping(bytes.as_ptr(),pointer,bytes.len());}
        if self.map(Mapping{base:destination,physical,size:mapped,permission:7,state:4}).is_err(){self.owner.free(physical,mapped);return None;}
        Some(crate::PageAllocation{addr:physical,size:mapped})
    }
    fn write_bytes(&mut self,address:u32,bytes:&[u8]){assert!(AddressSpace::write_bytes(self,address,bytes),"native module write span invalid");}
    fn trim_module(&mut self,address:u32,block:crate::PageAllocation,kept:u32) {
        assert!(kept<=block.size&&kept%4096==0,"invalid native module trim");
        self.unmap(address,block.size).expect("native module trim unmap");
        if kept!=0{self.map(Mapping{base:address,physical:block.addr,size:kept,permission:7,state:4}).expect("native module trim map");}
        assert!(self.owner.free(block.addr+kept,block.size-kept),"native module tail free");
    }
    fn release_module(&mut self,address:u32,block:crate::PageAllocation) {
        self.unmap(address,block.size).expect("native module release unmap");
        assert!(self.owner.free(block.addr,block.size),"native module backing free");
    }
}
