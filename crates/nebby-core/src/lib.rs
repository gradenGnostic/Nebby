//! Platform-neutral manifests, profile state and execution plans. No process/filesystem APIs.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub mod library;
pub mod modern_camera;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Feature {
    pub id: String,
    pub name: String,
    pub description: String,
    pub available: bool,
    pub configurable: bool,
    pub default: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub titles: Vec<String>,
    pub platforms: Vec<String>,
    pub available: bool,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub required_features: Vec<String>,
    #[serde(default)]
    pub revisions: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Title {
    pub id: String,
    pub name: String,
    pub region: String,
    pub revision: String,
    pub executable: String,
    pub input: String,
    pub build_manifest: String,
    #[serde(default)]
    pub native_profile: Option<String>,
    pub features: Vec<Feature>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub workspace: String,
    pub title: String,
    pub profile: String,
    pub save_root: String,
    pub features: BTreeMap<String, bool>,
    pub mods: BTreeMap<String, bool>,
    #[serde(default)]
    pub graphics: GraphicsSettings,
    #[serde(default)]
    pub keybinds: BTreeMap<String,BTreeMap<String,String>>,
    #[serde(default)]
    pub modern_camera: BTreeMap<String,modern_camera::Settings>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicsSettings {
    pub render_scale: u32,
    pub window_width: u32,
    pub window_height: u32,
}
impl Default for GraphicsSettings {
    fn default()->Self{Self{render_scale:1,window_width:1280,window_height:720}}
}
impl Config {
    pub fn new(title: &Title, workspace: String, save_root: String) -> Self {
        Self {
            version: 1,
            workspace,
            title: title.id.clone(),
            profile: "Alola".into(),
            save_root,
            features: title
                .features
                .iter()
                .map(|f| (f.id.clone(), f.default))
                .collect(),
            mods: BTreeMap::new(),
            graphics: GraphicsSettings::default(),
            keybinds:BTreeMap::new(),
            modern_camera:BTreeMap::new(),
        }
    }
    pub fn enabled(&self, id: &str) -> bool {
        self.features.get(id).copied().unwrap_or(false)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommandPlan {
    pub executable: String,
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub unset_environment: Vec<String>,
    pub working_directory: String,
}
pub trait Storage {
    fn read(&self, key: &str) -> Result<Vec<u8>, String>;
    fn write(&self, key: &str, bytes: &[u8]) -> Result<(), String>;
}
pub trait Launcher {
    type Handle;
    fn launch(&self, plan: &CommandPlan) -> Result<Self::Handle, String>;
}
pub trait Builder {
    type Handle;
    fn build(&self, plan: &CommandPlan) -> Result<Self::Handle, String>;
}
pub fn join(root: &str, relative: &str) -> String {
    if relative.starts_with('/') {return relative.into();}
    format!(
        "{}/{}",
        root.trim_end_matches('/'),
        relative.trim_start_matches('/')
    )
}
pub fn launch_plan(c: &Config, t: &Title, mods: &[ModDescriptor]) -> Result<CommandPlan, String> {
    if c.version != 1 {
        return Err("Unsupported config version".into());
    }
    for f in &t.features {
        if c.enabled(&f.id) && !f.available {
            return Err(format!("{} is not implemented", f.name));
        }
        if !f.configurable && c.enabled(&f.id) != f.default {
            return Err(format!("{} is required by this build", f.name));
        }
    }
    let mut env = BTreeMap::from([
        (
            "POKEMOON_RENDERER".into(),
            if c.enabled("renderer") {
                "triaevum"
            } else {
                "zakuro"
            }
            .into(),
        ),
        (
            "POKEMOON_FILESYSTEM".into(),
            if c.enabled("filesystem") {
                "native-strict"
            } else {
                "zakuro"
            }
            .into(),
        ),
        ("POKEMOON_NATIVE_FS_ROOT".into(), c.save_root.clone()),
        (
            "POKEMOON_NATIVE_FS_LIB".into(),
            join(
                &c.workspace,
                "static-recomp-work/native-filesystem/build-bridge/libctr_native_filesystem.so",
            ),
        ),
        (
            "POKEMOON_TRIAEVUM_FRONTEND_LIB".into(),
            join(
                &c.workspace,
                "static-recomp-work/native-renderer/build-probe/libmoon_triaevum_frontend_native.so",
            ),
        ),
        ("ZAKURO_HID_GAMEPLAY_BYPASS".into(), "1".into()),
        ("OOT3D_GRAPHICS_VULKAN_ADAPTER".into(), "0".into()),
        ("OOT3D_GRAPHICS_PICA_DYNAMIC_RENDERING".into(), "0".into()),
        ("TRIAEVUM_NRI_SCANOUT".into(), "0".into()),
        ("TRIAEVUM_DUAL_SCREEN_SCANOUT".into(), "1".into()),
        // Remote desktops may coalesce down/up between guest menu samples.
        ("NEBBY_REMOTE_INPUT".into(), "1".into()),
    ]);
    let bindings=c.keybinds.get(&t.id).cloned().unwrap_or_else(default_keybinds);
    if bindings.keys().any(|action|!default_keybinds().contains_key(action))||bindings.values().any(|key|!keyboard_keys().contains(&key.as_str())){return Err("Invalid keyboard binding".into());}
    env.insert("NEBBY_KEYBINDS".into(),bindings.iter().map(|(action,key)|format!("{action}={key}")).collect::<Vec<_>>().join(";"));
    for (id, enabled) in &c.mods {
        if !enabled {
            continue;
        }
        let m = mods
            .iter()
            .find(|m| &m.id == id)
            .ok_or_else(|| format!("Unknown mod: {id}"))?;
        if !m.available || !m.titles.contains(&t.id) || !m.platforms.iter().any(|p| p == "desktop")
        {
            return Err(format!("{} is unavailable for this title/platform", m.name));
        }
        if !m.revisions.is_empty() && !m.revisions.contains(&t.revision) {
            return Err(format!(
                "{} does not support revision {}",
                m.name, t.revision
            ));
        }
        for feature in &m.required_features {
            if !c.enabled(feature) {
                return Err(format!("{} requires runtime feature: {}", m.name, feature));
            }
        }
        for (k, v) in &m.environment {
            if env.contains_key(k) {
                return Err(format!("Mod conflicts with runtime: {k}"));
            }
            env.insert(k.clone(), v.clone());
        }
    }
    let unset_environment = mods
        .iter()
        .flat_map(|m| m.environment.keys())
        .filter(|key| !env.contains_key(*key))
        .cloned()
        .collect();
    let modern=c.mods.get("modern-camera").copied().unwrap_or(false);
    env.insert("NEBBY_MODERN_CAMERA_FOLLOW_PROBE".into(),u8::from(modern).to_string());
    env.insert("NEBBY_MODERN_CAMERA_TRACE".into(),"0".into());
    if modern {
        let key=modern_camera::profile_key(&t.id,&c.profile);
        let settings=c.modern_camera.get(&key).cloned().unwrap_or_default();
        env.extend(settings.environment()?);
        if c.keybinds.get(&t.id).is_none_or(|bindings|*bindings==default_keybinds()) {
            let mut bindings=default_keybinds();
            for (action,key) in [("circle_up","W"),("circle_down","S"),("circle_left","A"),("circle_right","D"),("x","C"),("y","V")] {bindings.insert(action.into(),key.into());}
            env.insert("NEBBY_KEYBINDS".into(),bindings.iter().map(|(action,key)|format!("{action}={key}")).collect::<Vec<_>>().join(";"));
        }
    }
    env.insert("TRIAEVUM_PICA_RESOLUTION_SCALE".into(),c.graphics.render_scale.clamp(1,4).to_string());
    env.insert("POKEMOON_WINDOW_WIDTH".into(),c.graphics.window_width.clamp(320,3840).to_string());
    env.insert("POKEMOON_WINDOW_HEIGHT".into(),c.graphics.window_height.clamp(240,2160).to_string());
    let arguments=if let Some(profile)=&t.native_profile{
        env.insert("POKEMON3DS_WINDOW_TITLE".into(),t.name.clone());
        if !c.enabled("renderer")||!c.enabled("filesystem"){return Err("Native runtime requires native renderer and filesystem".into());}
        vec![join(&c.workspace,&t.input),join(&c.workspace,profile),join(&c.workspace,"static-recomp-work/nativeization/stage1-gsp/build/libctr_native_gsp.so")]
    }else{vec!["--rasterizer".into(),"hardware".into(),"--resolution".into(),c.graphics.render_scale.clamp(1,4).to_string(),"--scale".into(),"1".into(),join(&c.workspace,&t.input)]};
    Ok(CommandPlan {
        unset_environment,
        executable: join(&c.workspace, &t.executable),
        arguments,
        environment: env,
        working_directory: c.workspace.clone(),
    })
}
pub fn build_plan(c: &Config, t: &Title) -> CommandPlan {
    CommandPlan {
        unset_environment: vec![],
        executable: "cargo".into(),
        arguments: vec![
            "build".into(),
            "--release".into(),
            "-j2".into(),
            "--manifest-path".into(),
            join(&c.workspace, &t.build_manifest),
        ],
        environment: BTreeMap::from([(
            "CARGO_TARGET_DIR".into(),
            join(
                &c.workspace,
                "static-recomp-work/native-renderer/moon-target",
            ),
        )]),
        working_directory: c.workspace.clone(),
    }
}
pub fn default_keybinds()->BTreeMap<String,String>{[("a","X"),("b","Z"),("x","S"),("y","A"),("l","Q"),("r","E"),("start","Return"),("select","Backspace"),("dpad_up","Up"),("dpad_down","Down"),("dpad_left","Left"),("dpad_right","Right"),("circle_up","I"),("circle_down","K"),("circle_left","J"),("circle_right","L")].into_iter().map(|(a,k)|(a.into(),k.into())).collect()}
pub fn keyboard_keys()->&'static [&'static str]{&["A","B","C","D","E","F","G","H","I","J","K","L","M","N","O","P","Q","R","S","T","U","V","W","X","Y","Z","0","1","2","3","4","5","6","7","8","9","Return","Backspace","Space","Tab","Up","Down","Left","Right","Left Shift","Right Shift","Left Ctrl","Right Ctrl"]}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn per_title_keybinds_persist_and_reach_native_host(){
        let t=title();let mut c=Config::new(&t,"/w".into(),"/s".into());
        let mut keys=default_keybinds();keys.insert("a".into(),"C".into());c.keybinds.insert(t.id.clone(),keys);
        c.keybinds.insert("other-game".into(),BTreeMap::from([("a".into(),"V".into())]));
        let loaded:Config=serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
        assert!(launch_plan(&loaded,&t,&[]).unwrap().environment["NEBBY_KEYBINDS"].contains("a=C"));
        c.keybinds.get_mut(&t.id).unwrap().insert("a".into(),"INVALID;KEY".into());assert!(launch_plan(&c,&t,&[]).is_err());
    }
    fn title() -> Title {
        serde_json::from_str(include_str!("../../../titles/moon.json")).unwrap()
    }
    #[test]
    fn alpha_uses_own_native_profile_and_executable(){
        let t:Title=serde_json::from_str(include_str!("../../../titles/alpha-sapphire.json")).unwrap();
        let c=Config::new(&t,"/workspace".into(),"/alpha-saves".into());
        let plan=launch_plan(&c,&t,&[]).unwrap();
        assert!(plan.executable.ends_with("/oras-native"));
        assert!(plan.arguments[1].ends_with("/alpha-sapphire/profile.json"));
        assert_eq!(plan.environment["POKEMON3DS_WINDOW_TITLE"],"Pokémon Alpha Sapphire");
        assert!(!plan.arguments.iter().any(|a|a=="--rasterizer"));
    }
    #[test]
    fn persistence_roundtrip() {
        let t = title();
        let c = Config::new(&t, "/workspace with spaces".into(), "/saves".into());
        let d: Config = serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
        assert_eq!(d.workspace, c.workspace);
        assert_eq!(d.features, c.features);
        assert_eq!(d.graphics.render_scale,1);
    }
    #[test]
    fn graphics_settings_reach_both_renderers_and_persist(){
        let t=title();let mut c=Config::new(&t,"/w".into(),"/s".into());
        c.graphics=GraphicsSettings{render_scale:2,window_width:1920,window_height:1080};
        let d:Config=serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
        let plan=launch_plan(&d,&t,&[]).unwrap();
        assert_eq!(plan.environment["TRIAEVUM_PICA_RESOLUTION_SCALE"],"2");
        assert_eq!(plan.environment["POKEMOON_WINDOW_WIDTH"],"1920");
        assert_eq!(plan.environment["POKEMOON_WINDOW_HEIGHT"],"1080");
        assert_eq!(plan.arguments.len(),3);
        let mut legacy=t.clone();legacy.native_profile=None;
        let legacy_plan=launch_plan(&d,&legacy,&[]).unwrap();
        let position=legacy_plan.arguments.iter().position(|a|a=="--resolution").unwrap();
        assert_eq!(legacy_plan.arguments[position+1],"2");
        let mut old=serde_json::to_value(&c).unwrap();old.as_object_mut().unwrap().remove("graphics");
        let old:Config=serde_json::from_value(old).unwrap();assert_eq!(old.graphics.render_scale,1);
        c.graphics.render_scale=999;
        assert_eq!(launch_plan(&c,&t,&[]).unwrap().environment["TRIAEVUM_PICA_RESOLUTION_SCALE"],"4");
    }
    #[test]
    fn real_plan_preserves_paths() {
        let t = title();
        let c = Config::new(&t, "/w space".into(), "/s".into());
        let p = launch_plan(&c, &t, &[]).unwrap();
        assert!(p.executable.starts_with("/w space/"));
        assert_eq!(p.environment["POKEMOON_RENDERER"], "triaevum");
        assert_eq!(p.environment["ZAKURO_HID_GAMEPLAY_BYPASS"], "1");
    }
    #[test]
    fn modern_camera_plan_and_original_mode() {
        let t=title();let mut c=Config::new(&t,"/sdk".into(),"/saves".into());
        let descriptor:ModDescriptor=serde_json::from_str(include_str!("../../../mods/modern-camera/mod.json")).unwrap();
        let off=launch_plan(&c,&t,&[descriptor.clone()]).unwrap();
        assert_eq!(off.environment["NEBBY_MODERN_CAMERA_FOLLOW_PROBE"],"0");
        c.mods.insert("modern-camera".into(),true);
        let mut settings=modern_camera::Settings::default();settings.fov_degrees=50.;
        c.modern_camera.insert(modern_camera::profile_key(&t.id,&c.profile),settings);
        let on=launch_plan(&c,&t,&[descriptor]).unwrap();
        assert_eq!(on.environment["NEBBY_MODERN_CAMERA_FOLLOW_PROBE"],"1");
        assert_eq!(on.environment["NEBBY_MODERN_CAMERA_FOV"],"50");
        assert!(on.environment["NEBBY_KEYBINDS"].contains("circle_up=W"));
    }
    #[test]
    fn unavailable_mod_rejected() {
        let t = title();
        let mut c = Config::new(&t, "/w".into(), "/s".into());
        c.mods.insert("single-screen".into(), true);
        assert!(launch_plan(&c, &t, &[]).is_err());
    }
    #[test]
    fn locked_features_enforced() {
        let t = title();
        let mut c = Config::new(&t, "/w".into(), "/s".into());
        c.features.insert("input".into(), false);
        assert!(launch_plan(&c, &t, &[]).is_err());
    }
    #[test]
    fn installed_binding_is_applied_and_conflicts_rejected() {
        let t = title();
        let mut c = Config::new(&t, "/w".into(), "/s".into());
        let mut m = ModDescriptor {
            id: "trace".into(),
            name: "Trace".into(),
            description: "".into(),
            titles: vec![t.id.clone()],
            platforms: vec!["desktop".into()],
            available: true,
            required_features: vec![],
            revisions: vec![],
            environment: BTreeMap::from([("RUST_LOG".into(), "debug".into())]),
        };
        c.mods.insert(m.id.clone(), true);
        assert_eq!(
            launch_plan(&c, &t, &[m.clone()]).unwrap().environment["RUST_LOG"],
            "debug"
        );
        m.environment
            .insert("POKEMOON_RENDERER".into(), "other".into());
        assert!(launch_plan(&c, &t, &[m]).is_err());
    }
}
