use eframe::egui::{self, Color32, RichText};
use nebby_core::*;
use nebby_desktop::*;
use std::{path::PathBuf, process::Child};
const LILAC: Color32 = Color32::from_rgb(192, 165, 243);
const GOLD: Color32 = Color32::from_rgb(240, 209, 139);
fn library_card(ui:&mut egui::Ui,title:&nebby_core::library::LibraryTitle,imported:bool,selected:bool,art:Option<&egui::TextureHandle>)->bool{
    let (rect,response)=ui.allocate_exact_size(egui::vec2(ui.available_width(),54.),egui::Sense::click());
    let background=if selected{Color32::from_rgb(43,35,77)}else if response.hovered(){Color32::from_rgb(30,30,51)}else{Color32::from_rgb(20,21,39)};
    let foreground=if imported{Color32::from_rgb(234,229,246)}else{Color32::from_rgb(151,150,173)};
    let p=ui.painter();p.rect_filled(rect,8.,background);
    if selected{p.rect_stroke(rect,8.,egui::Stroke::new(1.,Color32::from_rgb(132,109,220)),egui::StrokeKind::Inside);}
    let badge=egui::Rect::from_center_size(egui::pos2(rect.left()+23.,rect.center().y),egui::vec2(32.,36.));
    let badge_background=if imported{Color32::from_rgb(31,40,70)}else{Color32::from_rgb(34,34,47)};
    p.rect_filled(badge,5.,badge_background);
    let sun=title.name.contains("Sun");
    let accent=if !imported{Color32::from_rgb(124,124,141)}else if sun{GOLD}else{Color32::from_rgb(169,181,247)};
    p.circle_filled(badge.center(),10.,accent);
    if !sun{p.circle_filled(badge.center()+egui::vec2(-4.,-3.),8.5,badge_background);}
    if let Some(art)=art{p.image(art.id(),badge,egui::Rect::from_min_max(egui::pos2(0.,0.),egui::pos2(1.,1.)),Color32::WHITE);}
    let left=rect.left()+47.;
    p.text(egui::pos2(left,rect.top()+11.),egui::Align2::LEFT_TOP,&title.name,egui::FontId::proportional(12.),foreground);
    p.circle_filled(egui::pos2(left+3.,rect.top()+37.),2.5,if imported{Color32::from_rgb(124,207,185)}else{Color32::from_rgb(103,102,120)});
    p.text(egui::pos2(left+10.,rect.top()+30.),egui::Align2::LEFT_TOP,if imported{"Imported"}else{"Not imported"},egui::FontId::proportional(10.),foreground);
    response.on_hover_text(if imported{"Open game details"}else{"Select to import your own compatible game dump"}).clicked()
}
fn brand_pixels()->Vec<u8>{
    if let Ok(image)=image::open(nebby_desktop::app_root().join("assets/nebby.png")){
        return image.resize_exact(128,128,image::imageops::FilterType::Lanczos3).to_rgba8().into_raw();
    }
    let mut rgba=vec![0;128*128*4];
    for y in 0..128{for x in 0..128{
        let dx=x as f32-64.;let dy=y as f32-64.;
        let moon=dx*dx+dy*dy<43.*43. && (dx+18.)*(dx+18.)+(dy+12.)*(dy+12.)>37.*37.;
        let star=[(20,24),(101,16),(110,105),(24,99)].iter().any(|(sx,sy)|((x as i32-sx).abs()<=1&&(y as i32-sy).abs()<=4)||((x as i32-sx).abs()<=4&&(y as i32-sy).abs()<=1));
        let color=if moon{[173,171,249,255]}else if star{[125,225,243,255]}else{[0,0,0,0]};
        rgba[(y*128+x)*4..(y*128+x)*4+4].copy_from_slice(&color);
    }}rgba
}
fn cosmic_hero(ui:&mut egui::Ui,name:&str,subtitle:&str,art:Option<&egui::TextureHandle>){
    let (rect,_)=ui.allocate_exact_size(egui::vec2(ui.available_width(),200.),egui::Sense::hover());
    let p=ui.painter();
    p.rect_filled(rect,14.,Color32::from_rgb(17,23,51));
    if let Some(art)=art{p.image(art.id(),rect,egui::Rect::from_min_max(egui::pos2(0.,0.),egui::pos2(1.,1.)),Color32::from_rgb(100,100,120));}
    for i in 0..72{
        let x=rect.left()+((i*137+23)%997)as f32/997.*rect.width();
        let y=rect.top()+((i*67+11)%193)as f32/193.*rect.height();
        p.circle_filled(egui::pos2(x,y),if i%9==0{1.5}else{0.7},Color32::from_rgba_unmultiplied(130,168,235,if i%9==0{180}else{60}));
    }
    if art.is_none() && rect.width()>540.{
        let center=egui::pos2(rect.right()-98.,rect.top()+92.);
        p.circle_filled(center,61.,Color32::from_rgb(159,180,242));
        p.circle_filled(center+egui::vec2(-19.,-12.),52.,Color32::from_rgb(17,23,51));
    }
    let left=rect.left()+24.;
    p.text(egui::pos2(left,rect.top()+57.),egui::Align2::LEFT_TOP,name,egui::FontId::proportional(if rect.width()>700.{42.}else{30.}),Color32::from_rgb(232,237,255));
    p.text(egui::pos2(left,rect.top()+121.),egui::Align2::LEFT_TOP,subtitle,egui::FontId::proportional(14.),Color32::from_rgb(135,213,230));
}
struct Nebby {
    desktop: Desktop,
    titles: Vec<Title>,
    mods: Vec<ModDescriptor>,
    config: Config,
    icon: egui::TextureHandle,
    artwork: std::collections::BTreeMap<String,egui::TextureHandle>,
    status: String,
    job: Option<(Child, String)>,
    dirty: bool,
    library: nebby_core::library::Library,
    selected: String,
    log_open: bool,
    log_category: String,
    settings_open: bool,
    import_open: bool,
    import_path: String,
}
impl Nebby {
    fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let mut v = egui::Visuals::dark();
        v.panel_fill = Color32::from_rgb(18, 18, 36);
        v.window_fill = Color32::from_rgb(31, 27, 51);
        v.override_text_color = Some(Color32::from_rgb(234, 229, 246));
        v.widgets.inactive.bg_fill = Color32::from_rgb(43, 36, 66);
        v.widgets.hovered.bg_fill = Color32::from_rgb(68, 49, 98);
        v.selection.bg_fill = Color32::from_rgb(88, 62, 128);
        cc.egui_ctx.set_visuals(v);
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.spacing.item_spacing = egui::vec2(12., 10.);
        style.spacing.button_padding = egui::vec2(18., 10.);
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);
        let desktop = Desktop::new(default_root())?;
        let (mut titles, mods) = manifests()?;
        let config = if desktop.root.join("configs/settings.json").exists() {
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
        let mut library=desktop.load_library()?;
        for title in &mut titles {
            if let Some(game)=library.games.get(&title.id){title.input=game.content_path.clone();}
            else if let Ok(game)=nebby_desktop::import::inspect(std::path::Path::new(&join(&config.workspace,&title.input))){library.games.insert(game.title_id.clone(),game);}
        }
        desktop.save_library(&library)?;
        let selected=config.title.clone();
        let image=brand_pixels();
        let size=[128,128];
        let icon = cc.egui_ctx.load_texture(
            "Nebby",
            egui::ColorImage::from_rgba_unmultiplied(size, &image),
            egui::TextureOptions::LINEAR,
        );
        let mut artwork=std::collections::BTreeMap::new();
        for title in nebby_core::library::catalog(){for kind in ["heroes","grids"]{for ext in ["png","jpg"]{
            let key=format!("{}-{kind}",title.title_id);
            if let Ok(image)=image::open(nebby_desktop::app_root().join("assets/steamgriddb").join(format!("{key}.{ext}"))){
                let image=image.resize(1920,900,image::imageops::FilterType::Lanczos3);
                for dim in [false,true]{
                    let rgba=if dim{image.grayscale().to_rgba8()}else{image.to_rgba8()};
                    let name=if dim{format!("{key}-dim")}else{key.clone()};
                    let size=[rgba.width() as usize,rgba.height() as usize];
                    let texture=cc.egui_ctx.load_texture(&name,egui::ColorImage::from_rgba_unmultiplied(size,rgba.as_raw()),egui::TextureOptions::LINEAR);
                    artwork.insert(name,texture);
                }
                break;
            }
        }}}
        Ok(Self {
            desktop,
            titles,
            mods,
            config,
            icon,
            artwork,
            status: String::new(),
            job: None,
            dirty: false,
            library,
            selected,
            log_open:false,
            log_category:"launch".into(),
            settings_open:false,
            import_open:false,
            import_path:String::new(),
        })
    }
    fn title(&self) -> &Title {
        self.titles
            .iter()
            .find(|t| t.id == self.config.title)
            .unwrap_or(&self.titles[0])
    }
    fn import_game(&mut self){
        self.import_open=true;
    }
    fn import_dump(&mut self,path:&std::path::Path)->bool{
            match nebby_desktop::import::inspect(path){
                Ok(game)=>{
                    self.selected=game.title_id.clone();
                    if let Some(title)=self.titles.iter_mut().find(|t|t.id==game.title_id){title.input=game.content_path.clone();self.config.title=title.id.clone();self.dirty=true;}
                    self.library.games.insert(game.title_id.clone(),game);
                    self.status=match self.desktop.save_library(&self.library){Ok(())=>"Game imported.".into(),Err(e)=>e};
                    self.status=="Game imported."
                },Err(error)=>{self.status=error;false},
            }
    }
    fn start(&mut self, build: bool) {
        if self.job.is_some() {
            return;
        }
        let result = if build {
            let plan=CommandPlan{
                executable:std::env::current_exe().unwrap().to_string_lossy().into_owned(),
                arguments:vec!["--build-worker".into(),self.config.workspace.clone(),join(&self.config.workspace,&self.title().input),join(&self.config.workspace,&self.title().build_manifest),managed_tool("3dsrecomp",std::path::Path::new(&self.config.workspace)).to_string_lossy().into_owned(),self.desktop.root.join("builds").join(&self.config.title).to_string_lossy().into_owned()],
                environment:Default::default(),unset_environment:vec![],working_directory:self.config.workspace.clone(),
            };
            self.desktop.build(&plan)
        } else {
            launch_plan(&self.config, self.title(), &self.mods)
                .and_then(|p| self.desktop.launch(&p))
        };
        match result {
            Ok(child) => {
                let name = if build { "Build" } else { "Game" }.to_string();
                self.status = format!(
                    "{name} running • logs in {}",
                    self.desktop.root.join("logs").display()
                );
                self.job = Some((child, name));
            }
            Err(e) => self.status = e,
        }
    }
    fn folder(&mut self, p: PathBuf) {
        if let Err(e) = self.desktop.open_folder(&p) {
            self.status = e;
        }
    }
}
impl eframe::App for Nebby {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some((child, name)) = &mut self.job {
            match child.try_wait() {
                Ok(Some(s)) => {
                    self.status = if s.success(){format!("{name} finished")}else{format!("{name} failed ({s}) · View log")};
                    self.job = None;
                }
                Err(e) => {
                    self.status = e.to_string();
                    self.job = None;
                }
                _ => {
                    if name=="Build"{
                        let text=log_tail(&self.desktop.root.join("logs/build.log"));
                        if let Some(stage)=text.lines().rev().find_map(|line|line.strip_prefix("STAGE ")){self.status=format!("Build · {stage}");}
                    }
                }
            }
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
        if self.log_open{
            egui::Window::new("Logs").open(&mut self.log_open).default_size([800.,450.]).show(ui.ctx(),|ui|{
                ui.horizontal(|ui|{for category in ["launch","build"]{ui.selectable_value(&mut self.log_category,category.into(),if category=="launch"{"Runtime / Zakuro"}else{"Recomp / Build"});}});
                let path=self.desktop.root.join("logs").join(format!("{}.log",self.log_category));
                let text=log_tail(&path);
                egui::ScrollArea::both().stick_to_bottom(true).show(ui,|ui|{ui.label(RichText::new(text).monospace());});
            });
        }
        egui::Panel::bottom("status")
            .resizable(false)
            .show(ui, |ui| {
                if ui.button("View log").clicked(){self.log_open=true;}
                if !self.status.is_empty(){ui.label(RichText::new(&self.status).color(GOLD));}
            });
        egui::Panel::left("moon-sidebar")
            .resizable(false)
            .exact_size(215.)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing=egui::vec2(8.,5.);
                ui.spacing_mut().button_padding=egui::vec2(10.,6.);
                egui::ScrollArea::vertical().id_salt("library-sidebar-scroll").max_height((ui.available_height()-42.).max(100.)).show(ui,|ui|{
                ui.add_space(12.);
                ui.vertical_centered(|ui| {
                    ui.image((self.icon.id(), egui::vec2(76., 72.)));
                    ui.label(RichText::new("NEBBY").size(32.).color(LILAC).strong());
                });
                ui.add_space(14.);
                ui.separator();
                ui.label(RichText::new("LIBRARY").size(11.).color(LILAC));
                for backend in [nebby_core::library::RuntimeBackend::NativeRecomp,nebby_core::library::RuntimeBackend::Zakuro]{
                    ui.add_space(12.);
                    ui.label(RichText::new(if backend==nebby_core::library::RuntimeBackend::NativeRecomp{"NATIVE"}else{"EXPERIMENTAL · ZAKURO ONLY"}).size(11.).color(LILAC));
                    for t in nebby_core::library::catalog().into_iter().filter(|t|t.backend==backend){
                        let imported=self.library.games.contains_key(&t.title_id);
                        let key=format!("{}-grids{}",t.title_id,if imported{""}else{"-dim"});
                        if library_card(ui,&t,imported,self.selected==t.title_id,self.artwork.get(&key)){
                            self.selected=t.title_id.clone();
                            if let Some(native)=self.titles.iter().find(|n|n.id==t.title_id){self.config.title=native.id.clone();self.config.features=native.features.iter().map(|f|(f.id.clone(),f.default)).collect();self.dirty=true;}
                        }
                    }
                }
                });
                ui.separator();
                if ui.button("⚙ Settings").clicked(){self.settings_open=true;}
            });
        egui::CentralPanel::default().show(ui, |ui| {
            let selected=nebby_core::library::catalog().into_iter().find(|t|t.title_id==self.selected).unwrap();
            if selected.backend==nebby_core::library::RuntimeBackend::Zakuro{
                ui.add_space(26.);
                cosmic_hero(ui,&selected.name,"EXPERIMENTAL · ZAKURO ONLY",self.artwork.get(&format!("{}-heroes{}",selected.title_id,if self.library.games.contains_key(&selected.title_id){""}else{"-dim"})));
                ui.add_space(16.);
                ui.label("Uses Zakuro.");
                if ui.add_enabled(self.job.is_none(),egui::Button::new("Import game")).clicked(){self.import_game();}
                if let Some(game)=self.library.games.get(&self.selected).cloned(){
                    ui.label(format!("{} / {}",game.region,game.revision));
                    let runtime=managed_tool("zakuro",std::path::Path::new(&self.config.workspace)).to_string_lossy().into_owned();
                    let ready=std::path::Path::new(&runtime).is_file();
                    if ui.add_enabled(ready&&self.job.is_none(),egui::Button::new("Play experimentally")).clicked(){
                        let plan=CommandPlan{executable:runtime,arguments:vec![game.content_path],environment:Default::default(),unset_environment:vec![],working_directory:self.desktop.root.to_string_lossy().into_owned()};
                        match self.desktop.launch(&plan){Ok(child)=>self.job=Some((child,"Zakuro".into())),Err(e)=>self.status=e}
                    }
                    if !ready{ui.label("MISSING · Managed Zakuro runtime");}
                }
                return;
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(18.);
                let title=self.title();
                let status=if title.id=="000400000011C500"{"NATIVE PREVIEW"}else{"NATIVE"};
                let subtitle=format!("{} / {} · {status}",title.region,title.revision);
                cosmic_hero(ui,&title.name,&subtitle,self.artwork.get(&format!("{}-heroes",title.id)));
                ui.add_space(12.);
                if ui.add_enabled(self.job.is_none(),egui::Button::new("Import game")).clicked(){self.import_game();}
                ui.separator();
                ui.columns(2, |cols| {
                    egui::Frame::group(cols[1].style())
                        .corner_radius(12)
                        .fill(Color32::from_rgb(27, 25, 46))
                        .inner_margin(16.)
                        .show(&mut cols[1], |ui| {
                            ui.heading("Graphics & display");
                            egui::ComboBox::from_id_salt("render_resolution")
                                .selected_text(format!("Render resolution: {}×",self.config.graphics.render_scale))
                                .show_ui(ui,|ui|{
                                    for scale in 1..=4{
                                        if ui.selectable_value(&mut self.config.graphics.render_scale,scale,format!("{scale}× native")).changed(){self.dirty=true;}
                                    }
                                });
                            egui::ComboBox::from_id_salt("native_window_resolution")
                                .selected_text(format!("Window: {}×{}",self.config.graphics.window_width,self.config.graphics.window_height))
                                .show_ui(ui,|ui|{
                                    for (w,h) in [(960,540),(1280,720),(1600,900),(1920,1080)]{
                                        let selected=self.config.graphics.window_width==w&&self.config.graphics.window_height==h;
                                        if ui.selectable_label(selected,format!("{w}×{h}")).clicked(){self.config.graphics.window_width=w;self.config.graphics.window_height=h;self.dirty=true;}
                                    }
                                });
                            ui.small("Applies on next launch.");
                            ui.add_space(10.);
                            ui.label(RichText::new("Runtime · Native").color(LILAC));
                        });
                    egui::Frame::group(cols[0].style())
                        .corner_radius(12)
                        .fill(Color32::from_rgb(27, 25, 46))
                        .inner_margin(16.)
                        .show(&mut cols[0], |ui| {
                            ui.heading("Play & build");
                            let build_state=if self.job.as_ref().is_some_and(|(_,name)|name=="Build"){nebby_core::library::BuildState::Building}else{self.desktop.native_build_state(&self.config,self.title())};
                            ui.label(RichText::new(format!("Build: {}",build_state.label())).color(LILAC));
                            let ready=build_state==nebby_core::library::BuildState::Ready&&self.library.games.contains_key(&self.config.title)&&self.desktop.verify(&self.config,self.title()).iter().all(|(_,ok)|*ok);
                            if ui.add_enabled(ready&&self.job.is_none(),egui::Button::new(RichText::new("▶  Play").size(18.)).fill(Color32::from_rgb(90,66,211))).clicked(){self.start(false);}
                            if ui.add_enabled(self.job.is_none()&&self.library.games.contains_key(&self.config.title)&&std::env::var("NEBBY_PORTABLE").as_deref()!=Ok("1"),egui::Button::new("Build / Update Recomp")).clicked(){self.start(true);}
                            ui.add_space(12.);
                            egui::CollapsingHeader::new("Mods").show(ui,|ui|{
                            if ui.button("Import mod descriptor…").clicked() {
                                if let Some(p) = rfd::FileDialog::new()
                                    .add_filter("Nebby mod", &["json"])
                                    .pick_file()
                                {
                                    match self.desktop.install_mod(&p) {
                                        Ok(m) => {
                                            self.mods.retain(|old| old.id != m.id);
                                            self.status =
                                                format!("Installed {} • enable it above", m.name);
                                            self.mods.push(m);
                                        }
                                        Err(e) => self.status = e,
                                    }
                                }
                            }
                            ui.label("Only installed patches can be enabled.");
                            for m in &self.mods {
                                if !m.titles.contains(&self.config.title) {
                                    continue;
                                }
                                let mut enabled =
                                    self.config.mods.get(&m.id).copied().unwrap_or(false);
                                if ui
                                    .add_enabled(
                                        m.available,
                                        egui::Checkbox::new(&mut enabled, &m.name),
                                    )
                                    .changed()
                                {
                                    self.config.mods.insert(m.id.clone(), enabled);
                                    self.dirty = true;
                                }
                                ui.label(
                                    RichText::new(&m.description)
                                        .size(11.)
                                        .color(Color32::from_rgb(159, 153, 180)),
                                );
                                if m.id=="modern-camera" && enabled {
                                    let key=nebby_core::modern_camera::profile_key(&self.config.title,&self.config.profile);
                                    let settings=self.config.modern_camera.entry(key).or_default();
                                    let mut changed=false;
                                    ui.add_enabled_ui(self.job.is_none(),|ui|{
                                        for (label,value,min,max) in [
                                            ("Mouse sensitivity",&mut settings.mouse_sensitivity,0.05,5.),
                                            ("Controller sensitivity",&mut settings.controller_sensitivity,0.05,5.),
                                            ("Distance",&mut settings.distance,80.,1200.),
                                            ("Height",&mut settings.height,-100.,300.),
                                            ("FOV",&mut settings.fov_degrees,25.,85.),
                                            ("Smoothing",&mut settings.smoothing,0.,0.5)] {
                                            changed|=ui.add(egui::Slider::new(value,min..=max).text(label)).changed();
                                        }
                                        changed|=ui.checkbox(&mut settings.invert_y,"Invert Y").changed();
                                        changed|=ui.checkbox(&mut settings.camera_relative,"Camera-relative movement").changed();
                                        changed|=ui.checkbox(&mut settings.collision,"Camera collision").changed();
                                        egui::ComboBox::from_id_salt("modern-camera-recenter").selected_text(format!("Recenter: {} / R3",settings.recenter_key)).show_ui(ui,|ui|{
                                            for key in ["R","T","F","Space","F7","F8"] {changed|=ui.selectable_value(&mut settings.recenter_key,key.into(),key).changed();}
                                        });
                                        ui.label("Changes apply next launch.");
                                    });
                                    self.dirty|=changed;
                                }
                            }
                            });
                        });
                });
                ui.add_space(14.);
                ui.add_space(12.);
                egui::CollapsingHeader::new("Keyboard controls — selected game").show(ui,|ui|{
                    let title_id=self.config.title.clone();
                    let bindings=self.config.keybinds.entry(title_id.clone()).or_insert_with(default_keybinds);
                    ui.add_enabled_ui(self.job.is_none(),|ui|{
                        egui::Grid::new("game-keybind-grid").num_columns(2).show(ui,|ui|{
                            for(action,key)in bindings.iter_mut(){
                                ui.label(action.replace('_'," "));
                                egui::ComboBox::from_id_salt((&title_id,action)).selected_text(key.as_str()).show_ui(ui,|ui|{for candidate in keyboard_keys(){if ui.selectable_value(key,(*candidate).into(),*candidate).changed(){self.dirty=true;}}});
                                ui.end_row();
                            }
                        });
                        if ui.button("Reset keyboard defaults").clicked(){*bindings=default_keybinds();self.dirty=true;}
                    });
                });
                egui::Frame::group(ui.style())
                    .corner_radius(12)
                    .fill(Color32::from_rgb(27, 25, 46))
                    .inner_margin(14.)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.heading("Status");
                        for (n, ok) in self.desktop.verify(&self.config, self.title()) {
                            ui.label(
                                RichText::new(format!(
                                    "{}  {n}",
                                    if ok { "READY" } else { "MISSING" }
                                ))
                                .color(if ok {
                                    Color32::from_rgb(132, 207, 193)
                                } else {
                                    GOLD
                                }),
                            );
                        }
                        ui.add_space(6.);
                        ui.horizontal(|ui| {
                            if ui.button("Builds").clicked() {
                                self.folder(PathBuf::from(join(
                                    &self.config.workspace,
                                    "static-recomp-work/native-renderer/moon-target/release",
                                )));
                            }
                            if ui.button("Saves").clicked() {
                                self.folder(PathBuf::from(&self.config.save_root));
                            }
                            if ui.button("Mods").clicked() {
                                self.folder(self.desktop.root.join("mods"));
                            }
                        });
                    });
            });
        });
        let mut import_open=self.import_open;
        let mut imported=false;
        egui::Window::new("Import game").open(&mut import_open).default_width(560.).show(ui.ctx(),|ui|{
            ui.label("Choose your own decrypted CXI / 3DS / CCI dump.");
            ui.small("Native Moon requires EUR base v1.0. Games are never downloaded or copied.");
            ui.label("Game file path");
            ui.add(egui::TextEdit::singleline(&mut self.import_path).desired_width(f32::INFINITY));
            ui.horizontal(|ui|{
                if ui.button("Browse…").clicked(){
                    if let Some(path)=rfd::FileDialog::new().add_filter("Decrypted 3DS games",&["cxi","3ds","cci"]).pick_file(){self.import_path=path.to_string_lossy().into_owned();}
                }
                if ui.add_enabled(!self.import_path.trim().is_empty(),egui::Button::new("Import")).clicked(){
                    imported=self.import_dump(&PathBuf::from(self.import_path.trim()));
                }
            });
            ui.small("You can paste a path if your desktop file picker is unavailable.");
        });
        self.import_open=import_open&&!imported;
        egui::Window::new("Settings").open(&mut self.settings_open).default_width(560.).show(ui.ctx(),|ui|{
            ui.label("Profile");
            self.dirty |= ui.text_edit_singleline(&mut self.config.profile).changed();
            ui.add_space(8.);
            ui.label("Native SDK / build workspace");
            ui.horizontal(|ui|{
                self.dirty |= ui.text_edit_singleline(&mut self.config.workspace).changed();
                if ui.button("Browse…").clicked(){
                    if let Some(path)=rfd::FileDialog::new().pick_folder(){
                        self.config.workspace=path.to_string_lossy().into_owned();self.dirty=true;
                    }
                }
            });
            ui.label("Save directory · existing saves are never imported automatically");
            ui.horizontal(|ui|{
                self.dirty |= ui.text_edit_singleline(&mut self.config.save_root).changed();
                if ui.button("Choose…").clicked(){
                    if let Some(path)=rfd::FileDialog::new().pick_folder(){
                        self.config.save_root=path.to_string_lossy().into_owned();self.dirty=true;
                    }
                }
            });
            ui.add_space(8.);
            ui.label(format!("Library and launcher storage: {}",self.desktop.root.display()));
            ui.label("Native build cache is stored in the selected SDK workspace.");
        });
        if self.dirty {
            match self.desktop.save(&self.config) {
                Ok(()) => {
                    self.dirty = false;
                }
                Err(e) => self.status = format!("Settings not saved: {e}"),
            }
        }
    }
}
fn main() -> eframe::Result {
    let mut args=std::env::args().skip(1);
    if args.next().as_deref()==Some("--build-worker"){
        if let Err(error)=nebby_desktop::recomp::worker(args){eprintln!("BUILD_ERROR: {error}");std::process::exit(1);}
        return Ok(());
    }
    let icon=egui::IconData{rgba:brand_pixels(),width:128,height:128};
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Nebby")
            .with_icon(icon)
            .with_inner_size([1280., 720.])
            .with_min_inner_size([850., 650.]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Nebby",
        options,
        Box::new(|cc| Ok(Box::new(Nebby::new(cc).map_err(std::io::Error::other)?))),
    )
}
