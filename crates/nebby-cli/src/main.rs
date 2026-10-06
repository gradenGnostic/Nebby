use nebby_core::*;
use nebby_desktop::*;
fn main() {
    if let Err(e) = run() {
        eprintln!("Nebby: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let desktop = Desktop::new(default_root())?;
    let (titles, mods) = manifests()?;
    let c = if desktop.root.join("configs/settings.json").exists() {
        desktop.load()?
    } else {
        let c = Config::new(
            &titles[0],
            default_workspace(),
            desktop
                .root
                .join("profiles/alola/saves")
                .to_string_lossy()
                .into_owned(),
        );
        desktop.save(&c)?;
        c
    };
    let t = titles
        .iter()
        .find(|t| t.id == c.title)
        .ok_or("Selected title missing")?;
    match std::env::args().nth(1).as_deref().unwrap_or("status") {
        "status" => {
            for (n, ok) in desktop.verify(&c, t) {
                println!("{n}: {}", if ok { "found" } else { "missing" });
            }
        }
        "plan" => println!(
            "{}",
            serde_json::to_string_pretty(&launch_plan(&c, t, &mods)?).map_err(|e| e.to_string())?
        ),
        "launch" => {
            let status = desktop
                .launch(&launch_plan(&c, t, &mods)?)?
                .wait()
                .map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(format!("Runtime exit: {status}"));
            }
        }
        "build" => {
            recomp::worker([
                c.workspace.clone(),
                join(&c.workspace, &t.input),
                join(&c.workspace, &t.build_manifest),
                managed_tool("3dsrecomp", std::path::Path::new(&c.workspace)).to_string_lossy().into_owned(),
                desktop.root.join("builds").join(&c.title).to_string_lossy().into_owned(),
            ].into_iter())?;
        }
        _ => return Err("Use status | plan | launch | build".into()),
    }
    Ok(())
}
