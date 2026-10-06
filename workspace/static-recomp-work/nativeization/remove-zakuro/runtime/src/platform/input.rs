//! Direct host input snapshot for linked static-recomp overrides.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};

const A: u32 = 1 << 0;
const DPAD_RIGHT: u32 = 1 << 4;
const DPAD_LEFT: u32 = 1 << 5;
const DPAD_UP: u32 = 1 << 6;
const DPAD_DOWN: u32 = 1 << 7;
const LEFT: u32 = 1;
const RIGHT: u32 = 2;
const UP: u32 = 4;
const DOWN: u32 = 8;
const REPEAT_DELAY: u16 = 20;
// EUR retail Device::SetDefaultRepeatParam (0x00346EC8) loads {20,5}
// from 0x00597420. Keep the game's repeat cadence, independent of OS repeat.
const REPEAT_RATE: u16 = 5;

/// Host buttons and normalized analog axes, independent of any guest service.
#[derive(Clone, Copy, Default)]
pub struct PcInputState {
    pub held: u32,
    pub circle_x: f32,
    pub circle_y: f32,
    pub pressed_buttons: u32,
    pub pressed_circle: u32,
}

impl PcInputState {
    pub fn new(held: u32, circle_x: f32, circle_y: f32) -> Self {
        Self { held: held & 0x0fff, circle_x, circle_y, pressed_buttons:0, pressed_circle:0 }
    }
    pub fn with_press_edges(mut self,buttons:u32,circle:u32)->Self{
        self.pressed_buttons=buttons&0x0fff;
        self.pressed_circle=circle&15;self
    }
}

static BUTTONS: [AtomicU32; 4] = [const { AtomicU32::new(0) }; 4];
// Host/vblank snapshots can run faster than Moon's gameplay tick. Keep edges
// until the game has sampled them, then retire at the next snapshot, allowing
// all consumers in that tick to observe the same transition.
static BUTTON_EDGE_READ: [AtomicBool; 3] = [const { AtomicBool::new(false) }; 3];
static VECTORS: [[AtomicU32; 4]; 2] = [const { [const { AtomicU32::new(0) }; 4] }; 2];
static VECTOR_EDGE_READ: [[AtomicBool; 3]; 2] = [const { [const { AtomicBool::new(false) }; 3] }; 2];
static AXES: [[AtomicU32; 2]; 2] = [const { [const { AtomicU32::new(0) }; 2] }; 2];
static TRACE_EVENTS: AtomicU32 = AtomicU32::new(0);
static TRACKER: LazyLock<Mutex<Tracker>> = LazyLock::new(|| Mutex::new(Tracker::default()));

#[derive(Default)]
struct Tracker {
    previous_buttons: u32,
    previous_vectors: [u32; 2],
    button_age: [u16; 32],
    vector_age: [[u16; 4]; 2],
    diagnostic: bool,
    bypass: bool,
}

fn phases(current: u32, previous: u32, ages: &mut [u16]) -> [u32; 4] {
    let pressed = current & !previous;
    let released = previous & !current;
    let mut repeat = pressed;
    for (bit, age) in ages.iter_mut().enumerate() {
        if current & (1 << bit) == 0 {
            *age = 0;
        } else {
            *age = age.saturating_add(1);
            // Retail resets the counter to20 on the initial state change,
            // then decrements on subsequent updates (0x00346E24..00346E40).
            if *age > REPEAT_DELAY && (*age - REPEAT_DELAY - 1) % REPEAT_RATE == 0 {
                repeat |= 1 << bit;
            }
        }
    }
    [current, pressed, released, repeat]
}

fn directions(x: f32, y: f32) -> u32 {
    let mut state = 0;
    if y > 0.5 {
        state |= UP;
    }
    if y < -0.5 {
        state |= DOWN;
    }
    if x < -0.5 {
        state |= LEFT;
    }
    if x > 0.5 {
        state |= RIGHT;
    }
    state
}

pub fn configure() {
    let mut tracker = TRACKER.lock().unwrap();
    tracker.bypass = std::env::var("ZAKURO_HID_GAMEPLAY_BYPASS").is_ok_and(|value| value == "1");
    tracker.diagnostic = tracker.bypass || std::env::var("PC_INPUT_DIAGNOSTIC").is_ok_and(|value| value == "1");
    if tracker.bypass {
        log::info!("ZAKURO_HID_GAMEPLAY_BYPASS=1 (buttons and circle suppressed from HID)");
    }
}

/// Discard host transitions when an in-process frontend starts a new game.
pub fn reset_session() {
    *TRACKER.lock().unwrap()=Tracker::default();
    for button in &BUTTONS {button.store(0,Ordering::Relaxed);}
    for read in &BUTTON_EDGE_READ {read.store(false,Ordering::Relaxed);}
    for vectors in &VECTORS {for state in vectors {state.store(0,Ordering::Relaxed);}}
    for reads in &VECTOR_EDGE_READ {for read in reads {read.store(false,Ordering::Relaxed);}}
    for axes in &AXES {for axis in axes {axis.store(0,Ordering::Relaxed);}}
    TRACE_EVENTS.store(0,Ordering::Relaxed);
}

/// Publish exactly one native snapshot per emulated frame.
pub fn update(input: PcInputState) {
    let mut tracker = TRACKER.lock().unwrap();
    let buttons = input.held;
    let dpad = directions(
        ((buttons & DPAD_RIGHT != 0) as i8 - (buttons & DPAD_LEFT != 0) as i8) as f32,
        ((buttons & DPAD_UP != 0) as i8 - (buttons & DPAD_DOWN != 0) as i8) as f32,
    );
    let vectors = [dpad, directions(input.circle_x, input.circle_y)];
    for(bit,age)in tracker.button_age.iter_mut().enumerate(){if input.pressed_buttons&(1<<bit)!=0{*age=0;}}
    let mut button_phases = phases(buttons, tracker.previous_buttons, &mut tracker.button_age);
    button_phases[1]|=input.pressed_buttons;button_phases[3]|=input.pressed_buttons;
    tracker.previous_buttons = buttons;
    for (phase, (slot, value)) in BUTTONS.iter().zip(button_phases).enumerate() {
        if phase != 0 && !BUTTON_EDGE_READ[phase-1].swap(false,Ordering::AcqRel){
            slot.fetch_or(value,Ordering::AcqRel);
        }else{slot.store(value, Ordering::Release);}
    }
    let mut vector_events = 0;
    let forced_vectors=[
        u32::from(input.pressed_buttons&DPAD_LEFT!=0)*LEFT|u32::from(input.pressed_buttons&DPAD_RIGHT!=0)*RIGHT|
        u32::from(input.pressed_buttons&DPAD_UP!=0)*UP|u32::from(input.pressed_buttons&DPAD_DOWN!=0)*DOWN,
        u32::from(input.pressed_circle&4!=0)*LEFT|u32::from(input.pressed_circle&8!=0)*RIGHT|
        u32::from(input.pressed_circle&1!=0)*UP|u32::from(input.pressed_circle&2!=0)*DOWN,
    ];
    for kind in 0..2 {
        let forced=forced_vectors[kind]&vectors[kind];
        for(bit,age)in tracker.vector_age[kind].iter_mut().enumerate(){if forced&(1<<bit)!=0{*age=0;}}
        let mut vector_phases = phases(vectors[kind], tracker.previous_vectors[kind], &mut tracker.vector_age[kind]);
        vector_phases[1]|=forced;vector_phases[3]|=forced;
        tracker.previous_vectors[kind] = vectors[kind];
        vector_events |= vector_phases[1] | vector_phases[2];
        for (phase, (slot, value)) in VECTORS[kind].iter().zip(vector_phases).enumerate() {
            if phase != 0 && !VECTOR_EDGE_READ[kind][phase-1].swap(false, Ordering::AcqRel) {
                slot.fetch_or(value, Ordering::AcqRel);
            } else {
                slot.store(value, Ordering::Release);
            }
        }
    }
    let dpad_x = ((buttons & DPAD_RIGHT != 0) as i8 - (buttons & DPAD_LEFT != 0) as i8) as f32;
    let dpad_y = ((buttons & DPAD_UP != 0) as i8 - (buttons & DPAD_DOWN != 0) as i8) as f32;
    AXES[0][0].store(dpad_x.to_bits(), Ordering::Release);
    AXES[0][1].store(dpad_y.to_bits(), Ordering::Release);
    AXES[1][0].store(input.circle_x.to_bits(), Ordering::Release);
    AXES[1][1].store(input.circle_y.to_bits(), Ordering::Release);
    if tracker.diagnostic && (button_phases[1] | button_phases[2] | vector_events) != 0 {
        log::info!(target: "pc_input", "PC_INPUT: A held={} A pressed={} circle=({:.3},{:.3})", buttons & A != 0, button_phases[1] & A != 0, input.circle_x, input.circle_y);
    }
}

pub fn bypass_enabled() -> bool {
    TRACKER.lock().unwrap().bypass
}

#[unsafe(no_mangle)]
pub extern "C" fn pc_input_buttons(phase: u32) -> u32 {
    if (1..=3).contains(&phase){BUTTON_EDGE_READ[phase as usize-1].store(true,Ordering::Release);}
    let value=BUTTONS.get(phase as usize).map_or(0, |value| value.load(Ordering::Acquire));
    static SEEN:AtomicU32=AtomicU32::new(0);
    if value!=0&&SEEN.fetch_add(1,Ordering::Relaxed)==0{log::info!("PC_INPUT_NATIVE_GAME_READ phase={phase} value=0x{value:X}");}
    value
}

#[unsafe(no_mangle)]
pub extern "C" fn pc_input_vector(kind: u32, phase: u32) -> u32 {
    if (1..=3).contains(&phase) {
        if let Some(read) = VECTOR_EDGE_READ.get(kind as usize) {
            read[phase as usize-1].store(true, Ordering::Release);
        }
    }
    VECTORS.get(kind as usize).and_then(|states| states.get(phase as usize)).map_or(0, |value| value.load(Ordering::Acquire))
}

#[unsafe(no_mangle)]
pub extern "C" fn pc_input_axis(kind: u32, axis: u32) -> f32 {
    let value=AXES.get(kind as usize).and_then(|axes| axes.get(axis as usize)).map_or(0.0, |value| f32::from_bits(value.load(Ordering::Acquire)));
    static SEEN:AtomicU32=AtomicU32::new(0);
    if value.abs()>0.1&&SEEN.fetch_add(1,Ordering::Relaxed)==0{log::info!("PC_INPUT_NATIVE_AXIS_READ kind={kind} axis={axis} value={value}");}
    value
}

#[unsafe(no_mangle)]
pub extern "C" fn pc_input_trace(event: u32, value: u32) {
    let bit = 1u32.checked_shl(event).unwrap_or(0);
    if bit != 0 && TRACE_EVENTS.fetch_or(bit, Ordering::Relaxed) & bit == 0 {
        log::info!(target: "pc_input", "PC_INPUT_GAME: event={event} object=0x{value:08X}");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pc_input_menu_trace(caller:u32,direction:u32,accepted:u32) {
    static ENABLED:std::sync::OnceLock<bool>=std::sync::OnceLock::new();
    static COUNT:AtomicU32=AtomicU32::new(0);
    if !*ENABLED.get_or_init(||std::env::var("MENU_INPUT_DIAGNOSTIC").as_deref()==Ok("1")) {return;}
    let pressed=VECTORS[0][1].load(Ordering::Acquire)|VECTORS[1][1].load(Ordering::Acquire);
    let held=VECTORS[0][0].load(Ordering::Acquire)|VECTORS[1][0].load(Ordering::Acquire);
    let repeat=VECTORS[0][3].load(Ordering::Acquire)|VECTORS[1][3].load(Ordering::Acquire);
    if (accepted!=0||pressed&direction!=0)&&COUNT.fetch_add(1,Ordering::Relaxed)<64 {
        log::info!("MENU_INPUT dir={direction:x} pressed={} held={} repeat={} accepted={accepted} consumer={caller:08x}",u32::from(pressed&direction!=0),u32::from(held&direction!=0),u32::from(repeat&direction!=0));
    }
}
