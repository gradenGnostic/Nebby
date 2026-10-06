// SPDX-License-Identifier: MIT
// Copyright (c) 2026 fearkov. Narrow CTR wire-format extraction; checked native
// memory contract replaces the concrete emulator address-space dependency.
pub trait IpcMemory{fn read_word(&mut self,address:u32)->u32;fn write_word(&mut self,address:u32,value:u32);}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub struct Header(pub u32);
impl Header{
    pub const fn new(id:u16,normal:u32,translate:u32)->Self{Self((id as u32)<<16|((normal&63)<<6)|(translate&63))}
    pub const fn command_id(self)->u16{(self.0>>16)as u16}
    pub const fn normal_params(self)->u32{(self.0>>6)&63}
    pub const fn translate_params(self)->u32{self.0&63}
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Descriptor{
    Handles{count:u32,close:bool},CurrentProcessId,StaticBuffer{size:u32,buffer_id:u32},MappedBuffer{size:u32,permission:u32},Unknown(u32),
}
impl Descriptor{
    pub fn parse(raw:u32)->Self{match raw&15{
        0=>if raw&32!=0{Self::CurrentProcessId}else{Self::Handles{count:((raw>>26)&63)+1,close:raw&16!=0}},
        2=>Self::StaticBuffer{size:(raw>>14)&0x3ffff,buffer_id:(raw>>10)&15},
        8|10|12=>Self::MappedBuffer{size:raw>>4,permission:raw&15},_=>Self::Unknown(raw),
    }}
    pub const fn handles(count:u32)->u32{assert!(count>0&&count<=64);(count-1)<<26}
    pub const fn move_handles(count:u32)->u32{Self::handles(count)|16}
    pub const fn static_buffer(size:u32,id:u32)->u32{assert!(size<=0x3ffff&&id<16);size<<14|id<<10|2}
}
pub struct CommandBuffer{base:u32}
impl CommandBuffer{
    pub fn new(tls:u32)->Self{Self{base:tls.checked_add(0x80).expect("IPC TLS address overflow")}}
    pub fn address(&self)->u32{self.base}
    fn word_address(&self,index:u32)->u32{assert!(index<64,"IPC command buffer overflow");self.base.checked_add(index*4).expect("IPC address overflow")}
    pub fn get(&self,m:&mut impl IpcMemory,index:u32)->u32{m.read_word(self.word_address(index))}
    pub fn set(&self,m:&mut impl IpcMemory,index:u32,value:u32){m.write_word(self.word_address(index),value);}
    pub fn header(&self,m:&mut impl IpcMemory)->Header{Header(self.get(m,0))}
    pub fn reply(&self,m:&mut impl IpcMemory,id:u16,values:&[u32]){
        assert!(values.len()<=62);self.set(m,0,Header::new(id,values.len()as u32+1,0).0);self.set(m,1,0);
        for(i,&v)in values.iter().enumerate(){self.set(m,i as u32+2,v);}
    }
    pub fn reply_error(&self,m:&mut impl IpcMemory,id:u16,result:u32){self.set(m,0,Header::new(id,1,0).0);self.set(m,1,result);}
    pub fn reply_with_handle(&self,m:&mut impl IpcMemory,id:u16,handle:u32){
        self.set(m,0,Header::new(id,1,2).0);self.set(m,1,0);self.set(m,2,Descriptor::move_handles(1));self.set(m,3,handle);
    }
    pub fn static_buffer(&self,m:&mut impl IpcMemory,tls:u32,id:u32)->(u32,u32){
        assert!(id<16);let base=tls.checked_add(0x180+id*8).expect("IPC static table overflow");
        let descriptor=m.read_word(base);let pointer=m.read_word(base.checked_add(4).unwrap());(pointer,(descriptor>>14)&0x3ffff)
    }
}
#[cfg(test)]mod tests{
    use super::*;
    struct Memory([u32;128]);impl IpcMemory for Memory{fn read_word(&mut self,a:u32)->u32{self.0[a as usize/4]}fn write_word(&mut self,a:u32,v:u32){self.0[a as usize/4]=v;}}
    #[test]fn headers_descriptors_and_replies(){
        assert_eq!(Header::new(5,4,0).0,0x50100);assert_eq!(Descriptor::parse(Descriptor::move_handles(1)),Descriptor::Handles{count:1,close:true});
        assert_eq!(Descriptor::parse(Descriptor::static_buffer(32,3)),Descriptor::StaticBuffer{size:32,buffer_id:3});
        let mut m=Memory([0;128]);let b=CommandBuffer::new(0);b.reply_with_handle(&mut m,5,0x123);
        assert_eq!(b.header(&mut m),Header::new(5,1,2));assert_eq!(b.get(&mut m,3),0x123);
        m.write_word(0x180,Descriptor::static_buffer(32,0));m.write_word(0x184,0x8000);assert_eq!(b.static_buffer(&mut m,0,0),(0x8000,32));
    }
    #[test]#[should_panic]fn rejects_command_overflow(){CommandBuffer::new(0).get(&mut Memory([0;128]),64);}
}
