//! Native ldr:ro ownership, bounded CRR registration and CRO relocation.
//! Trusted-local-dump policy: CRR RSA signatures are not authenticated against
//! Nintendo firmware keys. Module hash-table membership IS checked before load.
use super::cro::{NativeModuleRegistry,Copy,format::header};
use crate::{GuestAddress,GuestMemory,PageAllocation};
use std::collections::{HashMap,HashSet};
use sha2::{Digest,Sha256};

pub trait ProcessMemory: GuestMemory {
    fn mirror_module(&mut self,address:u32,source:u32,size:u32)->bool;
    fn copy_module(&mut self,address:u32,source:u32,size:u32)->Option<PageAllocation>;
    fn write_bytes(&mut self,address:u32,bytes:&[u8]);
    fn trim_module(&mut self,address:u32,block:PageAllocation,kept:u32);
    fn release_module(&mut self,address:u32,block:PageAllocation);
}
#[cfg(test)]mod tests{
    use super::*;
    struct Memory(Vec<u8>);
    impl GuestMemory for Memory{
        fn read8(&mut self,a:u32)->u8{self.0[a as usize]}
        fn read32(&mut self,a:u32)->u32{u32::from_le_bytes(self.0[a as usize..a as usize+4].try_into().unwrap())}
        fn read_bytes(&mut self,a:u32,b:&mut[u8]){b.copy_from_slice(&self.0[a as usize..a as usize+b.len()]);}
        fn write8(&mut self,a:u32,v:u8){self.0[a as usize]=v;}
        fn write32(&mut self,a:u32,v:u32){self.0[a as usize..a as usize+4].copy_from_slice(&v.to_le_bytes());}
        fn write32_privileged(&mut self,a:u32,v:u32)->bool{self.write32(a,v);true}
    }
    impl ProcessMemory for Memory{
        fn mirror_module(&mut self,a:u32,s:u32,n:u32)->bool{let b=self.0[s as usize..(s+n)as usize].to_vec();self.write_bytes(a,&b);true}
        fn copy_module(&mut self,a:u32,s:u32,n:u32)->Option<PageAllocation>{self.mirror_module(a,s,n);Some(PageAllocation{addr:a,size:n})}
        fn write_bytes(&mut self,a:u32,b:&[u8]){self.0[a as usize..a as usize+b.len()].copy_from_slice(b);}
        fn trim_module(&mut self,_:u32,_:PageAllocation,_:u32){}
        fn release_module(&mut self,_:u32,_:PageAllocation){}
    }
    #[test]fn bounded_certificate_and_native_static_owner(){
        let mut m=Memory(vec![0;0x10000]);let mut l=ModuleLoader::default();let program=0x0004000000175e00;
        m.write_bytes(0x1080,b"CRS0");m.write32(0x1090,0x1000);
        m.write32(0x10c0,0x140);m.write32(0x10c4,7);m.write_bytes(0x1140,b"static\0");
        let mut w=[0u32;64];w[0]=0x100c2;w[1]=0x1000;w[2]=0x1000;w[3]=0x1000;
        l.dispatch(&mut w,&mut m,program).unwrap();assert_eq!(l.registry.modules[0].name,"static");
        m.write_bytes(0x3000,b"CRR0");m.write32(0x3340,0x175e00);m.write32(0x3344,0x380);
        m.write32(0x3350,0x360);m.write32(0x3354,1);
        assert!(l.register_crr(&mut m,0x3000,0x380,program).is_ok());
        m.write32(0x3354,u32::MAX);assert_eq!(l.register_crr(&mut m,0x3000,0x380,program),Err(LoaderError::Malformed));
        m.write32(0x3354,1);m.write32(0x3340,0x175d00);assert_eq!(l.register_crr(&mut m,0x3000,0x380,program),Err(LoaderError::Malformed));
        // A structurally valid dynamic module not in a CRR must not execute.
        m.write_bytes(0x5080,b"CRO0");m.write32(0x5090,0x1000);
        w.fill(0);w[0]=0x902c2;w[1]=0x5000;w[2]=0x6000;w[3]=0x1000;
        assert_eq!(l.dispatch(&mut w,&mut m,program),Err(LoaderError::Unattested));
        m.write32(0x50b0,0xfffffff0);m.write32(0x50b4,0x30);
        assert!(!ModuleLoader::validate_module(&mut m,0x5000,0x1000));
    }
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct ModulePlacement {pub name:String,pub base:GuestAddress}
#[derive(Default)]
pub struct ModuleLoader {
    pub registry:NativeModuleRegistry,
    certificates:HashMap<u32,HashSet<[u8;32]>>,
    pub commands:u64,
    pub crr_registrations:u64,
    pub cro_loads:u64,
}
/// Process-owned loader service lifetime, independent of the host application.
#[derive(Default)]
pub struct LoaderState { pub loader: ModuleLoader }
impl Drop for LoaderState {
    fn drop(&mut self) {
        if self.loader.commands > 0 {
            log::info!("NATIVE_CRO_COUNTERS commands={} crr_registrations={} cro_loads={} zakuro_cro_calls=0",
                self.loader.commands,self.loader.crr_registrations,self.loader.cro_loads);
        }
    }
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum LoaderError {Unsupported,Malformed,InvalidModule,Unattested,OutOfMemory}
impl ModuleLoader {
    fn validate_module(memory:&mut dyn ProcessMemory,source:u32,size:u32)->bool {
        if size<header::SIZE || source.checked_add(size).is_none(){return false;}
        let declared=memory.read32(source+header::FILE_SIZE);
        if declared>size||declared<header::SIZE{return false;}
        // All on-disk table offsets/counts must remain inside the supplied file.
        const ENTRIES:[u32;17]=[1,1,1,12,8,4,1,8,20,12,8,8,8,1,8,12,12];
        for (i,width) in ENTRIES.into_iter().enumerate(){
            let field=source+header::FIRST_REBASED+i as u32*8;
            let offset=memory.read32(field);let count=memory.read32(field+4);
            if count!=0 && (offset<header::SIZE || count.checked_mul(width)
                .and_then(|n|offset.checked_add(n)).is_none_or(|end|end>size)) {return false;}
        }
        true
    }
    fn register_crr(&mut self,memory:&mut dyn ProcessMemory,address:u32,size:u32,program:u64)->Result<(),LoaderError>{
        if size<0x360||address.checked_add(size).is_none(){return Err(LoaderError::Malformed);}
        let mut magic=[0u8;4];memory.read_bytes(address,&mut magic);
        if &magic!=b"CRR0"{return Err(LoaderError::Malformed);}
        let mask=memory.read32(address+0x20);let pattern=memory.read32(address+0x24);
        let identity=(program as u32)&0xffffff00;
        if identity&mask!=pattern || memory.read32(address+0x340)!=identity{return Err(LoaderError::Malformed);}
        let declared=memory.read32(address+0x344);
        let offset=memory.read32(address+0x350);let count=memory.read32(address+0x354);
        if declared>size||declared<0x360||offset<0x360||
            count.checked_mul(32).and_then(|n|offset.checked_add(n)).is_none_or(|end|end>declared){return Err(LoaderError::Malformed);}
        let mut hashes=HashSet::new();
        for i in 0..count{let mut hash=[0u8;32];memory.read_bytes(address+offset+i*32,&mut hash);hashes.insert(hash);}
        self.certificates.insert(address,hashes);self.crr_registrations+=1;Ok(())
    }
    /// Return module placement changes for the AOT entry-point table owner.
    /// Service state and relocation are independent of the host composition root.
    pub fn dispatch(&mut self,words:&mut[u32],memory:&mut dyn ProcessMemory,program:u64)->Result<Vec<ModulePlacement>,LoaderError>{
        if words.len()<16{return Err(LoaderError::Malformed);}
        let command=words[0]>>16;let normal=(words[0]>>6)&63;let translated=words[0]&63;
        let expected=match command{1=>3,2|3=>2,4|9=>11,5=>3,_=>return Err(LoaderError::Unsupported)};
        if normal!=expected||translated!=2||words[1+normal as usize]!=0{return Err(LoaderError::Malformed);}
        self.commands+=1;
        let mut changes=Vec::new();let mut reply=Vec::new();
        match command{
            1=>{
                let(source,size,address)=(words[1],words[2],words[3]);
                if !Self::validate_module(memory,source,size){return Err(LoaderError::InvalidModule);}
                let base=if address==0||address==source{source}else{
                    if !memory.mirror_module(address,source,size){return Err(LoaderError::OutOfMemory);}address
                };
                self.registry.initialize(memory,base,size).map_err(|_|LoaderError::InvalidModule)?;
            }
            2=>self.register_crr(memory,words[1],words[2],program)?,
            3=>{if self.certificates.remove(&words[1]).is_none(){return Err(LoaderError::Malformed);}},
            4|9=>{
                let(source,address,size)=(words[1],words[2],words[3]);
                if !Self::validate_module(memory,source,size){return Err(LoaderError::InvalidModule);}
                let mut hashes=[0u8;128];memory.read_bytes(source,&mut hashes);
                let digest:[u8;32]=Sha256::digest(hashes).into();
                if !self.certificates.values().any(|table|table.contains(&digest)){return Err(LoaderError::Unattested);}
                let base=if address==0||address==source{source}else{
                    let block=memory.copy_module(address,source,size).ok_or(LoaderError::OutOfMemory)?;
                    self.registry.copies.insert(address,Copy{buffer:source,block,fixed:size});address
                };
                let fixed=self.registry.load(memory,base,size,words[4],words[6],words[7],words[8],words[9]!=0,words[10])
                    .map_err(|_|LoaderError::InvalidModule)?;
                if let Some(copy)=self.registry.copies.get_mut(&base){
                    let mut bytes=vec![0u8;size as usize];memory.read_bytes(base,&mut bytes);memory.write_bytes(copy.buffer,&bytes);
                    copy.fixed=if fixed==0{size}else{fixed};let kept=(copy.fixed.saturating_add(0xfff)&!0xfff).min(copy.block.size);
                    if kept<copy.block.size{memory.trim_module(base,copy.block,kept);copy.block.size=kept;}
                }
                if let Some(module)=self.registry.modules.last(){changes.push(ModulePlacement{name:module.name.clone(),base});}
                self.cro_loads+=1;reply.push(fixed);
            }
            5=>{
                let address=words[1];
                if let Some(module)=self.registry.modules.iter().find(|module|module.base==address){
                    changes.push(ModulePlacement{name:module.name.clone(),base:0});
                }else{return Err(LoaderError::InvalidModule);}
                self.registry.unload(memory,address);
                if let Some(copy)=self.registry.copies.remove(&address){
                    let mut bytes=vec![0u8;copy.fixed.min(copy.block.size)as usize];memory.read_bytes(address,&mut bytes);
                    memory.write_bytes(copy.buffer,&bytes);memory.release_module(address,copy.block);
                }
            }
            _=>unreachable!(),
        }
        words[0]=(command<<16)|((1+reply.len()as u32)<<6);words[1]=0;
        words[2..2+reply.len()].copy_from_slice(&reply);Ok(changes)
    }
}
