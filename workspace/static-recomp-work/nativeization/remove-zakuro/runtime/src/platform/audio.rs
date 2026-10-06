//! Native host PCM output. CTR mixing stays in the native DSP producer.
use std::{collections::VecDeque,sync::{Arc,Mutex},cell::Cell};
use cpal::{traits::{HostTrait,DeviceTrait,StreamTrait},SampleFormat,SizedSample,FromSample};
// Measured native producer stalls exceeded the former 94ms prebuffer/375ms
// capacity. A bounded 250ms recovery reservoir tolerates frame/upload bursts;
// one second remains the hard limit. Long shader-compilation stalls still
// count genuine underruns instead of fabricating PCM or changing guest time.
const RESUME:usize=8192;
const CAPACITY:usize=32768;
#[derive(Default)]
struct ProducerStats{last:Option<std::time::Instant>,max_gap_us:u64,frames:u64,nonzero:u64}
struct Queue{samples:VecDeque<[f32;2]>,phase:f64,step:f64,starved:bool,last:[f32;2],underruns:u64,total:u64,dropped:u64,producer:ProducerStats}
impl Queue{
    fn next(&mut self)->[f32;2]{
        if self.starved{
            if self.samples.len()<RESUME{self.last=self.last.map(|v|v*0.99);return self.last;}
            self.starved=false;self.phase=0.0;
        }
        if self.samples.len()<2{
            self.starved=true;self.underruns+=1;self.total+=1;
            self.last=self.last.map(|v|v*0.99);return self.last;
        }
        let a=self.samples[0];let b=self.samples[1];let t=self.phase as f32;
        self.last=[a[0]+(b[0]-a[0])*t,a[1]+(b[1]-a[1])*t];
        self.phase+=self.step;
        while self.phase>=1.0&&!self.samples.is_empty(){self.samples.pop_front();self.phase-=1.0;}
        self.last
    }
    fn push(&mut self,samples:&[[i16;2]],volume:f32){
        if !samples.is_empty(){let now=std::time::Instant::now();if let Some(last)=self.producer.last{self.producer.max_gap_us=self.producer.max_gap_us.max(now.duration_since(last).as_micros()as u64);}self.producer.last=Some(now);self.producer.frames+=samples.len()as u64;self.producer.nonzero+=samples.iter().filter(|s|s[0]!=0||s[1]!=0).count()as u64;}
        self.samples.extend(samples.iter().map(|s|s.map(|v|v as f32*volume/32768.0)));
        if self.samples.len()>CAPACITY{let excess=self.samples.len()-CAPACITY;self.samples.drain(..excess);self.dropped+=excess as u64;}
    }
}
pub struct Audio{queue:Arc<Mutex<Queue>>,volume:Cell<f32>,_stream:cpal::Stream}
impl Audio{
    pub fn open(rate:f64)->Result<Self,String>{
        if !rate.is_finite()||rate<=0.0{return Err("invalid PCM source rate".into());}
        let device=cpal::default_host().default_output_device().ok_or("no host audio device")?;
        let supported=device.default_output_config().map_err(|e|e.to_string())?;
        let format=supported.sample_format();let config:cpal::StreamConfig=supported.into();
        let queue=Arc::new(Mutex::new(Queue{samples:VecDeque::with_capacity(CAPACITY),phase:0.0,step:rate/config.sample_rate.0 as f64,starved:true,last:[0.0;2],underruns:0,total:0,dropped:0,producer:Default::default()}));
        let stream=match format{
            SampleFormat::F32=>stream::<f32>(&device,&config,queue.clone()),
            SampleFormat::I16=>stream::<i16>(&device,&config,queue.clone()),
            SampleFormat::U16=>stream::<u16>(&device,&config,queue.clone()),
            SampleFormat::I32=>stream::<i32>(&device,&config,queue.clone()),
            _=>return Err(format!("unsupported host PCM format {format}")),
        }?;
        stream.play().map_err(|e|e.to_string())?;
        log::info!("NATIVE_HOST_AUDIO backend=cpal source_rate={rate} output_rate={} channels={}",config.sample_rate.0,config.channels);
        Ok(Self{queue,volume:Cell::new(1.0),_stream:stream})
    }
    pub fn set_volume(&self,volume:f32){self.volume.set(if volume.is_finite(){volume.clamp(0.0,1.0)}else{0.0});}
    pub fn take_underruns(&self)->u64{self.queue.lock().map(|mut q|std::mem::take(&mut q.underruns)).unwrap_or(0)}
    pub fn push(&self,samples:&[[i16;2]]){if let Ok(mut q)=self.queue.lock(){q.push(samples,self.volume.get());}}
}
impl super::HostAudioSink for Audio{fn push_pcm(&self,samples:&[[i16;2]]){self.push(samples);}}
impl Drop for Audio{fn drop(&mut self){if let Ok(q)=self.queue.lock(){log::info!("NATIVE_PLATFORM_AUDIO_COUNTERS zakuro_audio_output_calls=0 audio_underruns={} dropped_samples={} queued_samples={} produced_frames={} nonzero_frames={} max_producer_gap_us={}",q.total,q.dropped,q.samples.len(),q.producer.frames,q.producer.nonzero,q.producer.max_gap_us);}}}
fn stream<T:SizedSample+FromSample<f32>>(device:&cpal::Device,config:&cpal::StreamConfig,queue:Arc<Mutex<Queue>>)->Result<cpal::Stream,String>{
    let channels=config.channels as usize;
    device.build_output_stream(config,move|out:&mut[T],_|{
        out.fill(T::EQUILIBRIUM);
        if let Ok(mut q)=queue.lock(){for frame in out.chunks_mut(channels){let[a,b]=q.next();if channels==1{frame[0]=T::from_sample((a+b)*0.5);}else{frame[0]=T::from_sample(a);frame[1]=T::from_sample(b);}}}
    },|error|log::warn!("native host audio: {error}"),None).map_err(|e|e.to_string())
}
#[cfg(test)]mod tests{
    use super::*;
    fn queue()->Queue{Queue{samples:VecDeque::new(),phase:0.0,step:1.0,starved:true,last:[0.0;2],underruns:0,total:0,dropped:0,producer:Default::default()}}
    #[test]fn starvation_counts_real_transition_and_releases_to_silence(){let mut q=queue();q.push(&vec![[16384,-16384];RESUME],1.0);assert_eq!(q.next(),[0.5,-0.5]);for _ in 0..RESUME+2{q.next();}assert_eq!(q.total,1);for _ in 0..2000{q.next();}assert!(q.last[0].abs()<0.0001);}
    #[test]fn pcm_queue_is_bounded_and_mute_preserves_cadence(){let mut q=queue();q.push(&vec![[32767,32767];CAPACITY+123],0.0);assert_eq!(q.samples.len(),CAPACITY);assert_eq!(q.dropped,123);assert_eq!(q.next(),[0.0;2]);}
}
