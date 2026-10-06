//! Local-only native CTR service profiles. No emulator dispatcher dependency.
pub mod router;
pub mod hid;
pub mod dsp;
pub mod gsp;
pub mod filesystem;
pub mod fs_ipc;
pub mod directory;
pub mod objects;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum ServiceKind{Directory,BackgroundStorage,Apt,Gsp,Hid,Filesystem,Config,Ndm,Dsp,Loader,Y2r,Friends,Ssl,Uds}
pub fn service_kind(name:&str)->Option<ServiceKind>{Some(match name{
    "srv:"|"srv:pm"=>ServiceKind::Directory,"boss:U"=>ServiceKind::BackgroundStorage,
    "APT:U"|"APT:A"|"APT:S"=>ServiceKind::Apt,"gsp::Gpu"=>ServiceKind::Gsp,
    "hid:USER"|"hid:SPVR"=>ServiceKind::Hid,"fs:USER"|"FSFile"|"FSDirectory"=>ServiceKind::Filesystem,
    "cfg:u"|"cfg:s"|"cfg:i"=>ServiceKind::Config,"ndm:u"=>ServiceKind::Ndm,"dsp::DSP"=>ServiceKind::Dsp,
    "ldr:ro"=>ServiceKind::Loader,"y2r:u"=>ServiceKind::Y2r,"frd:u"=>ServiceKind::Friends,
    "ssl:C"=>ServiceKind::Ssl,"nwm::UDS"=>ServiceKind::Uds,_=>return None,
})}
#[derive(Default)]
pub struct BackgroundStorage {
    initialized:bool,pub program_id:u64,pub client_pid:u32,pub calls:u64,
}
impl BackgroundStorage {
    pub fn dispatch(&mut self,words:&mut[u32])->Result<(),&'static str>{
        if words.len()<6{return Err("short BOSS command");}self.calls+=1;
        match words[0]{
            0x00010082=>{
                if words[3]!=0x20{return Err("BOSS PID descriptor");}
                self.program_id=words[1]as u64|((words[2]as u64)<<32);self.client_pid=words[4];
                self.initialized=true;words[0]=0x00010040;words[1]=0;
            },
            0x00300000=>{
                if !self.initialized{return Err("BOSS session not initialized");}
                // Native offline profile has no registered SpotPass storage.
                // Do NOT invent a successful storage-info record. This is a
                // documented native policy error, not a firmware-exact code:
                // MAKERESULT(RL_STATUS,RS_NOTFOUND,RM_BOSS,RD_NO_DATA).
                words[0]=0x00300040;words[1]=(25<<27)|(4<<21)|(62<<10)|0x3ef;
            },
            _=>return Err("unsupported BOSS command"),
        }Ok(())
    }
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn no_fabricated_storage(){let mut s=BackgroundStorage::default();let mut w=[0u32;16];
        w[0]=0x300000;assert!(s.dispatch(&mut w).is_err());
        w[0]=0x10082;w[3]=0x20;s.dispatch(&mut w).unwrap();
        w[0]=0x300000;s.dispatch(&mut w).unwrap();assert_ne!(w[1]&0x80000000,0);assert_eq!((w[1]>>10)&255,62);
    }
}
