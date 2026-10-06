fn main() {
    println!("cargo:rustc-check-cfg=cfg(nebby_modern_camera)");
    println!("cargo:rerun-if-env-changed=NEBBY_RECOMP_CODE_DIR");
    let code = std::path::PathBuf::from(std::env::var_os("NEBBY_RECOMP_CODE_DIR").expect("Alpha Sapphire needs its own generated code directory"));
    assert!(code.join("librecomp.a").is_file(), "missing Alpha Sapphire AOT archive");
    println!("cargo:rustc-link-search=native={}", code.display());
    println!("cargo:rustc-link-lib=static=recomp");
    println!("cargo:rustc-link-lib=m");
    println!("cargo:rerun-if-changed={}", code.join("librecomp.a").display());
}
