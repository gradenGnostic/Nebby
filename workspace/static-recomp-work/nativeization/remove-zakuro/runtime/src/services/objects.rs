//! Production native service objects mapped into the process handle namespace.
//! Donor object identity is retained across repeated IPC replies.
use super::router::{ServiceRouter,ServiceMemory};
use crate::{kernel::{KObject,Mutex,ResetType,ObjectId},scheduler::{NativeScheduler,ExecutionContext}};
use std::collections::HashMap;
pub struct ServiceObjects {
    pub router:ServiceRouter,
    mirrors:HashMap<u32,ObjectId>,
    y2r_event:Option<ObjectId>,
}
impl ServiceObjects {
    pub fn new(router:ServiceRouter)->Self{Self{router,mirrors:HashMap::new(),y2r_event:None}}
    pub fn dispatch<C:ExecutionContext,R,M:ServiceMemory>(&mut self,service:u32,words:&mut[u32;64],memory:&mut M,static_table:u32,kernel:&mut NativeScheduler<C,R>)->Result<(),String>{
        let command=(words[0]>>16)as u16;
        let result=self.router.dispatch(service,words,memory,static_table);
        if result<0{return Err(format!("native service {service} rejected command {command:04x}: {result}"));}
        if service==2||service==3 {
            let slots:&[usize]=match(service,command){(2,1)=>&[5],(2,2)=>&[3,4],(3,15)=>&[3],_=>&[]};
            for &slot in slots {
                let native=words[slot];let identity=self.router.object_identity(native)?;
                let object=if let Some(&object)=self.mirrors.get(&identity){object}else{
                    let metadata=self.router.object(native).map_err(|error|format!("native object {native:08x}: {error}"))?;
                    let object=match metadata[0]{
                        0=>{let(object,handle)=kernel.create_event(ResetType::OneShot,"native service event");if metadata[1]>0{kernel.signal_event(object);}kernel.objects.add_ref(object);kernel.handles.close(&mut kernel.objects,handle);object},
                        1=>{let object=kernel.objects.insert(KObject::Mutex(Mutex::new("native service lock")));kernel.objects.add_ref(object);object},
                        kind=>return Err(format!("unsupported native service object kind {kind}")),
                    };
                    self.mirrors.insert(identity,object);object
                };
                words[slot]=kernel.handles.create(&mut kernel.objects,object,"native service object");
                if service==3{self.y2r_event=Some(object);}
            }
        }
        if service==3&&words[1]==0 {
            if let Some(object)=self.y2r_event{
                match command{0x26=>kernel.signal_event(object),0x2b=>kernel.clear_event(object),_=>{}}
            }
        }
        let header=crate::ipc::message::Header(words[0]);
        if 1+header.normal_params()+header.translate_params()>64{return Err("native service reply exceeds IPC buffer".into());}
        Ok(())
    }
}
