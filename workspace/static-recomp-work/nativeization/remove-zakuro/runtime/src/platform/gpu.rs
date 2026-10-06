//! Native GSP work adapter to the production PICA frontend and guest backing.
use super::pica::PicaFrontend;
use crate::{memory::address_space::AddressSpace,services::{gsp::GspHost,router::ServiceMemory}};
use std::collections::BTreeMap;
pub struct NativeGpu {
    pub frontend:PicaFrontend,
    registers:BTreeMap<u32,u32>,
    pub framebuffers:[[u32;6];2],pub force_black:bool,
    pub rejected:u64,pub last_error:Option<String>,
}
impl NativeGpu {
    pub fn new(frontend:PicaFrontend)->Self{Self{frontend,registers:BTreeMap::new(),framebuffers:[[0;6];2],force_black:false,rejected:0,last_error:None}}
    fn action(&mut self,memory:&mut AddressSpace,operation:u32,argument:u32,words:&[u32])->Result<(),String>{
        match operation {
            0|1=>{
                let count=if operation==1{if words.len()%2!=0{return Err("odd masked-register packet".into());}words.len()/2}else{words.len()};
                if argument>=0x420000||argument%4!=0||count>32{return Err("invalid GPU register packet".into());}
                for index in 0..count{
                    let address=argument.checked_add(index as u32*4).ok_or("GPU register overflow")?;
                    let value=if operation==1{let mask=words[count+index];(self.registers.get(&address).copied().unwrap_or(0)&!mask)|(words[index]&mask)}else{words[index]};
                    self.registers.insert(address,value);
                }
            }
            2 if words.len()==7=>match argument&255 {
                0=>{
                    let size=words[2]as usize;if size>16<<20{return Err("GPU DMA span exceeds limit".into());}
                    let mut bytes=vec![0;size];if !memory.read_gpu(words[0],&mut bytes)||!memory.write_gpu(words[1],&bytes){return Err("GPU DMA backing unavailable".into());}
                }
                1=>{self.frontend.submit(words[0]&!7,words[1]&!3,memory)?;}
                2=>for bank in 0..2 {
                    let(start,value,end)=(words[bank*3],words[bank*3+1],words[bank*3+2]);
                    if start==0||end<=start{continue;}
                    let size=(end-start)as usize;if size>16<<20{return Err("GPU fill exceeds limit".into());}
                    let control=words[6]>>(bank*16);let width=if control&(1<<9)!=0{4}else if control&(1<<8)!=0{3}else{2};
                    let pattern=value.to_le_bytes();let bytes:Vec<_>=(0..size).map(|index|pattern[index%width]).collect();
                    if !memory.write_gpu(start,&bytes){return Err("GPU fill backing unavailable".into());}
                    self.frontend.memory_fill(start,end,value,width as u32);
                },
                3=>{if !self.frontend.display_transfer(words[0],words[1],words[2],words[3],words[4],memory){self.cpu_transfer(memory,words)?;}}
                4=>self.texture_copy(memory,words)?,
                // Resource uploads resolve fresh snapshots for each native draw.
                5=>{},
                command=>return Err(format!("unknown native GX command {command}")),
            },
            3 if words.len()==6&&argument<2=>self.framebuffers[argument as usize].copy_from_slice(words),
            4=>self.force_black=argument!=0,
            _=>return Err(format!("invalid native GPU operation {operation}")),
        }
        Ok(())
    }
    fn texture_copy(&mut self,memory:&mut AddressSpace,words:&[u32])->Result<(),String>{
        let size=words[2];if size>16<<20{return Err("texture copy exceeds limit".into());}
        let (input_width,input_gap)=((words[3]&65535)*16,(words[3]>>16)*16);
        let (output_width,output_gap)=((words[4]&65535)*16,(words[4]>>16)*16);
        let mut source=words[0];let mut destination=words[1];let mut remaining=size;
        while remaining!=0{
            let chunk=if input_width==0||output_width==0{remaining}else{remaining.min(input_width.min(output_width))};
            let mut bytes=vec![0;chunk as usize];if !memory.read_gpu(source,&mut bytes)||!memory.write_gpu(destination,&bytes){return Err("texture copy backing unavailable".into());}
            remaining-=chunk;if remaining==0{break;}
            source=source.checked_add(input_width).and_then(|v|v.checked_add(input_gap)).ok_or("texture input overflow")?;
            destination=destination.checked_add(output_width).and_then(|v|v.checked_add(output_gap)).ok_or("texture output overflow")?;
        }
        Ok(())
    }
    /// Native memory-to-memory transfer for surfaces not owned by the renderer.
    /// PICA Morton layout/flag semantics match the CTR transfer engine; no
    /// framebuffer rendered by the reference backend participates here.
    fn cpu_transfer(&mut self,memory:&mut AddressSpace,words:&[u32])->Result<(),String>{
        let flags=words[4];let format=(flags>>8)&7;
        let out_format=(flags>>12)&7;
        if format>4||out_format>4||flags&(3<<24)!=0{return Err(format!("native CPU transfer conversion unsupported flags={flags:08x}"));}
        let bpp=match format{0=>4,1=>3,_=>2};let obpp=match out_format{0=>4,1=>3,_=>2};
        let(w,h)=(words[2]&65535,words[2]>>16);let(ow,oh)=(words[3]&65535,words[3]>>16);
        let length=|x:u32,y:u32|x.checked_mul(y).and_then(|v|v.checked_mul(bpp)).filter(|&v|v<=16<<20).map(|v|v as usize).ok_or("native transfer dimensions invalid");
        let mut input=vec![0;length(w,h)?];let out_len=ow.checked_mul(oh).and_then(|v|v.checked_mul(obpp)).filter(|&v|v<=16<<20).ok_or("native output dimensions invalid")?;let mut output=vec![0;out_len as usize];
        if !memory.read_gpu(words[0],&mut input)||!memory.read_gpu(words[1],&mut output){return Err("native transfer backing unavailable".into());}
        let linear=flags&2!=0;let tiled=linear!=(flags&32!=0);
        let offset=|x:u32,y:u32,width:u32,tiled:bool,bpp:u32|->usize{
            let pixel=if tiled{let m=(x&1)|((y&1)<<1)|((x&2)<<1)|((y&2)<<2)|((x&4)<<2)|((y&4)<<3);((y/8)*(width/8)+x/8)*64+m}else{y*width+x};(pixel*bpp)as usize
        };
        for y in 0..h.min(oh){for x in 0..w.min(ow){
            let sy=if flags&1!=0{h-1-y}else{y};let src=offset(x,sy,w,!linear,bpp);let dst=offset(x,y,ow,tiled,obpp);
            let value=input.get(src..src+bpp as usize).ok_or("native transfer source layout invalid")?;
            let target=output.get_mut(dst..dst+obpp as usize).ok_or("native transfer destination layout invalid")?;
            if format==out_format{target.copy_from_slice(value);}else{
                let v=if bpp==2{u16::from_le_bytes([value[0],value[1]])as u32}else{0};
                let expand=|v:u32,bits:u32|((v*255)/((1<<bits)-1))as u8;
                let rgba=match format{0=>[value[3],value[2],value[1],value[0]],1=>[value[2],value[1],value[0],255],2=>[expand(v>>11,5),expand((v>>5)&63,6),expand(v&31,5),255],3=>[expand(v>>11,5),expand((v>>6)&31,5),expand((v>>1)&31,5),if v&1!=0{255}else{0}],_=>[expand(v>>12,4),expand((v>>8)&15,4),expand((v>>4)&15,4),expand(v&15,4)]};
                match out_format{0=>target.copy_from_slice(&[rgba[3],rgba[2],rgba[1],rgba[0]]),1=>target.copy_from_slice(&[rgba[2],rgba[1],rgba[0]]),_=>{
                    let [r,g,b,a]=rgba.map(u16::from);let v=match out_format{2=>(r>>3)<<11|(g>>2)<<5|(b>>3),3=>(r>>3)<<11|(g>>3)<<6|(b>>3)<<1|u16::from(a>=128),_=>(r>>4)<<12|(g>>4)<<8|(b>>4)<<4|(a>>4)};target.copy_from_slice(&v.to_le_bytes());
                }}
            }
        }}
        if !memory.write_gpu(words[1],&output){return Err("native transfer destination unwritable".into());}Ok(())
    }
}
pub struct GpuHost<'a>{pub memory:&'a mut AddressSpace,pub gpu:&'a mut NativeGpu}
impl ServiceMemory for GpuHost<'_>{
    fn check_span(&self,address:u32,size:usize,writable:bool)->bool{self.memory.check_span(address,size,writable)}
    fn read_span(&mut self,address:u32,bytes:&mut[u8])->bool{AddressSpace::read_bytes(self.memory,address,bytes)}
    fn write_span(&mut self,address:u32,bytes:&[u8])->bool{self.memory.write_bytes(address,bytes)}
}
impl GspHost for GpuHost<'_>{
    fn gpu_action(&mut self,operation:u32,argument:u32,words:&[u32])->bool{
        match self.gpu.action(self.memory,operation,argument,words){Ok(())=>true,Err(error)=>{self.gpu.rejected+=1;log::error!("NATIVE_GPU_REJECT operation={operation} argument={argument:08x} words={words:08x?} reason={error}");self.gpu.last_error=Some(error);false}}
    }
}
