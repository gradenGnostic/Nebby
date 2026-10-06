//! Read-only real save IPC probe through native memory/kernel/FS ownership.
use pokemon3ds_runtime::{memory::{owner::{AddressSpaceOwner,Mapping},address_space::AddressSpace},ipc::{Target,message::{CommandBuffer,Header,Descriptor}},kernel::KObject,scheduler::{NativeScheduler,ThreadProfile},execution::{context::ThreadContext,arm::Cpu},services::fs_ipc::{self,Context,NativeFsState}};
fn main(){
    let args:Vec<_>=std::env::args().collect();assert_eq!(args.len(),3,"fs_ipc_probe <game input> <native CTR library>");
    let program=0x0004000000175e00;
    let mut memory=AddressSpace::new(AddressSpaceOwner::open(&args[2],0x8000000,0x4000000).unwrap());
    let physical=memory.allocate(0,8192).unwrap();
    memory.map(Mapping{base:0x10000000,physical,size:8192,permission:3,state:5}).unwrap();
    let mut kernel=NativeScheduler::<ThreadContext,u32>::new(program,0,0x14000000,ThreadProfile{tls_base:0x10000000,tls_stride:512,heap_base:0x8000000});
    let thread=kernel.create_thread("native FS probe",0,0x10002000,0,48,0);kernel.schedule(&mut Cpu::new(),0);
    let mut state=NativeFsState::default();let b=CommandBuffer::new(0x10000000);
    memory.write_bytes(0x10001000,b"main\0");
    let header=Header::new(0x803,8,4);
    for(index,value)in [(0,header.0),(2,4),(3,1),(4,0),(5,3),(6,5),(7,1),(9,Descriptor::static_buffer(0,0)),(10,0),(11,Descriptor::static_buffer(5,1)),(12,0x10001000)]{b.set(&mut memory,index,value);}
    let mut context=Context{memory:&mut memory,kernel:&mut kernel,fs:&mut state,image_path:std::path::Path::new(&args[1]),romfs_range:None,tick:0};
    fs_ipc::handle(&mut context,&b,header,&Target::Service{name:"fs:USER".into(),subhandle:0});
    assert_eq!(b.get(context.memory,1),0,"native save open result");
    let handle=b.get(context.memory,3);let object=context.kernel.handles.resolve(handle).unwrap();
    let Some(KObject::ClientSession(session))=context.kernel.objects.get(object)else{panic!("native FS session object absent");};
    let target=Target::Service{name:session.service.clone(),subhandle:session.subhandle};
    let header=Header::new(0x802,3,2);
    for(index,value)in [(0,header.0),(1,0),(2,0),(3,32),(4,(32<<4)|12),(5,0x10001100)]{b.set(context.memory,index,value);}
    fs_ipc::handle(&mut context,&b,header,&target);assert_eq!(b.get(context.memory,1),0);assert_eq!(b.get(context.memory,2),32);
    assert_eq!(context.kernel.thread(thread).status,pokemon3ds_runtime::scheduler::ThreadStatus::Sleeping);
    assert!(context.kernel.thread(thread).wakeup_at.is_some_and(|tick|tick>context.tick));
    let mut bytes=[0;32];assert!(AddressSpace::read_bytes(context.memory,0x10001100,&mut bytes));
    let root=std::env::var("POKEMOON_NATIVE_FS_ROOT").unwrap();
    let expected=std::fs::read(std::path::Path::new(&root).join("savedata/0004000000175E00/main")).unwrap();assert_eq!(bytes,&expected[..32]);
    let header=Header::new(0x808,0,0);b.set(context.memory,0,header.0);
    fs_ipc::handle(&mut context,&b,header,&target);assert_eq!(b.get(context.memory,1),0);
    println!("NATIVE_FS_IPC_PROBE save_open=ok typed_session=ok checked_read=ok native_wait=ok close=ok writes=0 zakuro_dependencies=0");
}
