fn main(){
    println!("cargo:rustc-check-cfg=cfg(nebby_modern_camera)");
    println!("cargo:rerun-if-env-changed=NEBBY_MODERN_CAMERA_ROOT");
    let root=std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../..");
    println!("cargo:rerun-if-env-changed=NEBBY_RECOMP_CODE_DIR");
    let code=std::env::var_os("NEBBY_RECOMP_CODE_DIR").map(std::path::PathBuf::from).unwrap_or_else(||root.join("native-input/pokemoon-recomp-native/code"));
    println!("cargo:rustc-link-search=native={}",code.display());
    println!("cargo:rustc-link-lib=static=recomp");println!("cargo:rustc-link-lib=m");
    println!("cargo:rerun-if-changed={}",code.join("librecomp.a").display());
    // Preserve verified game-side native input synchronization. Single-screen
    // UI overrides are deliberately not part of this dual-screen bootstrap.
    let source=root.join("single-screen/overrides/native_vector_sync.c");
    let object=std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("native_vector_sync.o");
    let status=std::process::Command::new(std::env::var("CC").unwrap_or("cc".into())).args(["-O2","-fPIC","-c"]).arg(&source).arg("-I").arg(&code).arg("-o").arg(&object).status().expect("compile native input synchronization");
    assert!(status.success());println!("cargo:rustc-link-arg={}",object.display());
    for symbol in ["override_0x00498930","override_0x004989E4","override_0x001049C8","f_00346C98","override_0x004983E8","override_0x0049868C","override_0x0049869C","override_0x00498704","override_0x00498720","override_0x00498730","override_0x00498754","override_0x00498F08","override_0x001049F8","override_0x00498F40","override_0x00498F90","override_0x00498FD8"]{println!("cargo:rustc-link-arg=-Wl,--wrap={symbol}");}
    println!("cargo:rerun-if-changed={}",source.display());
    if let Some(mod_root)=std::env::var_os("NEBBY_MODERN_CAMERA_ROOT") {
        println!("cargo:rustc-cfg=nebby_modern_camera");
        let mod_root=std::path::PathBuf::from(mod_root);
        let core=mod_root.join("core/modern_camera.c");
        let core_object=std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("modern_camera_core.o");
        let status=std::process::Command::new(std::env::var("CC").unwrap_or("cc".into()))
            .args(["-O2","-fPIC","-Wall","-Wextra","-c"]).arg(&core).arg("-o").arg(&core_object).status().expect("compile camera core");
        assert!(status.success());
        println!("cargo:rerun-if-changed={}",core.display());
        println!("cargo:rerun-if-changed={}",mod_root.join("core/modern_camera.h").display());
        println!("cargo:rustc-link-arg={}",core_object.display());
        let source=mod_root.join("adapters/moon/camera_probe.c");
        let object=std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("modern_camera_probe.o");
        let status=std::process::Command::new(std::env::var("CC").unwrap_or("cc".into()))
            .args(["-O2","-fPIC","-Wall","-Wextra","-c"]).arg(&source)
            .arg("-I").arg(&code).arg("-o").arg(&object).status().expect("compile camera probe");
        assert!(status.success());
        println!("cargo:rerun-if-changed={}",source.display());
        println!("cargo:rustc-link-arg={}",object.display());
        for symbol in ["f_0040D03C","f_0040D2F8","f_004A3CF4","f_004A5674","f_0036C140","m040_f_000338B0","m040_f_0005BCD0","m040_f_00035DEC","m040_f_0006FC94"] { println!("cargo:rustc-link-arg=-Wl,--wrap={symbol}"); }
    }
}
