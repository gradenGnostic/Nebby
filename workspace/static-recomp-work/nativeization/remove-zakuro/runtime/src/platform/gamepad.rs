//! Native host gamepads; CTR bit mapping matches the existing PC input contract.
use gilrs::{Axis,Button,EventType,Gilrs};
use super::window::HostInput;
const MAP:[(Button,u32);12]=[(Button::East,1),(Button::South,2),(Button::North,1024),(Button::West,2048),(Button::LeftTrigger,512),(Button::RightTrigger,256),(Button::Start,8),(Button::Select,4),(Button::DPadRight,16),(Button::DPadLeft,32),(Button::DPadUp,64),(Button::DPadDown,128)];
pub struct Gamepads{host:Option<Gilrs>,camera_recenter:std::cell::Cell<bool>}
impl Gamepads{
    /// Platform-neutral camera axes, separate from CTR gameplay buttons.
    /// Positive Y points upward; recenter is level-triggered for the consumer.
    pub fn camera_input(&self)->(f32,f32,bool){
        let Some(host)=self.host.as_ref()else{return (0.,0.,false);};
        let mut result=(0_f32,0_f32,self.camera_recenter.replace(false));
        for (_,pad)in host.gamepads(){
            let x=pad.value(Axis::RightStickX);let y=pad.value(Axis::RightStickY);
            if x.hypot(y)>result.0.hypot(result.1){result.0=x;result.1=y;}
            result.2|=pad.is_pressed(Button::RightThumb);
        }
        result
    }
    pub fn new()->Self{Self{host:Gilrs::new().inspect_err(|e|log::warn!("native gamepads unavailable: {e}")).ok(),camera_recenter:std::cell::Cell::new(false)}}
    pub fn sample(&mut self,input:&mut HostInput){
        let Some(host)=self.host.as_mut()else{return;};
        while let Some(event)=host.next_event(){if let EventType::ButtonPressed(button,_)=event.event{if button==Button::RightThumb{self.camera_recenter.set(true);}for &(bound,bit)in &MAP{if bound==button{input.pressed_buttons|=bit;}}}}
        for (_,pad)in host.gamepads(){
            for &(button,bit)in &MAP{if pad.is_pressed(button){input.buttons|=bit;}}
            let x=pad.value(Axis::LeftStickX);let y=pad.value(Axis::LeftStickY);
            if x.hypot(y)>0.15{input.circle_x=x.clamp(-1.,1.);input.circle_y=y.clamp(-1.,1.);}
            for (value,positive,negative)in [(pad.value(Axis::DPadX),16,32),(pad.value(Axis::DPadY),64,128)]{if value>0.5{input.buttons|=positive;}else if value< -0.5{input.buttons|=negative;}}
        }
        input.buttons|=input.pressed_buttons;
    }
}
