//! Typed CTR service directory, independent of the kernel/platform adapter.
//! Follows TriAevum CtrSrvService/CtrIpcRouter: only registered ports open.
use std::collections::{BTreeSet,VecDeque};
pub mod message;
#[derive(Debug,Clone)]pub enum Target{Port(String),Service{name:String,subhandle:u32}}
impl Target{
    pub fn port(name:String)->Self{Self::Port(name)}
    pub fn service(name:String,subhandle:u32)->Self{Self::Service{name,subhandle}}
    pub fn name(&self)->&str{match self{Self::Port(name)|Self::Service{name,..}=>name}}
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum DirectoryAction { Reply,Open(String),Unavailable(String),NotificationSemaphore }
#[derive(Default)]
pub struct ServiceDirectory {
    ports:BTreeSet<String>,subscriptions:BTreeSet<u32>,notifications:VecDeque<u32>,
    process:Option<u32>,pub calls:u64,pub unknown_ports:u64,
}
impl ServiceDirectory {
    pub fn register(&mut self,name:&str){assert!(!name.is_empty()&&name.len()<=8);self.ports.insert(name.into());}
    pub fn notify(&mut self,id:u32)->bool{
        if !self.subscriptions.contains(&id)||self.notifications.len()==16{return false;}
        self.notifications.push_back(id);true
    }
    pub fn dispatch(&mut self,w:&mut[u32])->Result<DirectoryAction,&'static str>{
        if w.len()<8{return Err("short service directory buffer");}self.calls+=1;
        let header=w[0];let cmd=header>>16;
        match header {
            0x00010002=>{if w[1]!=0x20{return Err("RegisterClient PID descriptor");}self.process=Some(w[2]);},
            0x00020000=>{if self.process.is_none(){return Err("unregistered directory client");}return Ok(DirectoryAction::NotificationSemaphore);},
            0x00050100|0x000e00c0=>{
                let n=w[3]as usize;if n==0||n>8{return Err("invalid service name length");}
                let mut bytes=[0u8;8];bytes[..4].copy_from_slice(&w[1].to_le_bytes());bytes[4..].copy_from_slice(&w[2].to_le_bytes());
                let name=std::str::from_utf8(&bytes[..n]).map_err(|_|"non-UTF8 service name")?;
                let exists=self.ports.contains(name);
                if cmd==5 {if self.process.is_none(){return Err("unregistered directory client");}
                    if exists{return Ok(DirectoryAction::Open(name.into()));}
                    self.unknown_ports+=1;w[0]=0x00050040;w[1]=0xd8806407; // SRV service-not-registered.
                    return Ok(DirectoryAction::Unavailable(name.into()));
                }
                w[0]=0x000e0080;w[1]=0;w[2]=u32::from(exists);return Ok(DirectoryAction::Reply);
            },
            0x00090040=>{self.subscriptions.insert(w[1]);},
            0x000a0040=>{self.subscriptions.remove(&w[1]);},
            0x000b0000=>{w[0]=0x000b0080;w[1]=0;w[2]=self.notifications.pop_front().ok_or("no pending notification")?;return Ok(DirectoryAction::Reply);},
            _=>return Err("unsupported service directory command"),
        }
        w[0]=(cmd<<16)|0x40;w[1]=0;Ok(DirectoryAction::Reply)
    }
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn registered_ports_only_and_notifications(){
        let mut s=ServiceDirectory::default();s.register("cfg:u");let mut w=[0u32;16];
        w[0]=0x10002;w[1]=0x20;assert_eq!(s.dispatch(&mut w),Ok(DirectoryAction::Reply));
        w[0]=0x50100;w[1]=u32::from_le_bytes(*b"cfg:");w[2]=b'u' as u32;w[3]=5;
        assert_eq!(s.dispatch(&mut w),Ok(DirectoryAction::Open("cfg:u".into())));
        w[0]=0x50100;w[1]=u32::from_le_bytes(*b"fake");w[2]=0;w[3]=4;
        assert_eq!(s.dispatch(&mut w),Ok(DirectoryAction::Unavailable("fake".into())));assert_eq!(w[1],0xd8806407);
        assert!(!s.notify(0x209));w[0]=0x90040;w[1]=0x209;s.dispatch(&mut w).unwrap();
        assert!(s.notify(0x209));w[0]=0xb0000;s.dispatch(&mut w).unwrap();assert_eq!(w[2],0x209);
        w[0]=0xb0000;assert!(s.dispatch(&mut w).is_err());
    }
}
