//! Production SDL/PICA host window lifecycle; no emulator shell dependency.
use std::{ffi::{c_char,c_void,CString},path::Path};
#[link(name="dl")]
unsafe extern "C"{fn dlopen(path:*const c_char,flags:i32)->*mut c_void;fn dlsym(handle:*mut c_void,name:*const c_char)->*mut c_void;fn dlclose(handle:*mut c_void)->i32;}
pub struct HostWindow {
    library:*mut c_void,running:unsafe extern "C" fn()->u32,
    shutdown:unsafe extern "C" fn()->u32,closed:bool,
    input:unsafe extern "C" fn(*mut HostInput),
    edges:unsafe extern "C" fn()->u64,
    mouse:Option<RelativeMouse>,
    remote_taps:std::cell::Cell<[u8;12]>,
}
/// SDL platform implementation; callers receive only deltas/capture status.
struct RelativeMouse {
    focus:unsafe extern "C" fn()->*mut c_void,
    set:unsafe extern "C" fn(i32)->i32,
    delta:unsafe extern "C" fn(*mut i32,*mut i32)->u32,
    captured:std::cell::Cell<bool>,
    keyboard:unsafe extern "C" fn(*mut i32)->*const u8,
    recenter_code:i32,
}
impl RelativeMouse {
    unsafe fn load(library:*mut c_void)->Option<Self>{
        let focus=unsafe{dlsym(library,c"SDL_GetKeyboardFocus".as_ptr())};
        let set=unsafe{dlsym(library,c"SDL_SetRelativeMouseMode".as_ptr())};
        let delta=unsafe{dlsym(library,c"SDL_GetRelativeMouseState".as_ptr())};
        let keyboard=unsafe{dlsym(library,c"SDL_GetKeyboardState".as_ptr())};
        let keycode=unsafe{dlsym(library,c"SDL_GetScancodeFromName".as_ptr())};
        if [focus,set,delta,keyboard,keycode].iter().any(|p|p.is_null()){return None;}
        let keycode:unsafe extern "C" fn(*const c_char)->i32=unsafe{std::mem::transmute(keycode)};
        let name=CString::new(std::env::var("NEBBY_MODERN_CAMERA_RECENTER_KEY").unwrap_or("R".into())).ok()?;
        Some(Self{focus:unsafe{std::mem::transmute(focus)},set:unsafe{std::mem::transmute(set)},delta:unsafe{std::mem::transmute(delta)},captured:std::cell::Cell::new(false),keyboard:unsafe{std::mem::transmute(keyboard)},recenter_code:unsafe{keycode(name.as_ptr())}})
    }
    fn sample(&self,wanted:bool)->(f32,f32,bool){
        let wanted=wanted&&!unsafe{(self.focus)()}.is_null();
        let changed=wanted!=self.captured.get();
        if changed {
            if unsafe{(self.set)(i32::from(wanted))}!=0{return (0.,0.,false);}
            self.captured.set(wanted);
        }
        let(mut x,mut y)=(0,0);unsafe{(self.delta)(&mut x,&mut y);}
        if wanted&&!changed{(x as f32,y as f32,true)}else{(0.,0.,wanted)}
    }
    fn recenter(&self)->bool {
        if !self.captured.get() || self.recenter_code<=0{return false;}
        let mut count=0;let keys=unsafe{(self.keyboard)(&mut count)};
        !keys.is_null() && self.recenter_code<count && unsafe{*keys.add(self.recenter_code as usize)!=0}
    }
}
#[repr(C)]
#[derive(Default,Clone,Copy)]
pub struct HostInput{pub buttons:u32,pub circle_x:f32,pub circle_y:f32,pub touch_x:i32,pub touch_y:i32,pub touch_down:u32,pub pressed_buttons:u32,pub pressed_circle:u32}
impl HostWindow {
    pub fn open(path:&Path)->Result<Self,String> {
        let path=CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_|"invalid host renderer path")?;
        let library=unsafe{dlopen(path.as_ptr(),2)};
        if library.is_null(){return Err("native renderer library unavailable".into());}
        let running=unsafe{dlsym(library,c"pokemon3ds_host_window_running".as_ptr())};
        let shutdown=unsafe{dlsym(library,c"pokemon3ds_host_shutdown".as_ptr())};
        let input=unsafe{dlsym(library,c"moon_single_screen_input".as_ptr())};
        let edges=unsafe{dlsym(library,c"moon_single_screen_press_edges".as_ptr())};
        if running.is_null()||shutdown.is_null()||input.is_null()||edges.is_null(){unsafe{dlclose(library);}return Err("native window lifecycle ABI missing".into());}
        Ok(Self{library,running:unsafe{std::mem::transmute(running)},shutdown:unsafe{std::mem::transmute(shutdown)},input:unsafe{std::mem::transmute(input)},edges:unsafe{std::mem::transmute(edges)},closed:false,mouse:unsafe{RelativeMouse::load(library)},remote_taps:std::cell::Cell::new([0;12])})
    }
    pub fn running(&self)->bool{!self.closed&&unsafe{(self.running)()!=0}}
    pub fn relative_mouse(&self,capture:bool)->(f32,f32,bool){self.mouse.as_ref().map_or((0.,0.,false),|mouse|mouse.sample(capture))}
    pub fn camera_recenter(&self)->bool{self.mouse.as_ref().is_some_and(RelativeMouse::recenter)}
    pub fn input(&self)->HostInput{let mut value=HostInput::default();unsafe{(self.input)(&mut value);let edges=(self.edges)();value.pressed_buttons=edges as u32;value.pressed_circle=(edges>>32)as u32;}
        if std::env::var_os("NEBBY_REMOTE_INPUT").is_some_and(|v|v=="1") {
            let mut ages=self.remote_taps.get();
            value.buttons=remote_tap_buttons(value.buttons,value.pressed_buttons,&mut ages);
            self.remote_taps.set(ages);
        }
        value}
    /// Call while guest resource callbacks and process memory are still alive.
    pub fn shutdown(&mut self)->Result<(),String> {
        if !self.closed {
            if let Some(mouse)=self.mouse.as_ref(){if mouse.captured.get(){mouse.sample(false);}}
            if unsafe{(self.shutdown)()==0}{return Err("native renderer teardown failed".into());}
            self.closed=true;
        }Ok(())
    }
}
impl Drop for HostWindow {
    fn drop(&mut self){if let Err(error)=self.shutdown(){log::error!("{error}");}unsafe{dlclose(self.library);}}
}
// Remote desktops can deliver down+up between guest samples. Retain a brief
// held level as well as the edge; some retail menus only sample held state.
fn remote_tap_buttons(mut held:u32,pressed:u32,ages:&mut [u8;12])->u32 {
    for (bit,age) in ages.iter_mut().enumerate() {
        if pressed&(1<<bit)!=0 {*age=3;}
        if *age>0 {held|=1<<bit;*age-=1;}
    }
    held
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn missing_window_library(){assert!(HostWindow::open(Path::new("/missing/native-window.so")).is_err());}
    #[test]fn remote_tap_is_bounded(){let mut ages=[0;12];assert_eq!(remote_tap_buttons(0,1,&mut ages),1);assert_eq!(remote_tap_buttons(0,0,&mut ages),1);assert_eq!(remote_tap_buttons(0,0,&mut ages),1);assert_eq!(remote_tap_buttons(0,0,&mut ages),0);}
}
