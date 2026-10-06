//! Native process-image descriptors/install, following TriAevum's
//! SourceProcessImageDescriptor/SourceImageSegment. No emulator loader calls.
use std::{path::{Path,PathBuf},fmt};
use serde::Deserialize;
use sha2::{Digest,Sha256};
#[derive(Debug)]pub struct ImageError(pub String);
impl fmt::Display for ImageError{fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str(&self.0)}}
impl std::error::Error for ImageError{}
impl<E:ToString> From<&E> for ImageError{fn from(e:&E)->Self{Self(e.to_string())}}
#[derive(Deserialize)]
pub struct TitleProfile{pub name:String,pub program_id:String,pub region:String,pub revision:String,pub code_sha256:String,pub stack_top:u32}
impl TitleProfile{pub fn load(path:&Path)->Result<Self,ImageError>{
    serde_json::from_slice(&std::fs::read(path).map_err(|e|ImageError::from(&e))?).map_err(|e|ImageError::from(&e))
}}
pub struct ImageSegment{pub name:&'static str,pub address:u32,pub mapped_size:u32,pub file_offset:u32,pub file_size:u32,pub permissions:u32}
pub trait ImageMemory{fn install(&mut self,segment:&ImageSegment,bytes:&[u8])->Result<(),ImageError>;}
pub struct NativeProcessImage{
    pub title:recomp3ds::rom::Title,pub path:PathBuf,pub profile:TitleProfile,
    pub segments:Vec<ImageSegment>,pub code:Vec<u8>,pub entry:u32,pub stack_size:u32,
    pub priority:u32,pub processor:i32,pub app_bytes:u32,pub system_mode:u8,pub memory_region:u32,
}
fn u32_at(bytes:&[u8],offset:usize)->u32{u32::from_le_bytes(bytes[offset..offset+4].try_into().unwrap())}
fn page_align(value:u32)->Result<u32,ImageError>{value.checked_add(4095).map(|v|v&!4095).ok_or_else(||ImageError("image size overflow".into()))}
fn validate_segments(segments:&[ImageSegment])->Result<(),ImageError>{
    for (i,s) in segments.iter().enumerate(){
        let end=s.address.checked_add(s.mapped_size).ok_or_else(||ImageError("segment address overflow".into()))?;
        if s.address%4096!=0||s.mapped_size%4096!=0||s.file_size>s.mapped_size{return Err(ImageError("invalid segment extent".into()));}
        for other in &segments[..i]{
            let other_end=other.address.checked_add(other.mapped_size).ok_or_else(||ImageError("segment address overflow".into()))?;
            if s.address<other_end&&other.address<end{return Err(ImageError("overlapping process segments".into()));}
        }
    }Ok(())
}
impl NativeProcessImage{
    pub fn open(path:&Path,profile:TitleProfile)->Result<Self,ImageError>{
        let title=recomp3ds::rom::Title::load(path).map_err(|e|ImageError::from(&e))?;
        let id=u64::from_str_radix(&profile.program_id,16).map_err(|e|ImageError::from(&e))?;
        if title.program_id()!=id{return Err(ImageError("input identity differs from title profile".into()));}
        let code=title.code().map_err(|e|ImageError::from(&e))?;
        let hash=Sha256::digest(&code).iter().map(|v|format!("{v:02x}")).collect::<String>();
        if hash!=profile.code_sha256.to_ascii_lowercase(){return Err(ImageError(format!("input code SHA256 differs from AOT profile: {hash}")));}
        let ex=&title.exheader;let raw=title.extended_header();
        if raw.len()!=0x400{return Err(ImageError("short extended header".into()));}
        let priority=raw[0x20f]as u32;let processor=(raw[0x20e]&3)as i32;
        if priority>0x3f{return Err(ImageError("invalid initial thread priority".into()));}
        let system_mode=raw[0x20e]>>4;let app_bytes=match system_mode{0|1=>64,2=>96,3=>80,4=>72,5=>32,_=>return Err(ImageError("unsupported process memory split".into()))}*1024*1024;
        let mut memory_region=1;
        for i in 0..28{let desc=u32_at(raw,0x370+i*4);if desc!=u32::MAX&&(desc>>20)&0xff8==0xff0{memory_region=(desc>>8)&15;}}
        if !(1..=3).contains(&memory_region){return Err(ImageError("invalid process memory region".into()));}
        let mut segments=Vec::new();let mut offset=0u32;
        for(name,info,permissions)in[(".text",ex.text,5),(".rodata",ex.rodata,1),(".data",ex.data,3)]{
            let mapped_size=info.num_pages.checked_mul(4096).ok_or_else(||ImageError("segment pages overflow".into()))?;
            let end=offset.checked_add(info.size).ok_or_else(||ImageError("segment file offset overflow".into()))?;
            if info.address%4096!=0||info.size>mapped_size||end as usize>code.len()||info.address.checked_add(mapped_size).is_none(){return Err(ImageError(format!("invalid {name} region")));}
            if mapped_size>0{segments.push(ImageSegment{name,address:info.address,mapped_size,file_offset:offset,file_size:info.size,permissions});}
            offset=offset.checked_add(mapped_size).ok_or_else(||ImageError("image file offset overflow".into()))?;
        }
        let bss_start=ex.data.address.checked_add(ex.data.num_pages*4096).ok_or_else(||ImageError("BSS start overflow".into()))?;
        let bss_end=ex.data.address.checked_add(ex.data.size).and_then(|v|v.checked_add(ex.bss_size)).ok_or_else(||ImageError("BSS end overflow".into()))?;
        if bss_end>bss_start{segments.push(ImageSegment{name:".bss",address:bss_start,mapped_size:page_align(bss_end-bss_start)?,file_offset:0,file_size:0,permissions:3});}
        let stack_size=page_align(u32_at(raw,0x1c).max(0x4000))?;
        let stack_start=profile.stack_top.checked_sub(stack_size).ok_or_else(||ImageError("stack underflow".into()))?;
        if profile.stack_top%4096!=0{return Err(ImageError("stack not page aligned".into()));}
        segments.push(ImageSegment{name:"stack",address:stack_start,mapped_size:stack_size,file_offset:0,file_size:0,permissions:3});
        validate_segments(&segments)?;
        let entry=ex.text.address;
        Ok(Self{title,path:path.to_owned(),profile,segments,code,entry,stack_size,priority,processor,app_bytes,system_mode,memory_region})
    }
    pub fn install(&self,memory:&mut impl ImageMemory)->Result<(),ImageError>{
        for s in &self.segments{let bytes=if s.file_size==0{&[][..]}else{&self.code[s.file_offset as usize..(s.file_offset+s.file_size)as usize]};memory.install(s,bytes)?;}Ok(())
    }
}
#[cfg(test)]mod tests{
    use super::*;
    fn segment(address:u32,size:u32)->ImageSegment{ImageSegment{name:"test",address,mapped_size:size,file_offset:0,file_size:0,permissions:3}}
    #[test]fn adjacent_segments_valid(){assert!(validate_segments(&[segment(0x1000,4096),segment(0x2000,4096)]).is_ok());}
    #[test]fn overlap_rejected(){assert!(validate_segments(&[segment(0x1000,8192),segment(0x2000,4096)]).is_err());}
    #[test]fn overflow_rejected(){assert!(validate_segments(&[segment(0xfffff000,8192)]).is_err());}
    #[test]fn page_alignment_checked(){assert!(validate_segments(&[segment(1,4096)]).is_err());assert!(page_align(u32::MAX).is_err());}
}
