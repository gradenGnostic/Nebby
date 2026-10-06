// SPDX-License-Identifier: MIT
// Copyright (c) 2026 fearkov. Narrow CTR shared-page protocol extraction.
//! Native CTR process-visible kernel pages; title/bootstrap supplies metadata.
use crate::memory::{address_space::AddressSpace,owner::Mapping};
pub struct KernelPages {
    pub boot_clock_ms:u64,pub new3ds:bool,pub slider:f32,
    pub application_bytes:u32,pub system_bytes:u32,pub base_bytes:u32,
    pub memory_type:u32,pub mac:[u8;6],
}
fn word(page:&mut[u8],offset:usize,value:u32){page[offset..offset+4].copy_from_slice(&value.to_le_bytes());}
fn wide(page:&mut[u8],offset:usize,value:u64){page[offset..offset+8].copy_from_slice(&value.to_le_bytes());}
impl KernelPages {
    pub fn install(&self,memory:&mut AddressSpace)->Result<(),String>{
        let mut config=[0;4096];
        config[0..3].copy_from_slice(&[0,0x34,2]);word(&mut config,0x10,2);
        config[0x14]=1;config[0x15]=1;word(&mut config,0x18,0xf297);
        for(offset,value)in [(0x30,self.memory_type),(0x40,self.application_bytes),(0x44,self.system_bytes),(0x48,self.base_bytes),(0x64,2),(0x68,0xf297)]{word(&mut config,offset,value);}
        config[0x60..0x63].copy_from_slice(&[0,0x34,2]);
        let mut shared=[0;4096];shared[4]=1;shared[5]=if self.new3ds{2}else{1};
        shared[0x60..0x66].copy_from_slice(&self.mac);shared[0x66]=3;shared[0x67]=2;
        let slider=if self.slider.is_finite(){self.slider.clamp(0.,1.)}else{0.};
        shared[0x80..0x84].copy_from_slice(&slider.to_le_bytes());shared[0x84]=u8::from(slider>0.);shared[0x85]=1|(5<<2);
        self.clock_bytes(&mut shared,0);
        if !memory.write_gpu(0x1ff80000,&config)||!memory.write_gpu(0x1ff81000,&shared){return Err("native kernel page backing unavailable".into());}
        for mapping in [
            Mapping{base:0x1ff80000,physical:0x1ff80000,size:4096,permission:1,state:3},
            Mapping{base:0x1ff81000,physical:0x1ff81000,size:4096,permission:1,state:3},
            Mapping{base:0x1f000000,physical:0x18000000,size:0x600000,permission:3,state:3},
            Mapping{base:0x1ff00000,physical:0x1ff00000,size:0x80000,permission:3,state:3},
        ]{memory.map(mapping)?;}
        Ok(())
    }
    fn clock_bytes(&self,page:&mut[u8],tick:u64){
        let hz=crate::scheduler::CPU_CLOCK_HZ;
        let elapsed=(tick/hz).saturating_mul(1000)+(tick%hz)*1000/hz;
        for slot in [0x20,0x40]{wide(page,slot,self.boot_clock_ms.saturating_add(elapsed));wide(page,slot+8,tick);wide(page,slot+16,0x10000);wide(page,slot+24,0);}
    }
    pub fn update_clock(&self,memory:&mut AddressSpace,tick:u64)->Result<(),String>{
        let mut bytes=[0;4096];if !memory.read_gpu(0x1ff81000,&mut bytes){return Err("native shared page absent".into());}
        self.clock_bytes(&mut bytes,tick);if !memory.write_gpu(0x1ff81000,&bytes){return Err("native shared page update failed".into());}Ok(())
    }
}
