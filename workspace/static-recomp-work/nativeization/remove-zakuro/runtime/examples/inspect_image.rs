use pokemon3ds_runtime::process::image::{TitleProfile,NativeProcessImage};
fn main(){let args:Vec<_>=std::env::args().collect();assert_eq!(args.len(),3);
    let image=NativeProcessImage::open(std::path::Path::new(&args[1]),TitleProfile::load(std::path::Path::new(&args[2])).unwrap()).unwrap();
    println!("NATIVE_IMAGE program={:016x} code_bytes={} regions={} entry={:08x} stack_size={} priority={} app_bytes={} romfs={:?}",image.title.program_id(),image.code.len(),image.segments.len(),image.entry,image.stack_size,image.priority,image.app_bytes,image.title.romfs_level3_range());
}
