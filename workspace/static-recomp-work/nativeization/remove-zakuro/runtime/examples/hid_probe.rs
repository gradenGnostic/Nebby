//! Production HID ABI probe, independent of any emulator process shell.
use pokemon3ds_runtime::services::hid::{HidInput,HidProducer};
fn main(){
    let path=std::env::args().nth(1).expect("native CTR library path");
    let mut hid=HidProducer::open(&path).expect("production HID producer");
    let mut words=[0u32;64];words[0]=0x000a0000;
    hid.dispatch(&mut words).expect("HID resources IPC");
    assert_eq!(words[1],0,"HID resources result");
    let mut bytes=[0u8;0x1000];
    for (tick,buttons) in [(1,1),(2,1),(3,0)] {
        let (size,signals)=hid.sample(HidInput{buttons,circle_x:0.5,..Default::default()},tick,&mut bytes).expect("production HID sample");
        assert_eq!(size,0x1000);assert_ne!(signals,0);
    }
    println!("NATIVE_HID_PROBE resources=ok samples=3 zakuro_dependencies=0");
}
