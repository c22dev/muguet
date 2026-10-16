// muguet - pad.rs
// Constantin Clerc (c22dev) - 0.1

use crate::input;
use objc2_game_controller::{GCController, GCControllerButtonInput, GCExtendedGamepad};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub static ACTIVE: AtomicBool = AtomicBool::new(false);
pub static FOCUSED: AtomicBool = AtomicBool::new(true);

pub static IN_VEHICLE: AtomicBool = AtomicBool::new(false);

pub const MODE_FOOT: u8 = 0;
pub const MODE_CAR: u8 = 1;
pub const MODE_BIKE: u8 = 2;
pub const MODE_HELI: u8 = 3;
pub const MODE_PLANE: u8 = 4;
pub const MODE_BOAT: u8 = 5;
pub const MODE_SUB: u8 = 6;
pub const MODE_BICYCLE: u8 = 7;
pub const MODE_PASSENGER: u8 = 8;
pub static VEH_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(MODE_FOOT);

pub const CTX_TOWING: u32 = 1;
pub const CTX_TRAILER: u32 = 2;
pub const CTX_CARGOBOB: u32 = 4;
pub const CTX_ROPE: u32 = 8;
pub const CTX_CONVERTIBLE: u32 = 16;
pub const CTX_VTOL: u32 = 32;
pub const CTX_GUN: u32 = 64;
pub const CTX_MELEE: u32 = 128;
pub const CTX_THROWN: u32 = 256;
pub const CTX_SWIM: u32 = 512;
pub const CTX_SNIPER: u32 = 1024;
pub static CONTEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub static INJECT: Mutex<Vec<(i32, f32)>> = Mutex::new(Vec::new());
pub static RADIO_WHEEL: Mutex<Option<(f32, f32)>> = Mutex::new(None);
pub const FRONTEND_CONTROL: i32 = 2;
pub const RADIO_WHEEL_UD: i32 = 332;
pub const RADIO_WHEEL_LR: i32 = 333;

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub enum Request {
    DropTow,
    DropTrailer,
    Rope,
}
pub static REQUESTS: Mutex<Vec<Request>> = Mutex::new(Vec::new());
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub fn request(r: Request) {
    REQUESTS.lock().unwrap().push(r);
}

static MENU: AtomicBool = AtomicBool::new(false);
static SCRIPT_TICK: Mutex<Option<Instant>> = Mutex::new(None);
pub fn script_tick(menu: bool) {
    MENU.store(menu, Ordering::Relaxed);
    *SCRIPT_TICK.lock().unwrap() = Some(Instant::now());
}
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub fn in_menu() -> bool {
    MENU.load(Ordering::Relaxed) || SCRIPT_TICK.lock().unwrap().is_some_and(|t| t.elapsed() > Duration::from_millis(500))
}

pub static HORN_PAD: AtomicBool = AtomicBool::new(false);
pub static HORN_TOUCH: AtomicBool = AtomicBool::new(false);
pub fn horn_held() -> bool {
    HORN_PAD.load(Ordering::Relaxed) || HORN_TOUCH.load(Ordering::Relaxed)
}

struct State {
    held: HashSet<&'static str>,
    buttons: u8,
    last: Instant,
    down_since: Option<Instant>,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);

const SWITCH_HOLD: Duration = Duration::from_millis(300);
const STICK_DEADZONE: f32 = 0.3;
const LOOK_DEADZONE: f32 = 0.12;

const LOOK_SPEED: f64 = 700.0;

fn gamepad() -> Option<objc2::rc::Retained<GCExtendedGamepad>> {
    unsafe { GCController::controllers().iter().find_map(|c| c.extendedGamepad()) }
}

fn down(b: &GCControllerButtonInput) -> bool {
    unsafe { b.isPressed() }
}

pub fn poll() -> bool {
    if !FOCUSED.load(Ordering::Relaxed) { release_all(); return false; }
    let Some(gp) = gamepad() else {
        if ACTIVE.swap(false, Ordering::Relaxed) {
            release_all();
            eprintln!("[pad] controller disconnected");
        }
        return false;
    };
    if !ACTIVE.swap(true, Ordering::Relaxed) {
        eprintln!("[pad] controller connected");
    }
    let mut guard = STATE.lock().unwrap();
    let st = guard.get_or_insert_with(|| State { held: HashSet::new(), buttons: 0, last: Instant::now(), down_since: None });
    let dt = st.last.elapsed().as_secs_f64().min(0.05);
    st.last = Instant::now();

    let in_car = IN_VEHICLE.load(Ordering::Relaxed);
    let in_air = in_car && matches!(VEH_MODE.load(Ordering::Relaxed), MODE_HELI | MODE_PLANE);
    HORN_PAD.store(false, Ordering::Relaxed);
    let mut keys: HashSet<&'static str> = HashSet::new();
    let mut buttons = 0u8;
    unsafe {
        let ls = gp.leftThumbstick();
        let (lx, ly) = (ls.xAxis().value(), ls.yAxis().value());
        if in_air {
            if lx < -STICK_DEADZONE { keys.insert("Numpad4"); }
            if lx > STICK_DEADZONE { keys.insert("Numpad6"); }
            if ly > STICK_DEADZONE { keys.insert("Numpad8"); }
            if ly < -STICK_DEADZONE { keys.insert("Numpad2"); }
            if down(&gp.leftShoulder()) { keys.insert("KeyA"); }
            if down(&gp.rightShoulder()) { keys.insert("KeyD"); }
            if gp.leftThumbstickButton().is_some_and(|b| down(&b)) { keys.insert("KeyG"); }
            if gp.rightTrigger().value() > 0.2 { keys.insert("KeyW"); }
            if gp.leftTrigger().value() > 0.2 { keys.insert("KeyS"); }
        } else if in_car {
            if lx < -STICK_DEADZONE { keys.insert("KeyA"); }
            if lx > STICK_DEADZONE { keys.insert("KeyD"); }
            if ly > STICK_DEADZONE { keys.insert("Numpad8"); }
            if ly < -STICK_DEADZONE { keys.insert("Numpad2"); }
            if gp.rightTrigger().value() > 0.2 { keys.insert("KeyW"); }
            if gp.leftTrigger().value() > 0.2 { keys.insert("KeyS"); }
            if down(&gp.rightShoulder()) { keys.insert("Space"); }
            HORN_PAD.store(gp.leftThumbstickButton().is_some_and(|b| down(&b)), Ordering::Relaxed);
            if down(&gp.buttonX()) { keys.insert("Space"); }
        } else {
            if lx < -STICK_DEADZONE { keys.insert("KeyA"); }
            if lx > STICK_DEADZONE { keys.insert("KeyD"); }
            if ly > STICK_DEADZONE { keys.insert("KeyW"); }
            if ly < -STICK_DEADZONE { keys.insert("KeyS"); }
            if gp.rightTrigger().value() > 0.2 { buttons |= 1; }
            if gp.leftTrigger().value() > 0.2 { buttons |= 2; }
            if down(&gp.buttonA()) { keys.insert("ShiftLeft"); }
            if down(&gp.buttonX()) { keys.insert("Space"); }
            if down(&gp.rightShoulder()) { keys.insert("KeyQ"); }
            if gp.leftThumbstickButton().is_some_and(|b| down(&b)) { keys.insert("ControlLeft"); }
        }
        if down(&gp.buttonA()) { keys.insert("Enter"); }
        if down(&gp.buttonB()) { keys.insert("KeyR"); keys.insert("Backspace"); }
        if down(&gp.buttonY()) { keys.insert("KeyF"); }
        if down(&gp.leftShoulder()) && !in_air { keys.insert("Tab"); }
        if gp.rightThumbstickButton().is_some_and(|b| down(&b)) { keys.insert("KeyC"); }
        let dp = gp.dpad();
        if down(&dp.up()) { keys.insert("ArrowUp"); }
        if down(&dp.down()) {
            let since = *st.down_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= SWITCH_HOLD { keys.insert("AltLeft"); } else { keys.insert("ArrowDown"); }
        } else {
            st.down_since = None;
        }
        if down(&dp.left()) { keys.insert("ArrowLeft"); }
        if down(&dp.right()) { keys.insert("ArrowRight"); }
        if down(&gp.buttonMenu()) { keys.insert("Escape"); }
        if gp.buttonOptions().is_some_and(|b| down(&b)) { keys.insert("KeyV"); }

        let rs = gp.rightThumbstick();
        let curve = |v: f32| -> f64 {
            let a = v.abs();
            if a < LOOK_DEADZONE { 0.0 } else { (((a - LOOK_DEADZONE) / (1.0 - LOOK_DEADZONE)) as f64).powf(1.8).copysign(v as f64) }
        };
        let (dx, dy) = (curve(rs.xAxis().value()), -curve(rs.yAxis().value()));
        if dx != 0.0 || dy != 0.0 {
            let speed = LOOK_SPEED * input::look_sens();
            input::mouse_delta_f(dx * speed * dt, dy * speed * dt);
        }
    }

    if cfg!(target_os = "ios") { input::set_locked(true); }

    for k in st.held.difference(&keys) { input::key(k, false, false, None, false); }
    for k in keys.difference(&st.held) {
        crate::mp::key_pressed(k);
        input::key(k, true, false, None, false);
    }
    st.held = keys;
    for (bit, dom) in [(1u8, 0usize), (2, 2)] {
        if (buttons ^ st.buttons) & bit != 0 { input::button(dom, buttons & bit != 0); }
    }
    st.buttons = buttons;
    true
}

fn release(st: State) {
    for k in st.held { input::key(k, false, false, None, false); }
    if st.buttons & 1 != 0 { input::button(0, false); }
    if st.buttons & 2 != 0 { input::button(2, false); }
}

pub fn release_all() {
    HORN_PAD.store(false, Ordering::Relaxed);
    if let Some(st) = STATE.lock().unwrap().take() { release(st); }
}
