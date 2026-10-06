//! Launch through the production Nebby path without changing user settings.
use nebby_core::{Config,Title,Launcher,launch_plan};
fn main(){
    let args:Vec<_>=std::env::args().collect();assert_eq!(args.len(),3,"workspace temporary-data-root");
    let title:Title=serde_json::from_str(include_str!("../../../titles/moon.json")).unwrap();
    let save=format!("{}/static-recomp-work/nativeization/stage0-baseline/test-profile",args[1]);
    let config=Config::new(&title,args[1].clone(),save);
    let mut plan=launch_plan(&config,&title,&[]).unwrap();
    plan.environment.insert("NEBBY_MOD_SINGLE_SCREEN".into(),"0".into());
    plan.environment.insert("RUST_LOG".into(),"info".into());
    plan.environment.insert("RUST_BACKTRACE".into(),"1".into());
    let desktop=nebby_desktop::Desktop::new(args[2].clone().into()).unwrap();
    let mut child=desktop.launch(&plan).unwrap();
    println!("NEBBY_NATIVE_SMOKE child_pid={}",child.id());
    let status=child.wait().unwrap();println!("NEBBY_NATIVE_SMOKE exit={status}");
    assert!(status.success());
}
