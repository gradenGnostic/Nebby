//! Managed native build orchestration. Child-process mode keeps egui responsive.
use std::{path::{Path,PathBuf},process::Command};
use sha2::{Digest,Sha256};
use std::io::Read;

#[derive(serde::Serialize,serde::Deserialize)]
struct RecompStamp{identity:String,archive_sha256:String}
#[derive(serde::Serialize,serde::Deserialize)]
struct ArtifactSnapshot{path:PathBuf,len:u64,modified_ns:u128}
impl ArtifactSnapshot{
    fn capture(path:PathBuf)->Result<Self,String>{
        let metadata=path.metadata().map_err(|e|e.to_string())?;
        let modified_ns=metadata.modified().map_err(|e|e.to_string())?.duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();
        Ok(Self{path,len:metadata.len(),modified_ns})
    }
    fn changed(&self)->bool{Self::capture(self.path.clone()).map_or(true,|current|current.len!=self.len||current.modified_ns!=self.modified_ns)}
}
#[derive(serde::Serialize,serde::Deserialize)]
struct BuildRecord{identity:String,artifacts:Vec<ArtifactSnapshot>}
pub(crate) fn recorded_build_outdated(cache:&Path)->bool{
    std::fs::read(cache.join("build.json")).ok().and_then(|bytes|serde_json::from_slice::<BuildRecord>(&bytes).ok()).is_some_and(|record|record.artifacts.iter().any(ArtifactSnapshot::changed))
}
fn file_hash(path:&Path)->Result<String,String>{
    let mut file=std::fs::File::open(path).map_err(|e|format!("{}: {e}",path.display()))?;
    let mut hash=Sha256::new();let mut bytes=[0;65536];
    loop{let count=file.read(&mut bytes).map_err(|e|e.to_string())?;if count==0{break;}hash.update(&bytes[..count]);}
    Ok(format!("{:x}",hash.finalize()))
}
fn override_files(root:&Path)->Result<Vec<PathBuf>,String>{
    fn visit(path:&Path,files:&mut Vec<PathBuf>)->Result<(),String>{
        for entry in std::fs::read_dir(path).map_err(|e|e.to_string())?{
            let entry=entry.map_err(|e|e.to_string())?;let kind=entry.file_type().map_err(|e|e.to_string())?;
            if kind.is_symlink(){return Err("Override symlinks are not supported".into());}
            if kind.is_dir(){visit(&entry.path(),files)?;}else if kind.is_file(){files.push(entry.path());}
        }Ok(())
    }
    let mut files=Vec::new();visit(root,&mut files)?;files.sort();
    Ok(files)
}
fn override_hash(root:&Path)->Result<String,String>{
    let files=override_files(root)?;
    let mut hash=Sha256::new();
    for file in files{hash.update(file.strip_prefix(root).unwrap().to_string_lossy().as_bytes());hash.update([0]);hash.update(file_hash(&file)?.as_bytes());}
    Ok(format!("{:x}",hash.finalize()))
}
fn sdk_source_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.is_dir() { return Ok(()); }
    files.push(root.to_owned()); // Directory timestamps detect additions/removals.
    for entry in std::fs::read_dir(root).map_err(|e|e.to_string())? {
        let entry = entry.map_err(|e|e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name.starts_with("build") || matches!(name.as_ref(), "target" | "evidence" | "node_modules") { continue; }
        let kind = entry.file_type().map_err(|e|e.to_string())?;
        if kind.is_symlink() { continue; }
        if kind.is_dir() { sdk_source_files(&entry.path(), files)?; }
        else if name == "CMakeLists.txt" || matches!(entry.path().extension().and_then(|s|s.to_str()), Some("c" | "cpp" | "cc" | "h" | "hpp" | "cmake" | "glsl" | "vert" | "frag")) {
            files.push(entry.path());
        }
    }
    Ok(())
}
fn current_archive(generated:&Path,identity:&str)->bool{
    std::fs::read(generated.join("nebby-recomp.json")).ok().and_then(|bytes|serde_json::from_slice::<RecompStamp>(&bytes).ok()).is_some_and(|stamp|stamp.identity==identity&&file_hash(&generated.join("code/librecomp.a")).is_ok_and(|hash|hash==stamp.archive_sha256))
}
fn stamp_archive(generated:&Path,identity:&str)->Result<(),String>{
    let stamp=RecompStamp{identity:identity.into(),archive_sha256:file_hash(&generated.join("code/librecomp.a"))?};
    std::fs::write(generated.join("nebby-recomp.json"),serde_json::to_vec(&stamp).map_err(|e|e.to_string())?).map_err(|e|e.to_string())
}

fn run(program:&Path,args:&[String],root:&Path)->Result<(),String>{
    let status=Command::new(program).args(args).env("RECOMP3DS_JOBS","2").current_dir(root).status().map_err(|e|format!("Cannot start {}: {e}",program.display()))?;
    if !status.success(){return Err(format!("{} failed ({status}); view build log",program.display()));}
    Ok(())
}
fn build_renderer_sdk(workspace: &Path) -> Result<(), String> {
    let source = workspace.join("static-recomp-work/native-renderer/runtime-project");
    if !source.join("CMakeLists.txt").is_file() { return Ok(()); }
    println!("STAGE Building native renderer SDK");
    let build = std::env::var_os("NEBBY_RENDERER_BUILD_DIR").map(PathBuf::from).unwrap_or_else(||workspace.join("static-recomp-work/native-renderer/build-sdk"));
    run(Path::new("cmake"), &[
        "-S".into(), source.to_string_lossy().into_owned(),
        "-B".into(), build.to_string_lossy().into_owned(),
        "-DCMAKE_BUILD_TYPE=Release".into(),
        "-DCMAKE_BUILD_RPATH=$ORIGIN/../../../../lib".into(),
        "-DCMAKE_INSTALL_RPATH=$ORIGIN/../../../../lib".into(),
        "-DCMAKE_BUILD_WITH_INSTALL_RPATH=ON".into(),
    ], workspace)?;
    run(Path::new("cmake"), &[
        "--build".into(), build.to_string_lossy().into_owned(),
        "--target".into(), "moon_triaevum_frontend_native".into(),
        "--parallel".into(), "2".into(),
    ], workspace)?;
    let destination = workspace.join("static-recomp-work/native-renderer/build-probe/libmoon_triaevum_frontend_native.so");
    let compiled = build.join("libmoon_triaevum_frontend_native.so");
    if !compiled.is_file() { return Err("Native renderer SDK output missing".into()); }
    std::fs::create_dir_all(destination.parent().ok_or("Renderer output directory missing")?).map_err(|e|e.to_string())?;
    // Preserve mtime on a warm build, so artifact readiness stays stable.
    if !destination.is_file() || file_hash(&compiled)? != file_hash(&destination)? {
        let pending = destination.with_extension("so.pending");
        std::fs::copy(compiled, &pending).map_err(|e|format!("Cannot stage native renderer: {e}"))?;
        std::fs::rename(pending, destination).map_err(|e|format!("Cannot install native renderer: {e}"))?;
    }
    Ok(())
}
fn build_service_sdk(workspace: &Path) -> Result<(), String> {
    // Packaged SDKs are source-built. Existing development baselines remain intact.
    if !workspace.join("static-recomp-work/native-renderer/runtime-project/CMakeLists.txt").is_file() { return Ok(()); }
    println!("STAGE Building native CTR service SDK");
    let source = workspace.join("static-recomp-work/nativeization/stage1-gsp");
    let build = std::env::var_os("NEBBY_SERVICE_BUILD_DIR").map(PathBuf::from).unwrap_or_else(||source.join("build-sdk"));
    let headers = workspace.join("static-recomp-work/native-renderer/deps/usr/include");
    let mut args = vec!["-S".into(), source.to_string_lossy().into_owned(), "-B".into(), build.to_string_lossy().into_owned(), "-DCMAKE_BUILD_TYPE=Release".into(), "-DCMAKE_BUILD_RPATH=$ORIGIN/../../../../../lib".into()];
    args.extend(["-DCMAKE_INSTALL_RPATH=$ORIGIN/../../../../../lib".into(), "-DCMAKE_BUILD_WITH_INSTALL_RPATH=ON".into()]);
    if headers.join("nlohmann/json.hpp").is_file() { args.push(format!("-DCTR_JSON_INCLUDE_DIR={}", headers.display())); }
    run(Path::new("cmake"), &args, workspace)?;
    run(Path::new("cmake"), &["--build".into(), build.to_string_lossy().into_owned(), "--target".into(), "ctr_native_gsp".into(), "--parallel".into(), "2".into()], workspace)?;
    let compiled = build.join("libctr_native_gsp.so");
    let destination = source.join("build/libctr_native_gsp.so");
    if !compiled.is_file() { return Err("Native service SDK output missing".into()); }
    std::fs::create_dir_all(destination.parent().ok_or("Service output directory missing")?).map_err(|e|e.to_string())?;
    if !destination.is_file() || file_hash(&compiled)? != file_hash(&destination)? {
        let pending = destination.with_extension("so.pending");
        std::fs::copy(compiled, &pending).map_err(|e|e.to_string())?;
        std::fs::rename(pending, destination).map_err(|e|e.to_string())?;
    }
    Ok(())
}
fn build_filesystem_sdk(workspace: &Path) -> Result<(), String> {
    if !workspace.join("static-recomp-work/native-renderer/runtime-project/CMakeLists.txt").is_file() { return Ok(()); }
    println!("STAGE Building native filesystem SDK");
    let source = workspace.join("static-recomp-work/native-filesystem");
    let build = source.join("build-sdk");
    run(Path::new("cmake"), &["-S".into(), source.to_string_lossy().into_owned(), "-B".into(), build.to_string_lossy().into_owned(), "-DCMAKE_BUILD_TYPE=Release".into(), "-DCMAKE_INSTALL_RPATH=$ORIGIN/../../../../lib".into(), "-DCMAKE_BUILD_WITH_INSTALL_RPATH=ON".into()], workspace)?;
    run(Path::new("cmake"), &["--build".into(), build.to_string_lossy().into_owned(), "--target".into(), "ctr_native_filesystem".into(), "--parallel".into(), "2".into()], workspace)?;
    let compiled = build.join("libctr_native_filesystem.so");
    let destination = source.join("build-bridge/libctr_native_filesystem.so");
    if !compiled.is_file() { return Err("Native filesystem SDK output missing".into()); }
    std::fs::create_dir_all(destination.parent().ok_or("Filesystem output directory missing")?).map_err(|e|e.to_string())?;
    if !destination.is_file() || file_hash(&compiled)? != file_hash(&destination)? {
        let pending = destination.with_extension("so.pending");
        std::fs::copy(compiled, &pending).map_err(|e|e.to_string())?;
        std::fs::rename(pending, destination).map_err(|e|e.to_string())?;
    }
    Ok(())
}
pub fn build_native(workspace:&Path,input:&Path,manifest:&Path,tool:&Path,cache:&Path)->Result<(),String>{
    println!("STAGE Preparing game");
    let game=crate::import::inspect(input)?;
    let title=nebby_core::library::catalog().into_iter().find(|t|t.title_id==game.title_id).ok_or("Unknown title")?;
    if title.backend!=nebby_core::library::RuntimeBackend::NativeRecomp{return Err("Native recompilation unavailable for selected title".into());}
    if !tool.is_file(){return Err("Managed 3dsrecomp executable missing".into());}
    std::fs::create_dir_all(cache).map_err(|e|e.to_string())?;
    let tool_hash=file_hash(tool)?;
    let analysis_stamp=cache.join("analysis.json");
    let identity=format!("{}:{}",game.code_sha256,tool_hash);
    if std::fs::read_to_string(&analysis_stamp).ok().as_deref()!=Some(identity.as_str()){
        println!("STAGE Analyzing executable");
        run(tool,&["analyze".into(),input.to_string_lossy().into_owned()],workspace)?;
        std::fs::write(&analysis_stamp,&identity).map_err(|e|e.to_string())?;
    }else{println!("STAGE Analysis cache current");}
    // Retail-address overrides belong to Moon, never to another title.
    let moon=game.title_id=="0004000000175E00";
    let overrides=moon.then(||workspace.join("static-recomp-work/native-input/overrides"));
    let override_identity=match &overrides{Some(path)=>override_hash(path)?,None=>"no-title-overrides-v1".into()};
    let generation_identity=format!("{identity}:{override_identity}");
    let key=format!("{:x}",Sha256::digest(generation_identity.as_bytes()));
    let mut generated=std::env::var_os("NEBBY_RECOMP_DIR").map(PathBuf::from).unwrap_or_else(||cache.join("recomp").join(&key));
    if !current_archive(&generated,&generation_identity){
        println!("STAGE Generating recompilation");
        if overrides.as_ref().is_some_and(|path|!path.is_dir()){return Err("Native input overrides missing; cannot build correct native target".into());}
        // Never regenerate over an unverified/modified existing export.
        if generated.exists(){
            generated=cache.join("recomp").join(format!("{key}-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos()));
        }
        let mut args=vec!["port".into(),input.to_string_lossy().into_owned(),generated.to_string_lossy().into_owned()];
        if let Some(path)=&overrides{args.extend(["--overrides".into(),path.to_string_lossy().into_owned()]);}
        run(tool,&args,workspace)?;
        stamp_archive(&generated,&generation_identity)?;
    }else{println!("STAGE Recomp cache current (game, tool, overrides and archive verified)");}
    if !manifest.is_file(){return Err("Native runtime build project missing".into());}
    build_service_sdk(workspace)?;
    build_filesystem_sdk(workspace)?;
    build_renderer_sdk(workspace)?;
    println!("STAGE Building native runtime");
    let status=Command::new("cargo").args(["build","--release","-j2","--manifest-path"]).arg(manifest)
        .env("NEBBY_RECOMP_CODE_DIR",generated.join("code"))
        .env("NEBBY_MODERN_CAMERA_ROOT",crate::app_root().join("mods/modern-camera"))
        .env("CARGO_TARGET_DIR",workspace.join("static-recomp-work/native-renderer/moon-target")).current_dir(workspace).status().map_err(|e|format!("Native compiler unavailable: {e}"))?;
    if !status.success(){return Err("Native build failed; view build log".into());}
    println!("STAGE Finalizing");
    let mut artifacts=vec![input.to_owned(),tool.to_owned(),generated.join("code/librecomp.a")];
    if let Some(path)=&overrides{artifacts.extend(override_files(path)?);}
    if moon{sdk_source_files(&crate::app_root().join("mods/modern-camera"),&mut artifacts)?;}
    for relative in ["static-recomp-work/native-filesystem", "static-recomp-work/nativeization/stage1-gsp", "static-recomp-work/nativeization/remove-zakuro/stage1-services", "static-recomp-work/native-renderer/runtime-project", "static-recomp-work/native-renderer/moon_adapter", "tools/external/triaevum/runtime", "tools/external/triaevum/tools/oot3d"] {
        sdk_source_files(&workspace.join(relative), &mut artifacts)?;
    }
    for relative in ["static-recomp-work/nativeization/stage1-gsp/build/libctr_native_gsp.so","static-recomp-work/native-filesystem/build-bridge/libctr_native_filesystem.so","static-recomp-work/native-renderer/build-probe/libmoon_triaevum_frontend_native.so"]{
        let path=workspace.join(relative);if path.is_file(){artifacts.push(path);}
    }
    let record=BuildRecord{identity,artifacts:artifacts.into_iter().map(ArtifactSnapshot::capture).collect::<Result<_,_>>()?};
    std::fs::write(cache.join("build.json"),serde_json::to_vec(&record).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    println!("STAGE Ready");Ok(())
}
pub fn worker(args:impl Iterator<Item=String>)->Result<(),String>{
    let paths:Vec<PathBuf>=args.map(PathBuf::from).collect();
    if paths.len()!=5{return Err("Internal build worker requires workspace, game, manifest, tool and cache".into());}
    std::fs::create_dir_all(&paths[4]).map_err(|e|e.to_string())?;
    let error_path=paths[4].join("build-error.txt");
    std::fs::write(&error_path,"").map_err(|e|e.to_string())?;
    let result=build_native(&paths[0],&paths[1],&paths[2],&paths[3],&paths[4]);
    if let Err(error)=&result{
        std::fs::write(&error_path,error).map_err(|e|format!("{error}; failed to save build status: {e}"))?;
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn sdk_snapshot_tracks_source_but_not_build_outputs() {
        let root=std::env::temp_dir().join(format!("nebby-sdk-snapshot-{}",std::process::id()));
        std::fs::create_dir_all(root.join("build-cache")).unwrap();
        std::fs::write(root.join("bridge.cpp"), b"source").unwrap();
        std::fs::write(root.join("build-cache/output.cpp"), b"generated").unwrap();
        let mut files=Vec::new();super::sdk_source_files(&root,&mut files).unwrap();
        assert!(files.contains(&root.join("bridge.cpp")));
        assert!(!files.contains(&root.join("build-cache/output.cpp")));
        let snapshot=super::ArtifactSnapshot::capture(root.join("bridge.cpp")).unwrap();
        std::fs::write(root.join("bridge.cpp"),b"changed source").unwrap();
        assert!(snapshot.changed());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn artifact_changes_mark_recorded_build_outdated(){
        let root=std::env::temp_dir().join(format!("nebby-artifact-state-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&root).unwrap();let file=root.join("fixture");std::fs::write(&file,b"original").unwrap();
        let record=super::BuildRecord{identity:"fixture".into(),artifacts:vec![super::ArtifactSnapshot::capture(file.clone()).unwrap()]};
        std::fs::write(root.join("build.json"),serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(!super::recorded_build_outdated(&root));
        std::fs::write(&file,b"different-length").unwrap();assert!(super::recorded_build_outdated(&root));
        std::fs::remove_file(&file).unwrap();assert!(super::recorded_build_outdated(&root));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn recomp_cache_requires_matching_identity_and_unmodified_archive(){
        let root=std::env::temp_dir().join(format!("nebby-recomp-cache-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(root.join("code")).unwrap();
        let archive=root.join("code/librecomp.a");std::fs::write(&archive,b"fixture archive").unwrap();
        assert!(!super::current_archive(&root,"game:tool:overrides"));
        super::stamp_archive(&root,"game:tool:overrides").unwrap();
        assert!(super::current_archive(&root,"game:tool:overrides"));
        assert!(!super::current_archive(&root,"game:new-tool:overrides"));
        assert!(!super::current_archive(&root,"game:tool:new-overrides"));
        std::fs::write(&archive,b"changed archive").unwrap();
        assert!(!super::current_archive(&root,"game:tool:overrides"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn override_content_changes_invalidate_generation(){
        let root=std::env::temp_dir().join(format!("nebby-override-hash-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("input.c"),b"first").unwrap();let before=super::override_hash(&root).unwrap();
        assert_eq!(before,super::override_hash(&root).unwrap());
        std::fs::write(root.join("input.c"),b"second").unwrap();assert_ne!(before,super::override_hash(&root).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_worker_persists_actual_error() {
        let root=std::env::temp_dir().join(format!("nebby-build-error-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let args=[root.clone(),root.join("missing.cxi"),root.join("Cargo.toml"),root.join("tool"),root.join("cache")];
        let error=super::worker(args.into_iter().map(|p|p.to_string_lossy().into_owned())).unwrap_err();
        assert_eq!(std::fs::read_to_string(root.join("cache/build-error.txt")).unwrap(),error);
        std::fs::remove_dir_all(root).unwrap();
    }
}
