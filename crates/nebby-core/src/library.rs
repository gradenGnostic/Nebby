//! Title-neutral library state and explicit backend policy.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeBackend { NativeRecomp, Zakuro }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryTitle {
    pub title_id: String,
    pub name: String,
    pub backend: RuntimeBackend,
    pub compatible_region: Option<String>,
    pub compatible_revision: Option<String>,
    #[serde(default)]
    pub native_code_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportedGame {
    pub title_id: String,
    pub content_path: String,
    pub region: String,
    pub revision: String,
    pub code_sha256: String,
    pub last_played: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Library { pub games: BTreeMap<String, ImportedGame> }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildState {
    NotImported, NeedsBuild, Building, Ready, Outdated, Failed(String),
}
impl BuildState {
    pub fn label(&self)->&'static str{
        match self{
            Self::NotImported=>"Not imported",Self::NeedsBuild=>"Build required",
            Self::Building=>"Building",Self::Ready=>"Ready to play",
            Self::Outdated=>"Update required",Self::Failed(_)=>"Build failed · View log",
        }
    }
}

pub fn catalog() -> Vec<LibraryTitle> {
    [
        ("0004000000175E00", "Pokémon Moon", RuntimeBackend::NativeRecomp),
        ("0004000000164800", "Pokémon Sun", RuntimeBackend::Zakuro),
        ("00040000001B5000", "Pokémon Ultra Sun", RuntimeBackend::Zakuro),
        ("00040000001B5100", "Pokémon Ultra Moon", RuntimeBackend::Zakuro),
        ("0004000000055D00", "Pokémon X", RuntimeBackend::Zakuro),
        ("0004000000055E00", "Pokémon Y", RuntimeBackend::Zakuro),
        ("000400000011C400", "Pokémon Omega Ruby", RuntimeBackend::Zakuro),
        ("000400000011C500", "Pokémon Alpha Sapphire", RuntimeBackend::NativeRecomp),
    ].into_iter().map(|(id,name,backend)|LibraryTitle {
        title_id:id.into(), name:name.into(), backend,
        compatible_region:(backend==RuntimeBackend::NativeRecomp).then(||"EUR".into()),
        compatible_revision:(backend==RuntimeBackend::NativeRecomp).then(||if id=="000400000011C500"{"Rev 2".into()}else{"1.0".into()}),
        native_code_sha256:(backend==RuntimeBackend::NativeRecomp).then(||if id=="000400000011C500"{"b7f9ce60361f3709ed0ce879658afe10a712f7db060640fa72bba826301b7c16".into()}else{"fbe0ce6da21542542f49645fff78ba1b7e5e7cc172ce4daceeb5c26ab54adba1".into()}),
    }).collect()
}

impl LibraryTitle {
    pub fn validate_import(&self, game:&ImportedGame)->Result<(),String> {
        if game.title_id!=self.title_id { return Err("Game dump does not match selected title".into()); }
        if self.compatible_region.as_ref().is_some_and(|r|r!=&game.region) ||
           self.compatible_revision.as_ref().is_some_and(|r|r!=&game.revision) {
            return Err("Unsupported region or revision for native runtime".into());
        }
        if self.native_code_sha256.as_ref().is_some_and(|hash|hash!=&game.code_sha256){return Err("Unsupported native executable hash".into());}
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn moon_and_alpha_are_native() {
        let titles=catalog();
        assert_eq!(titles.len(),8);
        assert_eq!(titles.iter().filter(|t|t.backend==RuntimeBackend::NativeRecomp).count(),2);
        assert_eq!(titles[0].name,"Pokémon Moon");
    }
    #[test] fn native_import_rejects_unsupported_revision() {
        let moon=&catalog()[0];
        let mut game=ImportedGame{title_id:moon.title_id.clone(),content_path:"game.cxi".into(),region:"EUR".into(),revision:"1.0".into(),code_sha256:moon.native_code_sha256.clone().unwrap(),last_played:None};
        assert!(moon.validate_import(&game).is_ok());
        game.revision="1.2".into();
        assert!(moon.validate_import(&game).is_err());
    }
    #[test] fn import_state_roundtrips() {
        let mut library=Library::default();
        let game=ImportedGame{title_id:catalog()[1].title_id.clone(),content_path:"user-game.cxi".into(),region:"unknown".into(),revision:"unknown".into(),code_sha256:"digest".into(),last_played:None};
        library.games.insert(game.title_id.clone(),game);
        let restored:Library=serde_json::from_str(&serde_json::to_string(&library).unwrap()).unwrap();
        assert_eq!(restored.games.len(),1);
    }
    #[test] fn matching_label_does_not_bypass_code_identity(){
        let moon=&catalog()[0];
        let game=ImportedGame{title_id:moon.title_id.clone(),content_path:"renamed-game.cxi".into(),region:"EUR".into(),revision:"1.0".into(),code_sha256:"wrong executable".into(),last_played:None};
        assert!(moon.validate_import(&game).unwrap_err().contains("hash"));
    }
}
