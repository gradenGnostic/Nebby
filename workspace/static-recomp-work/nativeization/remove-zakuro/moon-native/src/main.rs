// Native title composition, independent of any Zakuro application/runtime crate.
use pokemon3ds_runtime::{process::{image::{NativeProcessImage,TitleProfile},owner::{NativeProcess,ProcessLayout,ProcessServices},pages::KernelPages},execution::aot::{Linked,Stop},scheduler::ThreadProfile,services::{directory::ServiceNamespace,objects::ServiceObjects,router::ServiceRouter,hid::HidSession,dsp::DspSession,gsp::GspSession}};
use std::{ffi::c_void,ptr::addr_of,path::Path};
unsafe extern "C"{static recomp_abi:u32;static recomp_entry_count:u32;static recomp_entries:u8;static recomp_module_count:u32;static recomp_modules:u8;}
fn console_identity(root:&Path)->std::io::Result<u64>{
    use std::io::{Read,Write};
    std::fs::create_dir_all(root)?;let path=root.join(".ctr-console-identity");
    let bytes=match std::fs::read(&path){
        Ok(bytes)=>bytes,
        Err(error)if error.kind()==std::io::ErrorKind::NotFound=>{
            let mut random=[0;8];std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
            let bytes=(u64::from_le_bytes(random)&0xffff0003ffffffff).to_le_bytes();
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path){
                Ok(mut file)=>{file.write_all(&bytes)?;file.sync_all()?;bytes.to_vec()},
                Err(error)if error.kind()==std::io::ErrorKind::AlreadyExists=>std::fs::read(&path)?,
                Err(error)=>return Err(error),
            }
        }
        Err(error)=>return Err(error),
    };
    Ok(u64::from_le_bytes(bytes.try_into().map_err(|_|std::io::Error::new(std::io::ErrorKind::InvalidData,"console identity must contain eight bytes"))?))
}
fn main(){
    if std::env::args().nth(1).as_deref()==Some("--capabilities") {
        println!("modern_camera={}",cfg!(nebby_modern_camera));return;
    }
    env_logger::init();
    let args:Vec<_>=std::env::args().collect();assert_eq!(args.len(),4,"pokemoon-native <game> <profile> <native CTR library>");
    let image=NativeProcessImage::open(Path::new(&args[1]),TitleProfile::load(Path::new(&args[2])).unwrap()).unwrap();
    let id=image.title.program_id();
    let mut process=Box::new(NativeProcess::create(image,Path::new(&args[3]),ProcessLayout{physical_bytes:0x08000000,linear_base:0x14000000,threads:ThreadProfile{tls_base:0x1ff82000,tls_stride:0x200,heap_base:0x08000000}}).unwrap());
    let clock=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()as u64+2_208_988_800_000;
    let pages=KernelPages{boot_clock_ms:clock,new3ds:false,slider:0.,application_bytes:process.image.app_bytes,system_bytes:process.memory.region_size(1),base_bytes:process.memory.region_size(2),memory_type:process.image.system_mode as u32,mac:[0x40,0xf4,7,0,0,1]};pages.install(&mut process.memory).unwrap();
    let linked=unsafe{Linked::new(id,recomp_abi,addr_of!(recomp_entries)as *const c_void,recomp_entry_count,addr_of!(recomp_modules)as *const c_void,recomp_module_count)};
    process.attach_linked(&linked).unwrap();
    let save_root=std::path::PathBuf::from(std::env::var_os("POKEMOON_NATIVE_FS_ROOT").expect("native save profile required"));
    let identity=console_identity(&save_root).expect("native persistent console identity");
    process.services=Some(ProcessServices{directory:ServiceNamespace::new(["cfg:u","cfg:s","cfg:i","APT:A","APT:U","APT:S","ndm:u","fs:USER","hid:USER","hid:SPVR","gsp::Gpu","dsp::DSP","y2r:u","frd:u","ssl:C","nwm::UDS","ldr:ro","boss:U"]),objects:ServiceObjects::new(ServiceRouter::new(Path::new(&args[3]),2,1,0,identity).unwrap()),hid:HidSession::open(&args[3]).unwrap(),dsp:DspSession::open(&args[3]).unwrap(),gsp:GspSession::open(&args[3]).unwrap()});
    pokemon3ds_runtime::platform::input::configure();
    let frontend=std::env::var("POKEMOON_TRIAEVUM_FRONTEND_LIB").expect("native PICA frontend library required");
    process.gpu=Some(pokemon3ds_runtime::platform::gpu::NativeGpu::new(pokemon3ds_runtime::platform::pica::PicaFrontend::open(Path::new(&frontend)).unwrap()));
    let window=pokemon3ds_runtime::platform::HostWindow::open(Path::new(&frontend)).unwrap();
    let audio=pokemon3ds_runtime::platform::audio::Audio::open(32728.498).unwrap();
    let started=std::time::Instant::now();
    let mut gamepads=pokemon3ds_runtime::platform::gamepad::Gamepads::new();
    let mut timing=pokemon3ds_runtime::execution::pump::ProcessTiming::new(pokemon3ds_runtime::scheduler::CPU_CLOCK_HZ/60,160*8192,8192);
    eprintln!("MOON_NATIVE_BOOTSTRAP zakuro_runtime_libraries=0 {}",process.recompiled.as_ref().unwrap().describe());
    loop{
        let tick=process.cpu.cycles;
        if tick>=timing.frame{
            let p=&mut *process;let services=p.services.as_mut().unwrap();
            let mut input=window.input();gamepads.sample(&mut input);
            #[cfg(nebby_modern_camera)]
            {
                unsafe extern "C" {
                    fn modern_camera_enabled()->u32;
                    fn modern_camera_host_stick(x:f32,y:f32,recenter:u32,seconds:f64);
                    fn modern_camera_capture_requested()->u32;
                    fn modern_camera_host_mouse(x:f32,y:f32,captured:u32);
                    fn modern_camera_host_recenter(held:u32);
                }
                if unsafe { modern_camera_enabled()!=0 } {
                let (x,y,recenter)=gamepads.camera_input();
                unsafe { modern_camera_host_stick(x,y,u32::from(recenter),tick as f64/pokemon3ds_runtime::scheduler::CPU_CLOCK_HZ as f64); }
                let (x,y,captured)=window.relative_mouse(unsafe{modern_camera_capture_requested()!=0});
                unsafe { modern_camera_host_mouse(x,y,u32::from(captured)); }
                unsafe { modern_camera_host_recenter(u32::from(window.camera_recenter())); }
                }
            }
            pokemon3ds_runtime::platform::input::update(pokemon3ds_runtime::platform::input::PcInputState::new(input.buttons,input.circle_x,input.circle_y).with_press_edges(input.pressed_buttons,input.pressed_circle));
            services.hid.sample(pokemon3ds_runtime::services::hid::HidInput{buttons:input.buttons,circle_x:input.circle_x,circle_y:input.circle_y,touch:(input.touch_down!=0).then_some((input.touch_x.clamp(0,319)as u16,input.touch_y.clamp(0,239)as u16))},tick,&mut p.memory,&mut p.kernel).unwrap();
            services.gsp.vblank(&mut pokemon3ds_runtime::platform::gpu::GpuHost{memory:&mut p.memory,gpu:p.gpu.as_mut().unwrap()},&mut p.kernel).unwrap();
            pages.update_clock(&mut p.memory,tick).unwrap();
            timing.frame_done(tick);
            if !window.running(){eprintln!("MOON_NATIVE_RUNTIME interpreter_fallback_entries={} aot_fallback_instructions={} native_svc_calls={} missing_svc_calls={}",process.interpreter_fallback_entries,process.recompiled.as_ref().unwrap().fallbacks(),pokemon3ds_runtime::process::svc::counters().0,pokemon3ds_runtime::process::svc::counters().1);return;}
            let elapsed=std::time::Duration::from_secs_f64(tick as f64/pokemon3ds_runtime::scheduler::CPU_CLOCK_HZ as f64);
            let delay=elapsed.saturating_sub(started.elapsed());if !delay.is_zero(){std::thread::sleep(delay);}
        }
        while timing.audio_due(tick){let p=&mut *process;let pcm=p.services.as_mut().unwrap().dsp.frame(&mut p.memory,&mut p.kernel).unwrap();audio.push(&pcm);}
        if timing.preempt_due(tick){process.kernel.reschedule_pending=true;}
        let audio_running=process.services.as_ref().unwrap().dsp.running;
        if !pokemon3ds_runtime::execution::pump::prepare(&mut process.cpu,&mut process.kernel,&timing,None,audio_running){continue;}
        let budget=timing.execution_limit(None).saturating_sub(process.cpu.cycles).max(1);
        match process.execute_slice(budget){
            Stop::Svc(0x32)=>{process.send_sync_request().unwrap();}
            Stop::Svc(number)=>{let tick=process.cpu.cycles;if !process.dispatch_kernel(number,tick){panic!("native process syscall hook missing: svc={number:02x} pc={:08x} registers={:08x?}",process.cpu.regs[15],&process.cpu.regs[..5]);}}
            Stop::Left=>{},Stop::Exit(exit)=>panic!("native process exit: {exit:?}"),
        }
        if process.exited{return;}
    }
}
