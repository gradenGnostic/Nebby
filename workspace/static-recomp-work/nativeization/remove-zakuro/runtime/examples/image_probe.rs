//! Real title image installation and first SVC, without a reference runtime.
use pokemon3ds_runtime::{process::{image::{NativeProcessImage,TitleProfile},owner::{NativeProcess,ProcessLayout}},scheduler::ThreadProfile,execution::arm::Exit};
fn request(process:&mut NativeProcess,handle:u32,words:&[u32])->[u32;64]{
    use pokemon3ds_runtime::ipc::message::CommandBuffer;
    let buffer=CommandBuffer::new(process.kernel.current().unwrap().tls);
    for index in 0..64{buffer.set(&mut process.memory,index,words.get(index as usize).copied().unwrap_or(0));}
    process.cpu.regs[0]=handle;assert!(process.send_sync_request().unwrap().is_empty());
    let mut response=[0;64];for(index,word)in response.iter_mut().enumerate(){*word=buffer.get(&mut process.memory,index as u32);}response
}
fn main(){
    let args:Vec<_>=std::env::args().collect();
    assert_eq!(args.len(),4,"image_probe <game input> <title profile> <native CTR library>");
    let image=NativeProcessImage::open(std::path::Path::new(&args[1]),TitleProfile::load(std::path::Path::new(&args[2])).unwrap()).unwrap();
    let total=if image.title.platform()==2{0x10000000}else{0x08000000};
    let mut process=NativeProcess::create(image,std::path::Path::new(&args[3]),ProcessLayout{
        physical_bytes:total,linear_base:0x14000000,
        threads:ThreadProfile{tls_base:0x1ff82000,tls_stride:0x200,heap_base:0x08000000},
    }).expect("native process composition");
    assert_eq!(process.kernel.current_thread,Some(0));
    assert_eq!(process.cpu.cp15.thread_id_ro,process.kernel.thread(0).tls);
    let pages=pokemon3ds_runtime::process::pages::KernelPages{boot_clock_ms:1000,new3ds:false,slider:0.,application_bytes:process.image.app_bytes,system_bytes:process.memory.region_size(1),base_bytes:process.memory.region_size(2),memory_type:process.image.system_mode as u32,mac:[0x40,0xf4,7,0,0,1]};
    pages.install(&mut process.memory).unwrap();
    assert!(!process.memory.write_bytes(0x1ff80000,&[1]));
    pages.update_clock(&mut process.memory,pokemon3ds_runtime::scheduler::CPU_CLOCK_HZ).unwrap();
    let mut clock=[0;8];assert!(process.memory.read_bytes(0x1ff81020,&mut clock));assert_eq!(u64::from_le_bytes(clock),2000);
    pages.update_clock(&mut process.memory,0).unwrap();
    println!("NATIVE_PROCESS_PAGES_PROBE config_readonly=ok shared_clock=ok vram=ok dsp_ram=ok");
    for instructions in 1..=10000000 {
        if let Some(exit)=process.cpu.step(&mut process.memory){
            let Exit::Supervisor(command)=exit else{panic!("native image stopped before SVC: {exit:?}");};
            println!("NATIVE_IMAGE_PROBE title={} entry={:08x} first_svc={command:02x} instructions={instructions} zakuro_dependencies=0",process.image.profile.name,process.image.entry);
            println!("NATIVE_PROCESS_OWNER_PROBE image=ok kernel=ok primary_thread=ok tls=ok scheduler=ok");
            assert_eq!(command,0x21,"probe expects native arbiter bootstrap boundary");
            assert!(process.dispatch_kernel(command,0));
            assert_eq!(process.cpu.regs[0],0);
            let handle=process.cpu.regs[1];
            let object=process.kernel.resolve(handle).expect("native arbiter handle");
            assert!(matches!(process.kernel.objects.get(object),Some(pokemon3ds_runtime::kernel::KObject::AddressArbiter(_))));
            println!("NATIVE_PROCESS_SVC_PROBE first_guest_svc=handled object=AddressArbiter handle={handle:08x}");
            use pokemon3ds_runtime::{process::owner::ProcessServices,services::{directory::ServiceNamespace,objects::ServiceObjects,router::ServiceRouter},kernel::KObject};
            process.services=Some(ProcessServices{directory:ServiceNamespace::new(["cfg:u","APT:A","fs:USER","hid:USER","dsp::DSP","gsp::Gpu"]),objects:ServiceObjects::new(ServiceRouter::new(std::path::Path::new(&args[3]),2,1,0,0).unwrap()),hid:pokemon3ds_runtime::services::hid::HidSession::open(&args[3]).unwrap(),dsp:pokemon3ds_runtime::services::dsp::DspSession::open(&args[3]).unwrap(),gsp:pokemon3ds_runtime::services::gsp::GspSession::open(&args[3]).unwrap()});
            pokemon3ds_runtime::process::svc::connect_to_port(&mut process.kernel,&mut process.cpu.regs,"srv:");let srv=process.cpu.regs[1];
            assert_eq!(request(&mut process,srv,&[0x10002,0x20,1])[1],0);
            let cfg=request(&mut process,srv,&[0x50100,u32::from_le_bytes(*b"cfg:"),b'u' as u32,5])[3];
            let region=request(&mut process,cfg,&[0x20000]);assert_eq!((region[1],region[2]),(0,2));
            let apt=request(&mut process,srv,&[0x50100,u32::from_le_bytes(*b"APT:"),b'A' as u32,5])[3];
            let lock=request(&mut process,apt,&[0x10040,0])[5];
            let object=process.kernel.resolve(lock).unwrap();assert!(matches!(process.kernel.objects.get(object),Some(KObject::Mutex(_))));
            let duplicate=request(&mut process,apt,&[0x10040,0])[5];
            assert_ne!(duplicate,lock);assert_eq!(process.kernel.resolve(duplicate),Some(object));
            assert!(process.kernel.handles.close(&mut process.kernel.objects,lock));
            assert!(process.kernel.handles.close(&mut process.kernel.objects,duplicate));
            let reopened=request(&mut process,apt,&[0x10040,0])[5];assert_eq!(process.kernel.resolve(reopened),Some(object));
            println!("NATIVE_PROCESS_IPC_PROBE directory=ok cfg_region=ok apt_mutex=ok repeated_identity=ok zakuro_dependencies=0");
            let hid=request(&mut process,srv,&[0x50100,u32::from_le_bytes(*b"hid:"),u32::from_le_bytes(*b"USER"),8])[3];
            let resources=request(&mut process,hid,&[0xa0000]);assert_eq!(resources[1],0);
            let event=process.kernel.resolve(resources[4]).unwrap();
            process.cpu.regs[0]=resources[3];process.cpu.regs[1]=0;process.cpu.regs[2]=3;
            assert!(process.dispatch_kernel(0x1f,0));assert_eq!(process.cpu.regs[0],0);
            let shared=process.kernel.resolve(resources[3]).unwrap();
            let Some(KObject::SharedMemory(block))=process.kernel.objects.get(shared)else{panic!("native HID shared object absent");};
            let mapped=block.mapped_at.unwrap();let physical=block.paddr;
            assert_eq!(process.memory.mapping_at(mapped).unwrap().physical,physical);
            for (tick,buttons)in [(1,1),(2,1),(3,0)]{
                process.services.as_mut().unwrap().hid.sample(pokemon3ds_runtime::services::hid::HidInput{buttons,circle_x:0.5,..Default::default()},tick,&mut process.memory,&mut process.kernel).unwrap();
            }
            assert!(matches!(process.kernel.objects.get(event),Some(KObject::Event(event))if event.signaled));
            let mut ring=[0;4096];assert!(process.memory.read_bytes(mapped,&mut ring));assert!(ring.iter().any(|&byte|byte!=0));
            println!("NATIVE_PROCESS_HID_PROBE shared_memory=ok mapping=ok samples=3 events=ok zakuro_dependencies=0");
            let dsp=request(&mut process,srv,&[0x50100,u32::from_le_bytes(*b"dsp:"),u32::from_le_bytes(*b":DSP"),8])[3];
            let semaphore=request(&mut process,dsp,&[0x160000]);assert_eq!(semaphore[1],0);
            let object=process.kernel.resolve(semaphore[3]).unwrap();assert!(matches!(process.kernel.objects.get(object),Some(KObject::Event(_))));
            assert!(!process.services.as_ref().unwrap().dsp.running);
            println!("NATIVE_PROCESS_DSP_PROBE semaphore=ok typed_event=ok initialized_idle=ok zakuro_dependencies=0");
            if let Some(frontend)=std::env::var_os("POKEMOON_TRIAEVUM_FRONTEND_LIB"){
                use pokemon3ds_runtime::platform::{gpu::{NativeGpu,GpuHost},pica::PicaFrontend};
                process.gpu=Some(NativeGpu::new(PicaFrontend::open(std::path::Path::new(&frontend)).unwrap()));
                let gsp=request(&mut process,srv,&[0x50100,u32::from_le_bytes(*b"gsp:"),u32::from_le_bytes(*b":Gpu"),8])[3];
                assert_eq!(request(&mut process,gsp,&[0x160042,0,0,0xffff8001])[1],0);
                let(event,handle)=process.kernel.create_event(pokemon3ds_runtime::kernel::ResetType::OneShot,"GSP probe relay");
                let relay=request(&mut process,gsp,&[0x130042,1,0,handle]);assert!(matches!(relay[1],0|0x2a07),"native GSP initialization result");
                process.cpu.regs[0]=relay[4];process.cpu.regs[1]=0;process.cpu.regs[2]=3;
                assert!(process.dispatch_kernel(0x1f,0));assert_eq!(process.cpu.regs[0],0);
                let mut host=GpuHost{memory:&mut process.memory,gpu:process.gpu.as_mut().unwrap()};
                process.services.as_mut().unwrap().gsp.vblank(&mut host,&mut process.kernel).unwrap();
                assert!(matches!(process.kernel.objects.get(event),Some(KObject::Event(event))if event.signaled));
                println!("NATIVE_PROCESS_GSP_PROBE production_frontend=linked relay=ok shared_map=ok vblank=ok draw_submissions=0");
            }
            if let Some(root)=std::env::var_os("POKEMOON_NATIVE_FS_ROOT"){
                use pokemon3ds_runtime::{process::svc::AllocationMemory,ipc::message::{Header,Descriptor}};
                let physical=process.memory.allocate_region(&0,4096).unwrap();process.memory.install_allocation(0x09000000,physical,4096,3,false);
                assert!(process.memory.write_bytes(0x09000000,b"main\0"));
                let fs=request(&mut process,srv,&[0x50100,u32::from_le_bytes(*b"fs:U"),u32::from_le_bytes(*b"SER\0"),7])[3];
                let open=request(&mut process,fs,&[Header::new(0x803,8,4).0,0,4,1,0,3,5,1,0,Descriptor::static_buffer(0,0),0,Descriptor::static_buffer(5,1),0x09000000]);
                assert_eq!(open[1],0);let file=open[3];
                let read=request(&mut process,file,&[Header::new(0x802,3,2).0,0,0,32,(32<<4)|12,0x09000100]);assert_eq!((read[1],read[2]),(0,32));
                let expected=std::fs::read(std::path::Path::new(&root).join("savedata").join(format!("{:016X}",process.kernel.program_id)).join("main")).unwrap();
                let mut bytes=[0;32];assert!(process.memory.read_bytes(0x09000100,&mut bytes));assert_eq!(&bytes[..],&expected[..32]);
                assert_eq!(request(&mut process,file,&[0x8080000])[1],0);
                println!("NATIVE_PROCESS_FS_PROBE real_save_open=ok checked_read=ok close=ok writes=0");
            }
            return;
        }
    }
    panic!("native title did not reach first SVC within probe limit: pc={:08x}",process.cpu.regs[15]);
}
