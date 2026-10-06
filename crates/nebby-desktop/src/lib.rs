use nebby_core::*;
pub mod import;
pub mod recomp;
fn discover_app_root(executable:Option<&Path>,cwd:Option<&Path>)->Option<PathBuf>{
    if let Some(parent)=executable.and_then(Path::parent){
        for root in parent.ancestors(){
            if root.join("titles").is_dir(){return Some(root.to_path_buf());}
        }
    }
    if let Some(cwd)=cwd{
        for root in cwd.ancestors(){
            if root.join("titles").is_dir()&&root.join("Cargo.toml").is_file(){return Some(root.to_path_buf());}
        }
    }
    None
}
pub fn app_root()->PathBuf {
    fn normalized(path:PathBuf)->PathBuf{path.canonicalize().unwrap_or(path)}
    if let Some(path)=std::env::var_os("NEBBY_APP_ROOT"){return normalized(path.into());}
    let executable=std::env::current_exe().ok();
    let cwd=std::env::current_dir().ok();
    if let Some(root)=discover_app_root(executable.as_deref(),cwd.as_deref()){return normalized(root);}
    normalized(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}
pub fn managed_tool(name:&str,development:&Path)->PathBuf {
    let override_name=if name=="zakuro"{"NEBBY_ZAKURO"}else{"NEBBY_3DSRECOMP"};
    if let Some(path)=std::env::var_os(override_name){return path.into();}
    let root=app_root();
    let bundled=if name=="zakuro"{root.join("runtimes/zakuro/zakuro")}else{root.join("tools/3dsrecomp/3dsrecomp")};
    if bundled.is_file(){bundled}else{development.join("tools/external").join(name).join("target/release").join(name)}
}
pub fn log_tail(path:&Path)->String {
    use std::io::{Read,Seek,SeekFrom};
    let result=(||->std::io::Result<String>{let mut file=fs::File::open(path)?;let length=file.metadata()?.len();file.seek(SeekFrom::Start(length.saturating_sub(65536)))?;let mut bytes=Vec::new();file.take(65536).read_to_end(&mut bytes)?;Ok(String::from_utf8_lossy(&bytes).into_owned())})();
    result.unwrap_or_else(|_|"No log recorded yet.".into())
}
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
pub struct Desktop {
    pub root: PathBuf,
}
impl Desktop {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        for d in [
            "configs",
            "profiles",
            "builds",
            "shader-caches",
            "mods",
            "logs",
        ] {
            fs::create_dir_all(root.join(d)).map_err(|e| e.to_string())?;
        }
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        Ok(Self { root })
    }
    pub fn load(&self) -> Result<Config, String> {
        let mut c: Config = serde_json::from_slice(&self.read("configs/settings.json")?)
            .map_err(|e| e.to_string())?;
        if let Ok(p) = Path::new(&c.workspace).canonicalize() {
            c.workspace = p.to_string_lossy().into_owned();
        } else if app_root().join("workspace/static-recomp-work/nativeization/remove-zakuro/moon-native/Cargo.toml").is_file(){
            c.workspace=default_workspace();
        }
        if std::env::var("NEBBY_PORTABLE").as_deref()==Ok("1"){
            c.workspace=default_workspace();
            c.save_root=self.root.join("profiles/alola/saves").to_string_lossy().into_owned();
        }
        Ok(c)
    }
    pub fn save(&self, c: &Config) -> Result<(), String> {
        self.write(
            "configs/settings.json",
            &serde_json::to_vec_pretty(c).map_err(|e| e.to_string())?,
        )
    }
    pub fn load_library(&self)->Result<nebby_core::library::Library,String> {
        let path=self.root.join("configs/library.json");
        if !path.exists(){return Ok(Default::default());}
        serde_json::from_slice(&fs::read(path).map_err(|e|e.to_string())?).map_err(|e|format!("Library metadata is invalid: {e}"))
    }
    pub fn save_library(&self,library:&nebby_core::library::Library)->Result<(),String> {
        self.write("configs/library.json",&serde_json::to_vec_pretty(library).map_err(|e|e.to_string())?)
    }
    pub fn install_mod(&self, path: &Path) -> Result<ModDescriptor, String> {
        let m: ModDescriptor = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if m.id.is_empty()
            || !m.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || !m.available
            || m.environment.is_empty()
        {
            return Err(
                "Installed mod needs a safe id, available=true and real environment bindings"
                    .into(),
            );
        }
        let bytes = self
            .read("mods/installed.json")
            .unwrap_or_else(|_| b"[]".to_vec());
        let mut mods: Vec<ModDescriptor> =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        mods.retain(|old| old.id != m.id);
        mods.push(m.clone());
        self.write(
            "mods/installed.json",
            &serde_json::to_vec_pretty(&mods).map_err(|e| e.to_string())?,
        )?;
        Ok(m)
    }
    pub fn command(&self, p: &CommandPlan, kind: &str) -> Result<Child, String> {
        if kind=="launch" && p.environment.get("NEBBY_MODERN_CAMERA_FOLLOW_PROBE").is_some_and(|v|v=="1") {
            let output=Command::new(&p.executable).arg("--capabilities").output().map_err(|e|format!("Cannot check Modern Camera runtime: {e}"))?;
            if !output.status.success() || !String::from_utf8_lossy(&output.stdout).lines().any(|line|line=="modern_camera=true") {
                return Err("Modern Camera needs a camera-enabled native build. Build / Update Recomp first.".into());
            }
        }
        self.write(
            &format!("builds/{kind}-plan.json"),
            &serde_json::to_vec_pretty(p).map_err(|e| e.to_string())?,
        )?;
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join(format!("logs/{kind}.log")))
            .map_err(|e| e.to_string())?;
        let mut command = Command::new(&p.executable);
        for key in &p.unset_environment {
            command.env_remove(key);
        }
        command
            .args(&p.arguments)
            .envs(&p.environment)
            .current_dir(&p.working_directory)
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log)
            ;
        let libraries=app_root().join("lib");
        if libraries.is_dir(){
            let mut paths=vec![libraries];
            if let Some(existing)=std::env::var_os("LD_LIBRARY_PATH"){paths.extend(std::env::split_paths(&existing));}
            if let Ok(value)=std::env::join_paths(paths){command.env("LD_LIBRARY_PATH",value);}
        }
        command.spawn()
            .map_err(|e| e.to_string())
    }
    pub fn verify(&self, c: &Config, t: &Title) -> Vec<(String, bool)> {
        let runtime_ready = self.native_build_state(c, t) == nebby_core::library::BuildState::Ready;
        let mut checks: Vec<(String, bool)> = [("Game data",t.input.as_str()),("Build project",t.build_manifest.as_str()),("Native filesystem library","static-recomp-work/native-filesystem/build-bridge/libctr_native_filesystem.so"),("Renderer library","static-recomp-work/native-renderer/build-probe/libmoon_triaevum_frontend_native.so"),("Native GSP library","static-recomp-work/nativeization/stage1-gsp/build/libctr_native_gsp.so")].into_iter().map(|(n,p)|(n.into(),Path::new(&join(&c.workspace,p)).is_file())).collect();
        checks.insert(1, ("Native runtime build (includes audio and input)".into(), runtime_ready));
        checks
    }
    pub fn native_build_state(&self,c:&Config,t:&Title)->nebby_core::library::BuildState{
        use nebby_core::library::BuildState;
        if !Path::new(&join(&c.workspace,&t.input)).is_file(){return BuildState::NotImported;}
        if let Ok(error)=fs::read_to_string(self.root.join("builds").join(&t.id).join("build-error.txt")){
            if !error.trim().is_empty(){return BuildState::Failed(error);}
        }
        let executable=PathBuf::from(join(&c.workspace,&t.executable));
        let Ok(built)=executable.metadata().and_then(|m|m.modified()) else{return BuildState::NeedsBuild;};
        if crate::recomp::recorded_build_outdated(&self.root.join("builds").join(&t.id)){return BuildState::Outdated;}
        let source=PathBuf::from(join(&c.workspace,&t.build_manifest));
        fn newer(path:&Path,built:std::time::SystemTime)->bool{
            if path.is_dir(){return fs::read_dir(path).is_ok_and(|entries|entries.filter_map(Result::ok).any(|entry|entry.file_name()!="target"&&newer(&entry.path(),built)));}
            if !matches!(path.extension().and_then(|s|s.to_str()),Some("rs"|"toml"|"c"|"h")){return false;}
            path.metadata().and_then(|m|m.modified()).is_ok_and(|time|time>built)
        }
        let sources=source.parent().and_then(Path::parent).unwrap_or(&source);
        if newer(sources,built){BuildState::Outdated}else{BuildState::Ready}
    }
    pub fn open_folder(&self, path: &Path) -> Result<(), String> {
        fs::create_dir_all(path).map_err(|e| e.to_string())?;
        Command::new(if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        })
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
    }
}
impl Storage for Desktop {
    fn read(&self, key: &str) -> Result<Vec<u8>, String> {
        if key.contains("..") || Path::new(key).is_absolute() {
            return Err("Invalid storage key".into());
        }
        fs::read(self.root.join(key)).map_err(|e| e.to_string())
    }
    fn write(&self, key: &str, bytes: &[u8]) -> Result<(), String> {
        if key.contains("..") || Path::new(key).is_absolute() {
            return Err("Invalid storage key".into());
        }
        let p = self.root.join(key);
        let temp = p.with_extension("json.tmp");
        let mut f = fs::File::create(&temp).map_err(|e| e.to_string())?;
        use std::io::Write;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(temp, p).map_err(|e| e.to_string())
    }
}
impl Launcher for Desktop {
    type Handle = Child;
    fn launch(&self, p: &CommandPlan) -> Result<Child, String> {
        self.command(p, "launch")
    }
}
impl Builder for Desktop {
    type Handle = Child;
    fn build(&self, p: &CommandPlan) -> Result<Child, String> {
        self.command(p, "build")
    }
}
pub fn default_root() -> PathBuf {
    std::env::var_os("NEBBY_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| app_root().join("data"))
}
pub fn default_workspace() -> String {
    std::env::var("POKEMOON_WORKSPACE").unwrap_or_else(|_| {
        let root=app_root();
        let packaged=root.join("workspace");
        let development=root.join("../pokemonMoondecomp");
        let selected=if packaged.is_dir()||!development.is_dir(){packaged}else{development};
        selected.canonicalize().unwrap_or(selected).to_string_lossy().into_owned()
    })
}
pub fn manifests() -> Result<(Vec<Title>, Vec<ModDescriptor>), String> {
    let root = app_root();
    let mut titles = Vec::new();
    for e in fs::read_dir(root.join("titles")).map_err(|e| e.to_string())? {
        let p = e.map_err(|e| e.to_string())?.path();
        if p.extension().is_some_and(|x| x == "json") {
            titles.push(
                serde_json::from_slice(&fs::read(p).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
            );
        }
    }
    let mut mods: Vec<ModDescriptor> = serde_json::from_slice(
        &fs::read(root.join("mods/catalog.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    for entry in fs::read_dir(root.join("mods")).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path().join("mod.json");
        if path.is_file() {
            let m: ModDescriptor =
                serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            mods.retain(|old| old.id != m.id);
            mods.push(m);
        }
    }
    let installed = default_root().join("mods/installed.json");
    if installed.exists() {
        let extras: Vec<ModDescriptor> =
            serde_json::from_slice(&fs::read(installed).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        for m in extras {
            mods.retain(|old| old.id != m.id);
            mods.push(m);
        }
    }
    titles.sort_by(|a: &Title, b: &Title| a.name.cmp(&b.name));
    if titles.is_empty() {
        return Err("No title manifests found".into());
    }
    Ok((titles, mods))
}

#[cfg(test)]
mod tests {
    #[test]
    fn installed_resources_win_over_another_checkout_cwd(){
        let unique=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let temporary=std::env::temp_dir().join(format!("nebby-roots-{}-{unique}",std::process::id()));
        let bundle=temporary.join("bundle");let other=temporary.join("other-checkout");
        std::fs::create_dir_all(bundle.join("titles")).unwrap();
        std::fs::create_dir_all(other.join("titles")).unwrap();
        std::fs::write(other.join("Cargo.toml"),b"fixture").unwrap();
        assert_eq!(super::discover_app_root(Some(&bundle.join("nebby-ui")),Some(&other)),Some(bundle));
        std::fs::remove_dir_all(temporary).unwrap();
    }
    use super::*;
    #[test]
    fn missing_workspace_recovers_bundled_sdk() {
        let root=std::env::temp_dir().join(format!("nebby-sdk-recovery-{}",std::process::id()));
        let desktop=Desktop::new(root.clone()).unwrap();
        let (titles,_)=manifests().unwrap();
        let mut settings=Config::new(&titles[0],root.join("missing-sdk").to_string_lossy().into_owned(),root.join("saves").to_string_lossy().into_owned());
        let key=modern_camera::profile_key(&titles[0].id,&settings.profile);
        let mut camera=modern_camera::Settings::default();camera.distance=350.;camera.recenter_key="F8".into();
        settings.modern_camera.insert(key.clone(),camera);
        desktop.save(&settings).unwrap();
        let loaded=desktop.load().unwrap();
        assert_eq!(loaded.modern_camera[&key].distance,350.);
        assert_eq!(loaded.modern_camera[&key].recenter_key,"F8");
        assert_eq!(loaded.workspace,default_workspace());
        assert!(Path::new(&loaded.workspace).join("static-recomp-work/nativeization/remove-zakuro/moon-native/Cargo.toml").is_file());
        assert!(!app_root().to_string_lossy().contains("/../"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn storage_roundtrip_and_escape_rejection() {
        let root = std::env::temp_dir().join(format!(
            "nebby-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let d = Desktop::new(root.clone()).unwrap();
        d.write("configs/test.json", b"hello").unwrap();
        assert_eq!(d.read("configs/test.json").unwrap(), b"hello");
        assert!(d.write("../escape", b"bad").is_err());
        assert!(d.read("/absolute").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn process_failure_is_reported() {
        let d = Desktop::new(
            std::env::temp_dir().join(format!("nebby-process-test-{}", std::process::id())),
        )
        .unwrap();
        let p = CommandPlan {
            unset_environment: vec![],
            executable: "/nonexistent/nebby-test-program".into(),
            arguments: vec![],
            environment: Default::default(),
            working_directory: "/".into(),
        };
        assert!(d.launch(&p).is_err());
        std::fs::remove_dir_all(d.root).unwrap();
    }
    #[test]
    fn imported_mod_is_persistent() {
        let d = Desktop::new(
            std::env::temp_dir().join(format!("nebby-mod-test-{}", std::process::id())),
        )
        .unwrap();
        let source = d.root.join("mods/source.json");
        let m = ModDescriptor {
            id: "trace-test".into(),
            name: "Trace".into(),
            description: "Diagnostics".into(),
            titles: vec!["0004000000175E00".into()],
            platforms: vec!["desktop".into()],
            available: true,
            required_features: vec![],
            revisions: vec![],
            environment: std::collections::BTreeMap::from([("RUST_LOG".into(), "debug".into())]),
        };
        std::fs::write(&source, serde_json::to_vec(&m).unwrap()).unwrap();
        assert_eq!(d.install_mod(&source).unwrap().id, m.id);
        let saved: Vec<ModDescriptor> =
            serde_json::from_slice(&d.read("mods/installed.json").unwrap()).unwrap();
        assert_eq!(saved[0].environment["RUST_LOG"], "debug");
        std::fs::remove_dir_all(d.root).unwrap();
    }
}
