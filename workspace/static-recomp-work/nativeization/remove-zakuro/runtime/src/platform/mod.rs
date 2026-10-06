//! Platform output contracts: no CTR DSP state belongs here.
pub trait HostAudioSink{fn push_pcm(&self,samples:&[[i16;2]]);}
pub trait HostApplication{fn running(&self)->bool;fn frame(&mut self)->Result<bool,String>;}
/// Native owner of the application pump; presentation owns its own events.
pub fn run(application:&mut impl HostApplication,period:std::time::Duration)->Result<(),String>{
    let mut next=std::time::Instant::now();
    while application.running(){
        if !application.frame()?{break;}
        next+=period;
        let now=std::time::Instant::now();
        if next>now{std::thread::sleep(next-now);}else if now.duration_since(next)>period*4{next=now;}
    }
    Ok(())
}
#[cfg(feature="host-audio")]
pub mod audio;
pub mod window;
#[cfg(feature="host-gamepad")]
pub mod gamepad;
pub use window::HostWindow;
pub mod pica;
pub mod gpu;
#[cfg(feature="native-input")]
pub mod input;
