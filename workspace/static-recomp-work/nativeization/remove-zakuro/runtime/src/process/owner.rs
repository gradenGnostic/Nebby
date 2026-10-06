//! Native process composition. Services and platform adapters borrow this
//! owner; no emulator application object owns its address space or threads.
use super::{image::{NativeProcessImage,ImageError},loader::LoaderState,svc::ThreadMemory};
use crate::{memory::{address_space::AddressSpace,owner::AddressSpaceOwner},
    execution::{arm::Cpu,context::ThreadContext},scheduler::{NativeScheduler,ThreadProfile}};
use std::path::Path;

/// CTR layout is platform metadata, supplied by the title bootstrap.
pub struct ProcessLayout {
    pub physical_bytes:u32,
    pub linear_base:u32,
    pub threads:ThreadProfile,
}
pub struct NativeProcess {
    pub image:NativeProcessImage,
    pub memory:AddressSpace,
    pub kernel:NativeScheduler<ThreadContext,u32>,
    pub cpu:Cpu,
    pub loader:LoaderState,
    pub exited:bool,
    pub services:Option<ProcessServices>,
    pub filesystem:crate::services::fs_ipc::NativeFsState,
    pub recompiled:Option<crate::execution::aot::Library>,
    pub interpreter_fallback_entries:u64,
    pub gpu:Option<crate::platform::gpu::NativeGpu>,
}
pub struct ProcessServices {
    pub directory:crate::services::directory::ServiceNamespace,
    pub objects:crate::services::objects::ServiceObjects,
    pub hid:crate::services::hid::HidSession,
    pub dsp:crate::services::dsp::DspSession,
    pub gsp:crate::services::gsp::GspSession,
}
impl NativeProcess {
    pub fn create(image:NativeProcessImage,library:&Path,layout:ProcessLayout)->Result<Self,ImageError>{
        let region=image.memory_region.checked_sub(1).filter(|&r|r<=2)
            .ok_or_else(||ImageError("invalid native process memory region".into()))?;
        let owner=AddressSpaceOwner::open(library.to_str().ok_or_else(||ImageError("non-UTF8 native memory library path".into()))?,layout.physical_bytes,image.app_bytes)
            .map_err(ImageError)?;
        let mut memory=AddressSpace::new(owner);
        memory.set_image_region(region).map_err(ImageError)?;
        image.install(&mut memory)?;
        let mut kernel=NativeScheduler::new(image.title.program_id(),region,layout.linear_base,layout.threads);
        let main=kernel.create_thread("main",image.entry,image.profile.stack_top,0,image.priority,image.processor);
        memory.map_tls(kernel.thread(main).tls).map_err(|code|ImageError(format!("native primary TLS allocation: {code:08x}")))?;
        kernel.reschedule_pending=true;
        let mut cpu=Cpu::new();
        if !kernel.schedule(&mut cpu,0){return Err(ImageError("native primary thread not runnable".into()));}
        Ok(Self{image,memory,kernel,cpu,loader:LoaderState::default(),exited:false,services:None,filesystem:Default::default(),recompiled:None,interpreter_fallback_entries:0,gpu:None})
    }
    pub fn attach_linked(&mut self,linked:&crate::execution::aot::Linked)->Result<(),String>{
        if linked.program_id()!=self.kernel.program_id{return Err("linked AOT title differs from native process image".into());}
        self.recompiled=Some(crate::execution::aot::Library::linked(linked)?);Ok(())
    }
    /// Execute native code through the existing ABI-4 page tables. Missing
    /// entries use only the independent portable ARM implementation, counted.
    pub fn execute_slice(&mut self,budget:u64)->crate::execution::aot::Stop{
        use crate::execution::{aot::Stop,arm::Exit};
        if let Some(library)=self.recompiled.as_ref(){
            if library.has_code(self.cpu.regs[15]|self.cpu.cpsr.thumb as u32){
                let (ran,stop)=library.run(&mut self.cpu,&mut self.memory,budget);
                if ran!=0||!matches!(stop,Stop::Left){return stop;}
            }
        }
        self.interpreter_fallback_entries+=1;
        // Recheck AOT ownership after each missed instruction. Running a full
        // quantum here would interpret code that has a generated native entry.
        match self.cpu.step(&mut self.memory){Some(Exit::Supervisor(command))=>Stop::Svc(command),None|Some(Exit::Timeout)=>Stop::Left,Some(exit)=>Stop::Exit(exit)}
    }
    /// Execute a real typed session request against native process-owned objects.
    /// Returned placements update the caller's linked AOT module table.
    pub fn send_sync_request(&mut self)->Result<Vec<super::loader::ModulePlacement>,String>{
        use crate::{ipc::message::{CommandBuffer,Header},services::{service_kind,ServiceKind}};
        let Some(target)=super::svc::session_target(&mut self.kernel,&mut self.cpu.regs)else{return Ok(Vec::new());};
        let tls=self.kernel.current().ok_or("native IPC requires a running thread")?.tls;
        let buffer=CommandBuffer::new(tls);
        let mut words=[0;64];
        for(index,word)in words.iter_mut().enumerate(){*word=buffer.get(&mut self.memory,index as u32);}
        let mut placements=Vec::new();
        match service_kind(target.name()).ok_or_else(||format!("unregistered native IPC service {}",target.name()))?{
            ServiceKind::Gsp=>{
                let gsp=&mut self.services.as_mut().ok_or("native GSP session not attached")?.gsp;
                gsp.prepare(&words,&mut self.memory,&mut self.kernel)?;
                let mut host=crate::platform::gpu::GpuHost{memory:&mut self.memory,gpu:self.gpu.as_mut().ok_or("native GPU frontend not attached")?};
                gsp.dispatch(&mut words,&mut host,&mut self.kernel)?;
            }
            ServiceKind::Dsp=>{
                self.services.as_mut().ok_or("native DSP session not attached")?.dsp.dispatch(&mut words,tls,&mut self.memory,&mut self.kernel)?;
            }
            ServiceKind::Hid=>{
                self.services.as_mut().ok_or("native HID session not attached")?.hid.dispatch(&mut words,&mut self.memory,&mut self.kernel)?;
            }
            ServiceKind::Filesystem=>{
                let mut context=crate::services::fs_ipc::Context{memory:&mut self.memory,kernel:&mut self.kernel,fs:&mut self.filesystem,
                    image_path:&self.image.path,romfs_range:self.image.title.romfs_level3_range(),tick:self.cpu.cycles};
                let header=Header(words[0]);
                crate::services::fs_ipc::handle(&mut context,&buffer,header,&target);
                return Ok(Vec::new());
            }
            ServiceKind::Directory|ServiceKind::BackgroundStorage=>{
                self.services.as_mut().ok_or("native service namespace not attached")?.directory
                    .dispatch(&mut words,&mut self.kernel,service_kind(target.name())==Some(ServiceKind::BackgroundStorage))?;
            }
            ServiceKind::Loader=>{
                placements=self.loader.loader.dispatch(&mut words,&mut self.memory,self.kernel.program_id)
                    .map_err(|error|format!("native loader request rejected: {error:?}"))?;
                if let Some(library)=self.recompiled.as_mut(){for module in &placements{library.place(&module.name,module.base);}}
            }
            kind@ (ServiceKind::Config|ServiceKind::Ndm|ServiceKind::Apt|ServiceKind::Y2r|ServiceKind::Friends|ServiceKind::Ssl|ServiceKind::Uds)=>{
                let service=match kind{ServiceKind::Config=>0,ServiceKind::Ndm=>1,ServiceKind::Apt=>2,ServiceKind::Y2r=>3,ServiceKind::Friends=>4,ServiceKind::Ssl=>5,ServiceKind::Uds=>6,_=>unreachable!()};
                self.services.as_mut().ok_or("native service objects not attached")?.objects
                    .dispatch(service,&mut words,&mut self.memory,tls.checked_add(0x180).ok_or("native static buffer table overflow")?,&mut self.kernel)?;
            }
            kind=>return Err(format!("native process service hook not connected: {kind:?}")),
        }
        let header=Header(words[0]);let count=1+header.normal_params()+header.translate_params();
        if count>64{return Err("native IPC reply overflow".into());}
        for index in 0..count{buffer.set(&mut self.memory,index,words[index as usize]);}
        Ok(placements)
    }
    /// Dispatch native kernel/register operations. False is an explicit missing
    /// process integration operation, never an emulator fallback.
    pub fn dispatch_kernel(&mut self,number:u32,tick:u64)->bool {
        use super::svc;
        match number {
            0x3a=>{
                let output=self.cpu.regs[0];let input=self.cpu.regs[2];let count=self.cpu.regs[3];
                if count>10||input%4!=0||output%4!=0{self.cpu.regs[0]=0xe0e01bfd;return true;}
                let mut bytes=vec![0;count as usize*4];
                if !AddressSpace::read_bytes(&self.memory,input,&mut bytes){self.cpu.regs[0]=0xe0e01bf5;return true;}
                let names:Vec<u32>=bytes.chunks_exact(4).map(|b|u32::from_le_bytes(b.try_into().unwrap())).collect();
                let committed=self.memory.region_used(self.kernel.memory_region)as u64;
                match svc::resource_current_values(&mut self.kernel,&mut self.cpu.regs,&names,committed){
                    Ok(values)=>{let bytes:Vec<u8>=values.into_iter().flat_map(u64::to_le_bytes).collect();self.cpu.regs[0]=if AddressSpace::write_bytes(&mut self.memory,output,&bytes){0}else{0xe0e01bf5};}
                    Err(result)=>self.cpu.regs[0]=result,
                }
            }
            0x01=>svc::control_memory(&mut self.kernel,&mut self.cpu.regs,&mut self.memory),
            0x02=>svc::query_memory(&mut self.cpu.regs,&self.memory),
            0x08=>svc::create_thread(&mut self.kernel,&mut self.cpu.regs,&mut self.memory),
            0x1e=>svc::create_memory_block(&mut self.kernel,&mut self.cpu.regs,&mut self.memory),
            0x1f=>{svc::map_memory_block(&mut self.kernel,&mut self.cpu.regs,&mut self.memory);}
            0x22=>svc::arbitrate(&mut self.kernel,&mut self.cpu.regs,tick,&mut self.memory),
            0x24|0x25=>{svc::wait_registers(&mut self.kernel,&mut self.cpu.regs,tick,&mut self.memory,number);}
            0x2b=>{let committed=self.memory.region_used(self.kernel.memory_region)as u64;svc::process_info(&mut self.kernel,&mut self.cpu.regs,committed);}
            0x2d=>{
                let pointer=self.cpu.regs[1];let mut name=Vec::new();let mut terminated=false;
                for offset in 0..12{
                    let Some(address)=pointer.checked_add(offset)else{break;};let mut byte=[0];
                    if !AddressSpace::read_bytes(&self.memory,address,&mut byte){break;}
                    if byte[0]==0{terminated=true;break;}name.push(byte[0]);
                }
                if !terminated{self.cpu.regs[0]=0xe0e01bf5;}else{match std::str::from_utf8(&name){Ok(name)=>svc::connect_to_port(&mut self.kernel,&mut self.cpu.regs,name),Err(_)=>self.cpu.regs[0]=0xe0e01bf5}}
            }
            _=>return svc::dispatch(&mut self.kernel,&mut self.cpu.regs,tick,&mut self.exited,number),
        }
        true
    }
}
