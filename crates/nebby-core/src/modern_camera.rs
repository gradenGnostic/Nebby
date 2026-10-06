//! Per-title/profile preferences; no game addresses or renderer dependencies.
use serde::{Serialize,Deserialize};
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(default)]
pub struct Settings {
    pub mouse_sensitivity:f32,
    pub controller_sensitivity:f32,
    pub invert_y:bool,
    pub distance:f32,
    pub height:f32,
    pub fov_degrees:f32,
    pub smoothing:f32,
    pub collision:bool,
    pub camera_relative:bool,
    pub recenter_key:String,
}
impl Default for Settings {
    fn default()->Self{Self{mouse_sensitivity:1.,controller_sensitivity:1.,invert_y:false,distance:600.,height:0.,fov_degrees:45.,smoothing:0.08,collision:true,camera_relative:true,recenter_key:"R".into()}}
}
impl Settings {
    pub fn validate(&self)->Result<(),String>{
        if !["R","T","F","Space","F7","F8"].contains(&self.recenter_key.as_str()) {return Err("Invalid camera recenter key".into());}
        for (name,value,min,max) in [
            ("mouse sensitivity",self.mouse_sensitivity,0.05,5.),
            ("controller sensitivity",self.controller_sensitivity,0.05,5.),
            ("distance",self.distance,80.,1200.),("height",self.height,-100.,300.),
            ("FOV",self.fov_degrees,25.,85.),("smoothing",self.smoothing,0.,0.5)] {
            if !value.is_finite() || value<min || value>max {return Err(format!("Invalid Modern Camera {name}"));}
        }
        Ok(())
    }
    pub fn environment(&self)->Result<Vec<(String,String)>,String>{
        self.validate()?;
        Ok(vec![
            ("MOUSE",self.mouse_sensitivity.to_string()),("STICK",self.controller_sensitivity.to_string()),
            ("INVERT_Y",u8::from(self.invert_y).to_string()),("DISTANCE",self.distance.to_string()),
            ("HEIGHT",self.height.to_string()),("FOV",self.fov_degrees.to_string()),
            ("SMOOTHING",self.smoothing.to_string()),("COLLISION",u8::from(self.collision).to_string()),
            ("RELATIVE",u8::from(self.camera_relative).to_string()),("RECENTER_KEY",self.recenter_key.clone()),
        ].into_iter().map(|(key,value)|(format!("NEBBY_MODERN_CAMERA_{key}"),value)).collect())
    }
}
pub fn profile_key(title:&str,profile:&str)->String{format!("{title}/{profile}")}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn settings_roundtrip(){let s=Settings::default();let json=serde_json::to_string(&s).unwrap();let loaded:Settings=serde_json::from_str(&json).unwrap();assert_eq!(s.environment().unwrap(),loaded.environment().unwrap());}
    #[test]fn rejects_nonfinite(){let mut s=Settings::default();s.distance=f32::NAN;assert!(s.validate().is_err());}
}
