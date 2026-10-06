//! Production native memory ownership probe, with no reference runtime link.
use pokemon3ds_runtime::memory::owner::{AddressSpaceOwner,Mapping};
use pokemon3ds_runtime::{memory::address_space::AddressSpace,GuestMemory,services::router::ServiceMemory,execution::{aot::AotMemory,arm::{Cpu,Exit}}};
fn main(){
    let path=std::env::args().nth(1).expect("native CTR library path");
    let mut memory=AddressSpaceOwner::open(&path,0x08000000,0x04000000).expect("native address space");
    let physical=memory.allocate(0,4096).expect("application allocation");
    assert_eq!(memory.region_used(0),4096);
    let mapping=Mapping{base:0x08000000,size:4096,physical,permission:3,state:5};
    assert!(memory.map(mapping));assert_eq!(memory.query(mapping.base),(true,mapping));
    let pointer=memory.pointer(physical,4096);assert!(!pointer.is_null());
    // Exclusive owner held, checked live backing; no aliases are accessed here.
    unsafe{*pointer=0x5a;assert_eq!(*pointer,0x5a);}
    assert!(memory.pointer(physical,0xffffffff).is_null());
    assert_eq!(memory.mappings(),vec![mapping]);
    assert!(memory.unmap(mapping.base,mapping.size));assert!(!memory.query(mapping.base).0);
    assert!(memory.free(physical,4096));assert_eq!(memory.region_used(0),0);
    let mut memory=AddressSpace::new(memory);
    use pokemon3ds_runtime::process::svc::{ThreadMemory,SharedBacking,SharedMapping,AllocationMemory};
    memory.map_tls(0x1ff82000).expect("native TLS page");
    assert_eq!(memory.region_used(2),4096);
    let mut tls=[1;4096];assert!(AddressSpace::read_bytes(&memory,0x1ff82000,&mut tls));assert!(tls.iter().all(|&byte|byte==0));
    assert!(memory.write_bytes(0x1ff82080,&[9]));
    memory.map_tls(0x1ff82200).unwrap();assert_eq!(memory.region_used(2),4096);
    assert_eq!(memory.shared_backing(0x1ff82000,4096).unwrap(),memory.mapping_at(0x1ff82000).unwrap().physical);
    assert_eq!(memory.shared_backing(0x1ff82001,4096),Err(0xe0e01bf1));
    assert_eq!(memory.shared_backing(0x1ff82000,8192),Err(0xe0e01bf5));
    let tls_physical=memory.shared_backing(0x1ff82000,4096).unwrap();
    let shared=memory.map_shared(0,tls_physical,4096,3).unwrap();assert_eq!(shared,0x10000000);
    let second=memory.map_shared(0,tls_physical,4096,1).unwrap();assert_eq!(second,0x10001000);
    let mut value=[0];assert!(AddressSpace::read_bytes(&memory,shared+128,&mut value));assert_eq!(value,[9]);
    assert!(!memory.write_bytes(second,&[1]));
    assert_eq!(memory.map_shared(1,tls_physical,4096,3),Err(0xe0e01bf1));
    memory.unmap(shared,4096).unwrap();memory.unmap(second,4096).unwrap();
    let allocated=memory.allocate_region(&0,4096).unwrap();
    memory.install_allocation(0x09000000,allocated,4096,3,false);
    assert_eq!(memory.query(0x09000000).state,5);
    assert!(memory.write_bytes(0x09000000,&[42]));memory.unmap(0x09000000,4096).unwrap();
    assert!(memory.write_gpu(allocated,&[7,8,9]));let mut gpu_bytes=[0;3];assert!(memory.read_gpu(allocated,&mut gpu_bytes));assert_eq!(gpu_bytes,[7,8,9]);
    assert!(!memory.read_gpu(0xffffffff,&mut gpu_bytes));assert!(!memory.write_gpu(0xffffffff,&[1,2,3]));
    println!("NATIVE_PROCESS_MEMORY_PROBE shared_map=ok shared_alias=ok read_only=ok allocation=ok");
    println!("NATIVE_THREAD_MEMORY_PROBE tls_zero=ok tls_reuse=ok shared_identity=ok invalid_span=ok");
    let physical=memory.allocate(0,8192).expect("native execution pages");
    let mapping=Mapping{base:0x08000000,size:8192,physical,permission:3,state:5};
    memory.map(mapping).unwrap();
    assert!(memory.write_bytes(mapping.base+4094,&[1,2,3,4]));
    let mut bytes=[0;4];assert!(AddressSpace::read_bytes(&memory,mapping.base+4094,&mut bytes));assert_eq!(bytes,[1,2,3,4]);
    assert!(!memory.check_span(0xffffffff,2,false));
    assert!(!memory.check_span(mapping.base+8191,2,true));
    memory.map(Mapping{permission:1,..mapping}).unwrap();
    assert!(!memory.write_bytes(mapping.base,&[1]));
    assert!(memory.write32_privileged(mapping.base,0xe3a0002a)); // ARM mov r0,#42
    assert!(memory.write32_privileged(mapping.base+4,0xef000001)); // ARM svc 1
    let (read,write)=memory.page_tables();
    unsafe{assert!(!(*read.add((mapping.base>>12)as usize)).is_null());assert!((*write.add((mapping.base>>12)as usize)).is_null());}
    let mut cpu=Cpu::new();cpu.regs[15]=mapping.base;
    assert_eq!(cpu.step(&mut memory),None);assert_eq!(cpu.regs[0],42);
    assert!(matches!(cpu.step(&mut memory),Some(Exit::Supervisor(1))));
    use pokemon3ds_runtime::process::loader::ProcessMemory;
    let block=memory.copy_module(0x01000000,mapping.base,8192).expect("native CRO copy backing");
    assert_eq!(memory.region_used(1),8192);
    assert!(memory.mirror_module(0x02000000,0x01000000,8192));
    assert!(AddressSpace::write_bytes(&mut memory,0x01000008,&[5,6,7,8]));
    let mut alias=[0;4];assert!(AddressSpace::read_bytes(&memory,0x02000008,&mut alias));assert_eq!(alias,[5,6,7,8]);
    memory.unmap(0x02000000,8192).unwrap();
    memory.trim_module(0x01000000,block,4096);assert_eq!(memory.region_used(1),4096);
    assert!(!memory.check_span(0x01001000,1,false));
    memory.release_module(0x01000000,pokemon3ds_runtime::PageAllocation{addr:block.addr,size:4096});assert_eq!(memory.region_used(1),0);
    memory.unmap(mapping.base,mapping.size).unwrap();
    assert!(!memory.check_span(mapping.base,1,false));
    println!("NATIVE_MEMORY_PROBE allocate=ok map=ok backing=ok unmap=ok free=ok zakuro_dependencies=0");
    println!("NATIVE_ADDRESS_SPACE_PROBE cross_page=ok permissions=ok privileged_write=ok native_cpu=ok aot_tables=ok");
    println!("NATIVE_MODULE_MEMORY_PROBE copy=ok mirror=ok trim=ok release=ok");
}
