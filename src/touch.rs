// muguet - touch.rs
// Constantin Clerc (c22dev) - 0.1

use crate::input;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::NSString;
use std::collections::{HashMap, HashSet};
use winit::event::{Touch, TouchPhase};
use winit::window::Window;

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Key(&'static str),

    Mouse(u8),

    AimToggle,

    KeyNav(&'static str),
    NavToggle,

    Horn,

    Ctrl(i32, f32),
    Req(crate::pad::Request),
}

#[derive(Clone, Copy)]
enum Anchor {
    BottomRight(f64, f64),
    TopLeft(f64, f64),
    TopRight(f64, f64),

    Nav(f64, f64),
}

const FOOT: usize = 0;
const HELI: usize = 3;
const PLANE: usize = 4;
const SUB: usize = 6;
const MODES: usize = 9;

type Slot = Option<(&'static str, Act)>;

struct Spec {
    anchor: Anchor,
    radius: f64,
    acts: [Slot; MODES],
    need: u32,
    unless: u32,
    nav: bool,
}

const fn b(anchor: Anchor, radius: f64, acts: [Slot; MODES]) -> Spec {
    Spec { anchor, radius, acts, need: 0, unless: 0, nav: false }
}
const fn all(anchor: Anchor, radius: f64, label: &'static str, act: Act) -> Spec {
    b(anchor, radius, [Some((label, act)); MODES])
}
const fn driving(anchor: Anchor, radius: f64, slot: Slot) -> Spec {
    b(anchor, radius, [None, slot, slot, slot, slot, slot, slot, slot, None])
}
const fn when(need: u32, anchor: Anchor, label: &'static str, act: Act) -> Spec {
    let slot = Some((label, act));
    Spec { anchor, radius: 24.0, acts: [None, slot, slot, slot, slot, slot, slot, slot, None], need, unless: 0, nav: false }
}
const fn held(need: u32, unless: u32, anchor: Anchor, radius: f64, label: &'static str, act: Act) -> Spec {
    let mut acts = [None; MODES];
    acts[FOOT] = Some((label, act));
    Spec { anchor, radius, acts, need, unless, nav: false }
}
const fn n(x: f64, label: &'static str, key: &'static str) -> Spec {
    Spec { anchor: Anchor::Nav(x, 0.0), radius: 21.0, acts: [Some((label, Act::Key(key))); MODES], need: 0, unless: 0, nav: true }
}
const fn on(label: &'static str, control: i32) -> Slot {
    Some((label, Act::Ctrl(control, 1.0)))
}

const VEH_MOVE_LR: i32 = 59;
const VEH_MOVE_UD: i32 = 60;
const VEH_ACCELERATE: i32 = 71;
const VEH_BRAKE: i32 = 72;
const VEH_DUCK: i32 = 73;
const VEH_HEADLIGHT: i32 = 74;
const VEH_HANDBRAKE: i32 = 76;
const VEH_LOOK_BEHIND: i32 = 79;
const VEH_FLY_THROTTLE_UP: i32 = 87;
const VEH_FLY_THROTTLE_DOWN: i32 = 88;
const VEH_FLY_YAW_LEFT: i32 = 89;
const VEH_FLY_YAW_RIGHT: i32 = 90;
const VEH_PASSENGER_AIM: i32 = 91;
const VEH_PASSENGER_ATTACK: i32 = 92;
const VEH_ROOF: i32 = 101;
const VEH_FLY_ROLL_LR: i32 = 107;
const VEH_FLY_PITCH_UD: i32 = 110;
const VEH_FLY_UNDERCARRIAGE: i32 = 113;
const VEH_FLY_ATTACK: i32 = 114;
const VEH_FLY_VERTICAL_FLIGHT_MODE: i32 = 119;
const VEH_SUB_TURN_LR: i32 = 123;
const VEH_SUB_PITCH_UD: i32 = 126;
const VEH_SUB_THROTTLE_UP: i32 = 129;
const VEH_SUB_THROTTLE_DOWN: i32 = 130;
const VEH_SUB_ASCEND: i32 = 131;
const VEH_SUB_DESCEND: i32 = 132;
const VEH_PUSHBIKE_PEDAL: i32 = 136;
const VEH_PUSHBIKE_SPRINT: i32 = 137;
const SNIPER_ZOOM_IN: i32 = 42;
const SNIPER_ZOOM_OUT: i32 = 43;
const VEH_PUSHBIKE_FRONT_BRAKE: i32 = 138;

use crate::pad::{CTX_CARGOBOB, CTX_CONVERTIBLE, CTX_GUN, CTX_MELEE, CTX_ROPE, CTX_SNIPER, CTX_SWIM, CTX_THROWN, CTX_TOWING, CTX_TRAILER, CTX_VTOL};
use crate::pad::Request as R;
use Act::*;
use Anchor::*;
const GAS: Slot = on("GAS", VEH_ACCELERATE);
const BRAKE: Slot = on("BRAKE", VEH_BRAKE);
const HBRAKE: Slot = on("H.BRAKE", VEH_HANDBRAKE);
const HORN: Slot = Some(("HORN", Horn));
const SHOOT: Slot = Some(("SHOOT", Mouse(1)));
const AIR_SHOOT: Slot = on("SHOOT", VEH_FLY_ATTACK);
const DUCK: Slot = on("DUCK", VEH_DUCK);
const RADIO: Slot = Some(("RADIO", Key("KeyQ")));
const BACK: Slot = on("LOOK\nBACK", VEH_LOOK_BEHIND);
const YAW_L: Slot = on("YAW\nLEFT", VEH_FLY_YAW_LEFT);
const YAW_R: Slot = on("YAW\nRIGHT", VEH_FLY_YAW_RIGHT);
const BUTTONS: &[Spec] = &[
    held(0, CTX_GUN | CTX_MELEE | CTX_THROWN, BottomRight(62.0, 68.0), 40.0, "PUNCH", Mouse(1)),
    held(CTX_GUN, 0, BottomRight(62.0, 68.0), 40.0, "FIRE", Mouse(1)),
    held(CTX_MELEE, 0, BottomRight(62.0, 68.0), 40.0, "ATTACK", Mouse(1)),
    held(CTX_THROWN, 0, BottomRight(62.0, 68.0), 40.0, "THROW", Mouse(1)),
    held(CTX_SWIM, 0, BottomRight(222.0, 160.0), 24.0, "SPRINT", Key("ShiftLeft")),
    held(CTX_GUN, 0, BottomRight(222.0, 98.0), 24.0, "RELOAD", Key("KeyR")),
    held(CTX_SNIPER, 0, BottomRight(300.0, 132.0), 24.0, "ZOOM\nIN", Ctrl(SNIPER_ZOOM_IN, 1.0)),
    held(CTX_SNIPER, 0, BottomRight(300.0, 70.0), 24.0, "ZOOM\nOUT", Ctrl(SNIPER_ZOOM_OUT, 1.0)),
    b(BottomRight(62.0, 68.0), 40.0, [None, GAS, GAS, on("UP", VEH_FLY_THROTTLE_UP), on("THRUST", VEH_FLY_THROTTLE_UP), GAS, on("GAS", VEH_SUB_THROTTLE_UP), on("PEDAL", VEH_PUSHBIKE_PEDAL), on("SHOOT", VEH_PASSENGER_ATTACK)]),
    b(BottomRight(158.0, 46.0), 31.0, [Some(("AIM", AimToggle)), BRAKE, BRAKE, on("DOWN", VEH_FLY_THROTTLE_DOWN), on("SLOW", VEH_FLY_THROTTLE_DOWN), on("REVERSE", VEH_BRAKE), on("REVERSE", VEH_SUB_THROTTLE_DOWN), on("BRAKE", VEH_PUSHBIKE_FRONT_BRAKE), on("AIM", VEH_PASSENGER_AIM)]),
    b(BottomRight(58.0, 168.0), 28.0, [Some(("JUMP", Key("Space"))), HBRAKE, HBRAKE, YAW_L, YAW_L, None, on("RISE", VEH_SUB_ASCEND), on("HOP", VEH_HANDBRAKE), None]).unless(CTX_SWIM),
    held(CTX_SWIM, 0, BottomRight(58.0, 168.0), 28.0, "DIVE", Key("Space")),
    all(BottomRight(140.0, 132.0), 26.0, "EXIT", Key("KeyF")).foot(Some(("ENTER", Key("KeyF")))),
    b(BottomRight(222.0, 98.0), 24.0, [None, HORN, HORN, YAW_R, YAW_R, HORN, on("DIVE", VEH_SUB_DESCEND), on("SPRINT", VEH_PUSHBIKE_SPRINT), None]),
    b(BottomRight(140.0, 200.0), 24.0, [Some(("ACTION", Key("KeyE"))), SHOOT, SHOOT, AIR_SHOOT, AIR_SHOOT, None, None, None, None]),
    b(BottomRight(222.0, 160.0), 24.0, [None, DUCK, DUCK, None, on("GEAR", VEH_FLY_UNDERCARRIAGE), None, None, None, DUCK]),
    when(CTX_TOWING, BottomRight(300.0, 70.0), "DROP\nCAR", Req(R::DropTow)),
    when(CTX_TRAILER, BottomRight(300.0, 70.0), "DROP\nTRAILER", Req(R::DropTrailer)),
    when(CTX_CARGOBOB, BottomRight(300.0, 70.0), "LOWER\nROPE", Req(R::Rope)).unless(CTX_ROPE),
    when(CTX_CARGOBOB | CTX_ROPE, BottomRight(300.0, 70.0), "DROP\nLOAD", Req(R::Rope)),
    when(CTX_VTOL, BottomRight(300.0, 70.0), "VTOL", Ctrl(VEH_FLY_VERTICAL_FLIGHT_MODE, 1.0)),
    when(CTX_CONVERTIBLE, BottomRight(300.0, 132.0), "ROOF", Ctrl(VEH_ROOF, 1.0)),
    all(TopRight(30.0, 30.0), 22.0, "WEAPON", Key("Tab")),
    b(TopRight(84.0, 30.0), 22.0, [Some(("COVER", Key("KeyQ"))), RADIO, RADIO, RADIO, RADIO, RADIO, RADIO, None, RADIO]),
    b(TopRight(138.0, 30.0), 22.0, [Some(("CROUCH", Key("ControlLeft"))), BACK, BACK, None, None, BACK, None, BACK, BACK]),
    all(TopRight(192.0, 30.0), 22.0, "CAM", Key("KeyV")),
    driving(TopRight(30.0, 84.0), 22.0, on("LIGHTS", VEH_HEADLIGHT)),
    all(TopRight(84.0, 84.0), 22.0, "ABILITY", Key("CapsLock")),
    all(TopLeft(30.0, 30.0), 22.0, "PAUSE", KeyNav("Escape")),
    all(TopLeft(84.0, 30.0), 22.0, "PHONE", KeyNav("ArrowUp")),
    all(TopLeft(138.0, 30.0), 22.0, "MENU\nKEYS", NavToggle),
    all(TopLeft(192.0, 30.0), 22.0, "SWITCH", Key("AltLeft")),
    n(-135.0, "◀", "ArrowLeft"),
    n(-87.0, "▲", "ArrowUp"),
    n(-39.0, "▼", "ArrowDown"),
    n(9.0, "▶", "ArrowRight"),
    n(63.0, "OK", "Enter"),
    n(117.0, "BACK", "Backspace"),
];

impl Spec {
    const fn unless(mut self, bits: u32) -> Spec {
        self.unless = bits;
        self
    }
    const fn foot(mut self, slot: Slot) -> Spec {
        self.acts[FOOT] = slot;
        self
    }
}



const STICK_RADIUS: f64 = 58.0;
const KNOB_RADIUS: f64 = 26.0;
const SPRINT_REACH: f64 = 1.35;

const LOOK_GAIN: f64 = 3.0;
const WHEEL_DEADZONE: f64 = 10.0;
const WHEEL_FULL: f64 = 45.0;
const WHEEL_PUSH: f64 = 40.0;

const MIN_HOLD: std::time::Duration = std::time::Duration::from_millis(120);
const ROOF_HOLD: std::time::Duration = std::time::Duration::from_millis(1000);
const CLICK_DELAY: std::time::Duration = std::time::Duration::from_millis(50);

enum Grip {
    Stick { origin: (f64, f64), at: (f64, f64) },
    Look { last: (f64, f64) },
    Button { idx: usize, start: (f64, f64), last: (f64, f64) },
    Cursor { since: std::time::Instant, up: bool },
}

struct Btn {
    view: Retained<AnyObject>,
    label: Retained<AnyObject>,
    icon: Retained<AnyObject>,
    center: (f64, f64),
    radius: f64,
}

struct Overlay {
    root: Retained<AnyObject>,
    buttons: Vec<Btn>,
    stick_base: Retained<AnyObject>,
    stick_knob: Retained<AnyObject>,
    stick_home: (f64, f64),

    safe: (f64, f64, f64, f64),
    full: (f64, f64),
    scale: f64,
    grips: HashMap<u64, Grip>,
    held: HashSet<&'static str>,

    since: HashMap<&'static str, std::time::Instant>,
    mouse_since: [std::time::Instant; 2],

    lingering: bool,
    mouse: u8,
    aim_latched: bool,
    nav: bool,
    in_car: bool,
    mode: usize,
    ctx: u32,
    menu: bool,
    hidden: bool,
    ctrl_since: HashMap<i32, std::time::Instant>,
    injected: Vec<(i32, f32)>,
}

thread_local! {
    static OVERLAY: std::cell::RefCell<Option<Overlay>> = const { std::cell::RefCell::new(None) };
}

enum Fps {
    Unset,
    Off,
    On { label: Retained<AnyObject>, at: std::time::Instant, presents: u64 },
}
thread_local! {
    static FPS: std::cell::RefCell<Fps> = const { std::cell::RefCell::new(Fps::Unset) };
}

fn fps_label(o: &Overlay) -> Option<Retained<AnyObject>> {
    unsafe {
        let parent: Option<Retained<AnyObject>> = msg_send![&*o.root, superview];
        let parent = parent?;
        let l: Retained<AnyObject> = msg_send![class!(UILabel), new];
        let _: () = msg_send![&*l, setFrame: rect(o.safe.0 + 226.0, o.safe.1 + 8.0, 58.0, 17.0)];
        let _: () = msg_send![&*l, setUserInteractionEnabled: false];
        let _: () = msg_send![&*l, setTextColor: &*color(1.0, 0.9)];
        let _: () = msg_send![&*l, setBackgroundColor: &*color(0.0, 0.35)];
        let font: Retained<AnyObject> = msg_send![class!(UIFont), monospacedDigitSystemFontOfSize: 11.0f64, weight: 0.3f64];
        let _: () = msg_send![&*l, setFont: &*font];
        let _: () = msg_send![&*l, setTextAlignment: 1isize];
        let layer: *mut AnyObject = msg_send![&*l, layer];
        let _: () = msg_send![layer, setCornerRadius: 4.0f64];
        let _: () = msg_send![layer, setMasksToBounds: true];
        let _: () = msg_send![&*parent, addSubview: &*l];
        Some(l)
    }
}

thread_local! {
    static STOP_NOTE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn stop_tick(o: &Overlay) {
    if STOP_NOTE.get() { return; }
    let Some(reason) = crate::host::stop_reason() else { return };
    STOP_NOTE.set(true);
    unsafe {
        let parent: Option<Retained<AnyObject>> = msg_send![&*o.root, superview];
        let Some(parent) = parent else { return };
        let bounds: CGRect = msg_send![&*parent, bounds];
        let l: Retained<AnyObject> = msg_send![class!(UILabel), new];
        let _: () = msg_send![&*l, setFrame: bounds];
        let _: () = msg_send![&*l, setNumberOfLines: 0isize];
        let _: () = msg_send![&*l, setTextAlignment: 1isize];
        let _: () = msg_send![&*l, setTextColor: &*color(1.0, 1.0)];
        let _: () = msg_send![&*l, setBackgroundColor: &*color(0.0, 0.8)];
        let font: Retained<AnyObject> = msg_send![class!(UIFont), systemFontOfSize: 17.0f64];
        let _: () = msg_send![&*l, setFont: &*font];
        set_text(&l, &format!("The game hit an error and can't continue.\n\n{reason}\n\nClose Muguet from the app switcher and open it again."));
        let _: () = msg_send![&*parent, addSubview: &*l];
    }
}

fn fps_tick(o: &Overlay) {
    let now_presents = crate::gpu::PRESENTS.load(std::sync::atomic::Ordering::Relaxed);
    if now_presents == 0 { return; }
    FPS.with(|cell| {
        let mut f = cell.borrow_mut();
        if let Fps::Unset = *f {
            let on = std::env::var("MUGUET_SHOW_FPS").map(|v| v != "0").unwrap_or(false);
            *f = match (on, on.then(|| fps_label(o)).flatten()) {
                (true, Some(label)) => Fps::On { label, at: std::time::Instant::now(), presents: now_presents },
                _ => Fps::Off,
            };
        }
        if let Fps::On { label, at, presents } = &mut *f {
            let dt = at.elapsed().as_secs_f64();
            if dt >= 0.5 {
                set_text(label, &format!("{:.0} fps", (now_presents - *presents) as f64 / dt));
                *at = std::time::Instant::now();
                *presents = now_presents;
            }
        }
    });
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> CGRect {
    CGRect { origin: CGPoint { x, y }, size: CGSize { width: w, height: h } }
}

fn color(white: f64, alpha: f64) -> Retained<AnyObject> {
    unsafe { msg_send![class!(UIColor), colorWithWhite: white, alpha: alpha] }
}

fn circle(parent: &AnyObject, center: (f64, f64), r: f64, white: f64, alpha: f64) -> Retained<AnyObject> {
    unsafe {
        let v: Retained<AnyObject> = msg_send![class!(UIView), new];
        let _: () = msg_send![&*v, setFrame: rect(center.0 - r, center.1 - r, 2.0 * r, 2.0 * r)];
        let _: () = msg_send![&*v, setUserInteractionEnabled: false];
        let _: () = msg_send![&*v, setBackgroundColor: &*color(white, alpha)];
        let layer: *mut AnyObject = msg_send![&*v, layer];
        let _: () = msg_send![layer, setCornerRadius: r];
        let _: () = msg_send![parent, addSubview: &*v];
        v
    }
}

fn label(parent: &AnyObject, r: f64) -> Retained<AnyObject> {
    unsafe {
        let l: Retained<AnyObject> = msg_send![class!(UILabel), new];
        let _: () = msg_send![&*l, setFrame: rect(2.0, 0.0, 2.0 * r - 4.0, 2.0 * r)];
        let _: () = msg_send![&*l, setUserInteractionEnabled: false];
        let _: () = msg_send![&*l, setTextColor: &*color(1.0, 0.9)];
        let _: () = msg_send![&*l, setShadowColor: &*color(0.0, 0.85)];
        let _: () = msg_send![&*l, setShadowOffset: CGSize { width: 0.0, height: 1.0 }];
        let font: Retained<AnyObject> = msg_send![class!(UIFont), boldSystemFontOfSize: (r * 0.42).clamp(9.0, 15.0)];
        let _: () = msg_send![&*l, setFont: &*font];
        let _: () = msg_send![&*l, setTextAlignment: 1isize];
        let _: () = msg_send![&*l, setNumberOfLines: 2isize];
        let _: () = msg_send![&*l, setAdjustsFontSizeToFitWidth: true];
        let _: () = msg_send![parent, addSubview: &*l];
        l
    }
}

fn symbol(label: &str) -> Option<&'static str> {
    Some(match label {
        "PUNCH" => "hand.raised.fill",
        "FIRE" | "SHOOT" => "target",
        "ATTACK" => "bolt.fill",
        "THROW" => "arrow.up.right.circle.fill",
        "AIM" => "scope",
        "RELOAD" => "arrow.triangle.2.circlepath",
        "ZOOM\nIN" => "plus.magnifyingglass",
        "ZOOM\nOUT" => "minus.magnifyingglass",
        "JUMP" => "chevron.up.2",
        "ENTER" => "car.fill",
        "EXIT" => "rectangle.portrait.and.arrow.right",
        "WEAPON" => "circle.hexagongrid.fill",
        "COVER" => "shield.fill",
        "CROUCH" => "chevron.down.2",
        "CAM" => "eye.fill",
        "PAUSE" => "pause.fill",
        "PHONE" => "iphone",
        "MENU\nKEYS" => "keyboard",
        "SWITCH" => "person.2.fill",
        "MODS" => "wrench.and.screwdriver.fill",
        "GAS" | "THRUST" => "arrow.up",
        "BRAKE" | "SLOW" | "REVERSE" => "arrow.down",
        "H.BRAKE" => "parkingsign",
        "HORN" => "speaker.wave.3.fill",
        "DUCK" => "arrow.down.to.line",
        "LOOK\nBACK" => "arrow.uturn.backward",
        "LIGHTS" => "lightbulb.fill",
        "RADIO" => "music.note",
        "UP" | "RISE" => "arrow.up.to.line",
        "DOWN" | "DIVE" => "arrow.down.to.line",
        "YAW\nLEFT" => "arrow.counterclockwise",
        "YAW\nRIGHT" => "arrow.clockwise",
        "GEAR" => "chevron.down.circle",
        "PEDAL" => "bicycle",
        "SPRINT" => "hare.fill",
        "HOP" => "arrow.up.forward",
        "DROP\nCAR" | "DROP\nTRAILER" | "DROP\nLOAD" => "link.badge.minus",
        "LOWER\nROPE" => "arrow.down.circle",
        "VTOL" => "airplane",
        "ROOF" => "sun.max.fill",
        "OK" => "checkmark",
        "BACK" => "arrow.uturn.left",
        "ACTION" => "hand.point.up.left.fill",
        "ABILITY" => "sparkles",
        _ => return None,
    })
}

fn icon_view(parent: &AnyObject, r: f64) -> Retained<AnyObject> {
    unsafe {
        let v: Retained<AnyObject> = msg_send![class!(UIImageView), new];
        let _: () = msg_send![&*v, setFrame: rect(0.0, 0.0, 2.0 * r, 2.0 * r)];
        let _: () = msg_send![&*v, setUserInteractionEnabled: false];
        let _: () = msg_send![&*v, setContentMode: 4isize];
        let _: () = msg_send![&*v, setTintColor: &*color(1.0, 0.95)];
        let _: () = msg_send![parent, addSubview: &*v];
        v
    }
}

fn set_content(b: &Btn, text: &str) {
    let image: Option<Retained<AnyObject>> = symbol(text).and_then(|name| unsafe {
        let config: Retained<AnyObject> = msg_send![class!(UIImageSymbolConfiguration), configurationWithPointSize: (b.radius * 0.8).clamp(12.0, 30.0), weight: 6isize];
        msg_send![class!(UIImage), systemImageNamed: &*NSString::from_str(name), withConfiguration: &*config]
    });
    let d = 2.0 * b.radius;
    unsafe {
        let _: () = msg_send![&*b.icon, setImage: image.as_deref().map_or(std::ptr::null(), |i| i as *const AnyObject)];
        let (frame, size, lines) = if image.is_some() {
            let _: () = msg_send![&*b.icon, setFrame: rect(0.0, d * 0.06, d, d * 0.6)];
            (rect(3.0, d * 0.64, d - 6.0, d * 0.24), (b.radius * 0.3).clamp(7.0, 10.0), 1isize)
        } else {
            (rect(2.0, 0.0, d - 4.0, d), (b.radius * 0.42).clamp(9.0, 15.0), 2isize)
        };
        let _: () = msg_send![&*b.label, setFrame: frame];
        let font: Retained<AnyObject> = msg_send![class!(UIFont), boldSystemFontOfSize: size];
        let _: () = msg_send![&*b.label, setFont: &*font];
        let _: () = msg_send![&*b.label, setNumberOfLines: lines];
    }
    set_hidden(&b.icon, image.is_none());
    set_text(&b.label, &if image.is_some() { text.replace('\n', " ") } else { text.to_string() });
}

fn set_text(l: &AnyObject, text: &str) {
    unsafe { let _: () = msg_send![l, setText: &*NSString::from_str(text)]; }
}
fn set_hidden(v: &AnyObject, hidden: bool) {
    unsafe { let _: () = msg_send![v, setHidden: hidden]; }
}
fn style_button(v: &AnyObject, label: &AnyObject, icon: &AnyObject, state: u8) {
    let (bg, ring, text) = match state {
        2 => (color(1.0, 0.85), 1.0, 0.0),
        1 => (color(1.0, 0.35), 0.9, 1.0),
        _ => (color(0.0, 0.38), 0.55, 1.0),
    };
    unsafe {
        let _: () = msg_send![v, setBackgroundColor: &*bg];
        let layer: *mut AnyObject = msg_send![v, layer];
        let _: () = msg_send![layer, setBorderWidth: 1.5f64];
        let ring = color(1.0, ring);
        let cg: *mut std::ffi::c_void = msg_send![&*ring, CGColor];
        let _: () = msg_send![layer, setBorderColor: cg];
        let _: () = msg_send![label, setTextColor: &*color(text, 0.95)];
        let _: () = msg_send![icon, setTintColor: &*color(text, 0.95)];
    }
}
fn set_alpha_bg(v: &AnyObject, alpha: f64) {
    unsafe { let _: () = msg_send![v, setBackgroundColor: &*color(1.0, alpha)]; }
}
fn move_to(v: &AnyObject, c: (f64, f64)) {
    unsafe { let _: () = msg_send![v, setCenter: CGPoint { x: c.0, y: c.1 }]; }
}

pub fn install(window: &Window) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::UiKit(h) = handle.as_raw() else { return };
    let view = unsafe { &*(h.ui_view.as_ptr() as *const AnyObject) };

    let scale = window.scale_factor();
    let full = window.outer_size().to_logical::<f64>(scale);
    let pos = window.inner_position().map(|p| p.to_logical::<f64>(scale)).unwrap_or_default();
    let inner = window.inner_size().to_logical::<f64>(scale);
    let safe = (pos.x, pos.y, inner.width, inner.height);

    let root = unsafe {
        let v: Retained<AnyObject> = msg_send![class!(UIView), new];
        let _: () = msg_send![&*v, setFrame: rect(0.0, 0.0, full.width, full.height)];
        let _: () = msg_send![&*v, setUserInteractionEnabled: false];
        let _: () = msg_send![view, addSubview: &*v];
        v
    };
    let (sx, sy, sw, sh) = safe;
    let place = |a: Anchor| match a {
        BottomRight(dx, dy) => (sx + sw - dx, sy + sh - dy),
        TopLeft(dx, dy) => (sx + dx, sy + dy),
        TopRight(dx, dy) => (sx + sw - dx, sy + dy),
        Nav(dx, dy) => (sx + sw / 2.0 + dx, sy + 30.0 + dy),
    };
    let buttons = BUTTONS
        .iter()
        .map(|s| {
            let center = place(s.anchor);
            let view = circle(&root, center, s.radius, 1.0, 0.18);
            let label = label(&view, s.radius);
            let icon = icon_view(&view, s.radius);
            Btn { view, label, icon, center, radius: s.radius }
        })
        .collect();
    let stick_home = (sx + 40.0 + STICK_RADIUS, sy + sh - 30.0 - STICK_RADIUS);
    let stick_base = circle(&root, stick_home, STICK_RADIUS, 1.0, 0.12);
    let stick_knob = circle(&root, stick_home, KNOB_RADIUS, 1.0, 0.3);
    let mut o = Overlay {
        root, buttons, stick_base, stick_knob, stick_home, safe, full: (full.width, full.height), scale,
        grips: HashMap::new(), held: HashSet::new(), since: HashMap::new(),
        mouse_since: [std::time::Instant::now(); 2], lingering: false, mouse: 0, aim_latched: false,
        nav: false, in_car: false, mode: FOOT, ctx: 0, menu: false, hidden: false,
        ctrl_since: HashMap::new(), injected: Vec::new(),
    };
    o.relabel();
    eprintln!("[touch] overlay ready ({:.0}x{:.0} pt, safe area {:.0},{:.0} {:.0}x{:.0})", full.width, full.height, sx, sy, sw, sh);
    OVERLAY.with(|cell| *cell.borrow_mut() = Some(o));
}

impl Overlay {
    fn is_wheel(&self, idx: usize) -> bool {
        match self.spec_act(idx).map(|(_, a)| a) {
            Some(Key("Tab")) | Some(Key("AltLeft")) => true,
            Some(Key("KeyQ")) => self.in_car,
            _ => false,
        }
    }

    fn spec_act(&self, idx: usize) -> Option<(&'static str, Act)> {
        let s = &BUTTONS[idx];
        if s.nav && !self.nav { return None; }
        if s.need & self.ctx != s.need || s.unless & self.ctx != 0 { return None; }
        s.acts[self.mode]
    }

    fn relabel(&mut self) {
        for i in 0..self.buttons.len() {
            let act = self.spec_act(i);
            let b = &self.buttons[i];
            set_hidden(&b.view, act.is_none());
            if let Some((text, _)) = act { set_content(b, text); }
        }
        self.restyle();
    }

    fn restyle(&self) {
        let pressed: HashSet<usize> = self.grips.values().filter_map(|g| if let Grip::Button { idx, .. } = g { Some(*idx) } else { None }).collect();
        for (i, b) in self.buttons.iter().enumerate() {
            let latched = self.aim_latched && matches!(self.spec_act(i), Some((_, AimToggle)))
                || self.nav && matches!(self.spec_act(i), Some((_, NavToggle)));
            style_button(&b.view, &b.label, &b.icon, if pressed.contains(&i) { 2 } else if latched { 1 } else { 0 });
        }
    }

    fn button_at(&self, p: (f64, f64)) -> Option<usize> {
        (0..self.buttons.len()).filter(|&i| self.spec_act(i).is_some()).find(|&i| {
            let b = &self.buttons[i];
            let r = b.radius + 8.0;
            (p.0 - b.center.0).powi(2) + (p.1 - b.center.1).powi(2) <= r * r
        })
    }

    fn in_stick_zone(&self, p: (f64, f64)) -> bool {
        let (sx, sy, sw, _) = self.safe;
        p.0 < sx + sw * 0.4 && p.1 > sy + 70.0
    }

    fn cursor_to(&self, p: (f64, f64)) {
        let (w, h) = input::size();
        input::mouse_pos((p.0 / self.full.0 * w as f64) as i32, (p.1 / self.full.1 * h as f64) as i32);
    }

    fn touch(&mut self, t: &Touch) {
        let p = (t.location.x / self.scale, t.location.y / self.scale);
        match t.phase {
            TouchPhase::Started => {
                if let Some(idx) = self.button_at(p) {
                    self.grips.insert(t.id, Grip::Button { idx, start: p, last: p });
                    match self.spec_act(idx).map(|(_, a)| a) {
                        Some(AimToggle) => self.aim_latched = !self.aim_latched,
                        Some(NavToggle) => { self.nav = !self.nav; self.relabel(); }
                        Some(KeyNav(_)) if !self.nav => { self.nav = true; self.relabel(); }
                        Some(Req(r)) => crate::pad::request(r),
                        _ => {}
                    }
                } else if self.menu {
                    self.cursor_to(p);
                    self.grips.insert(t.id, Grip::Cursor { since: std::time::Instant::now(), up: false });
                } else if self.in_stick_zone(p) && !self.grips.values().any(|g| matches!(g, Grip::Stick { .. })) {
                    self.grips.insert(t.id, Grip::Stick { origin: p, at: p });
                } else {
                    self.grips.insert(t.id, Grip::Look { last: p });
                }
            }
            TouchPhase::Moved => {
                let wheel = matches!(self.grips.get(&t.id), Some(Grip::Button { idx, .. }) if self.is_wheel(*idx));
                match self.grips.get_mut(&t.id) {
                    Some(Grip::Stick { at, .. }) => *at = p,
                    Some(Grip::Button { last, .. }) if wheel => *last = p,
                    Some(Grip::Cursor { .. }) => self.cursor_to(p),
                    Some(Grip::Look { last }) | Some(Grip::Button { last, .. }) => {
                        let (dx, dy) = (p.0 - last.0, p.1 - last.1);
                        *last = p;

                        let speed = (dx * dx + dy * dy).sqrt();
                        let gain = LOOK_GAIN * input::look_sens() * (1.0 + (speed / 12.0).min(1.5));
                        input::mouse_delta_f(dx * gain, dy * gain);
                    }
                    None => {}
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => match self.grips.get_mut(&t.id) {
                Some(Grip::Cursor { up, .. }) => *up = true,
                _ => { self.grips.remove(&t.id); }
            },
        }
        self.apply();
    }

    fn apply(&mut self) {
        let mut keys: HashSet<&'static str> = HashSet::new();
        let mut mouse = if self.aim_latched && !self.in_car { 2u8 } else { 0 };
        let mut stick = None;
        let mut horn = false;
        let mut inject: Vec<(i32, f32)> = Vec::new();
        let mut clicking = false;
        self.grips.retain(|_, g| !matches!(g, Grip::Cursor { since, up: true } if since.elapsed() >= CLICK_DELAY + MIN_HOLD));
        for g in self.grips.values() {
            match g {
                Grip::Stick { origin, at } => stick = Some((*origin, *at)),
                Grip::Button { idx, .. } => match self.spec_act(*idx).map(|(_, a)| a) {
                    Some(Key(k)) | Some(KeyNav(k)) => { keys.insert(k); }
                    Some(Mouse(bit)) => mouse |= bit,
                    Some(Horn) => horn = true,
                    Some(Ctrl(c, v)) => inject.push((c, v)),
                    _ => {}
                },
                Grip::Look { .. } => {}
                Grip::Cursor { since, up } => {
                    if since.elapsed() >= CLICK_DELAY { mouse |= 1; }
                    clicking |= *up || since.elapsed() < CLICK_DELAY;
                }
            }
        }
        if let Some((o, at)) = stick {
            let (mut dx, mut dy) = (at.0 - o.0, at.1 - o.1);
            let len = (dx * dx + dy * dy).sqrt();
            let k = if len > STICK_RADIUS { STICK_RADIUS / len } else { 1.0 };
            dx *= k;
            dy *= k;
            let (nx, ny) = (dx / STICK_RADIUS, dy / STICK_RADIUS);
            let dead = |v: f64| if v.abs() < 0.15 { 0.0 } else { v as f32 };
            match self.mode {
                FOOT => {
                    if nx < -0.35 { keys.insert("KeyA"); }
                    if nx > 0.35 { keys.insert("KeyD"); }
                    if ny < -0.35 { keys.insert("KeyW"); }
                    if ny > 0.35 { keys.insert("KeyS"); }
                }
                HELI | PLANE => {
                    inject.push((VEH_FLY_ROLL_LR, dead(nx)));
                    inject.push((VEH_FLY_PITCH_UD, dead(ny)));
                }
                SUB => {
                    inject.push((VEH_SUB_TURN_LR, dead(nx)));
                    inject.push((VEH_SUB_PITCH_UD, dead(ny)));
                }
                _ => {
                    inject.push((VEH_MOVE_LR, dead(nx)));
                    inject.push((VEH_MOVE_UD, dead(ny)));
                }
            }
            let sprint = !self.in_car && len > STICK_RADIUS * SPRINT_REACH;
            if sprint { keys.insert("ShiftLeft"); }
            set_alpha_bg(&self.stick_knob, if sprint { 0.65 } else { 0.3 });
            move_to(&self.stick_base, o);
            move_to(&self.stick_knob, (o.0 + dx, o.1 + dy));
        } else {
            set_alpha_bg(&self.stick_knob, 0.3);
            move_to(&self.stick_base, self.stick_home);
            move_to(&self.stick_knob, self.stick_home);
        }
        crate::pad::HORN_TOUCH.store(horn, std::sync::atomic::Ordering::Relaxed);
        let now = std::time::Instant::now();
        let young = |t: Option<&std::time::Instant>| t.is_some_and(|t| now - *t < MIN_HOLD);
        self.lingering = clicking;
        for k in self.held.clone() {
            if keys.contains(k) { continue; }
            if young(self.since.get(k)) { keys.insert(k); self.lingering = true; continue; }
            input::key(k, false, false, None, false);
            self.since.remove(k);
        }
        for k in keys.difference(&self.held) {
            crate::mp::key_pressed(k);
            input::key(k, true, false, None, false);
            self.since.insert(k, now);
        }
        self.held = keys;
        for (i, (bit, dom)) in [(1u8, 0usize), (2, 2)].into_iter().enumerate() {
            if self.mouse & bit != 0 && mouse & bit == 0 && young(Some(&self.mouse_since[i])) {
                mouse |= bit;
                self.lingering = true;
            }
            if (mouse ^ self.mouse) & bit != 0 {
                input::button(dom, mouse & bit != 0);
                if mouse & bit != 0 { self.mouse_since[i] = now; }
            }
        }
        self.mouse = mouse;
        for (c, v) in self.injected.clone() {
            if v == 0.0 || inject.iter().any(|(k, _)| *k == c) { continue; }
            let hold = if c == VEH_ROOF { ROOF_HOLD } else { MIN_HOLD };
            if self.ctrl_since.get(&c).is_some_and(|t| now - *t < hold) { inject.push((c, v)); self.lingering = true; }
        }
        for (c, _) in &inject {
            if !self.injected.iter().any(|(k, _)| k == c) { self.ctrl_since.insert(*c, now); }
        }
        *crate::pad::INJECT.lock().unwrap() = inject.clone();
        self.injected = inject;
        self.restyle();
    }

    fn release_all(&mut self) {
        self.grips.clear();
        self.aim_latched = false;
        self.apply();
    }
}

pub fn touch(t: &Touch) {
    OVERLAY.with(|cell| {
        if let Some(o) = cell.borrow_mut().as_mut() {
            if !o.hidden { o.touch(t); }
        }
    });
}

pub fn refresh() -> bool {
    OVERLAY.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(o) = guard.as_mut() else { return false };
        fps_tick(o);
        stop_tick(o);
        if o.lingering { o.apply(); }
        let mut wheeling = false;
        let mut radio = None;
        for g in o.grips.values() {
            if let Grip::Button { idx, start, last } = g {
                if !o.is_wheel(*idx) {
                    continue;
                }
                wheeling = true;
                let (dx, dy) = (last.0 - start.0, last.1 - start.1);
                let len = (dx * dx + dy * dy).sqrt();
                let is_radio = matches!(o.spec_act(*idx), Some((_, Key("KeyQ"))));
                if len > WHEEL_DEADZONE {
                    let amount = ((len - WHEEL_DEADZONE) / WHEEL_FULL).min(1.0);
                    if is_radio {
                        radio = Some(((dx / len * amount) as f32, (dy / len * amount) as f32));
                    } else {
                        let push = WHEEL_PUSH * amount;
                        input::mouse_delta_f(dx / len * push, dy / len * push);
                    }
                }
            }
        }
        *crate::pad::RADIO_WHEEL.lock().unwrap() = radio;
        let hide = crate::pad::ACTIVE.load(std::sync::atomic::Ordering::Relaxed);

        if !hide { input::set_locked(true); }
        if !crate::gpu::SUSPENDED.load(std::sync::atomic::Ordering::Relaxed) { input::set_focus(true); }
        if hide != o.hidden {
            o.hidden = hide;
            if hide { o.release_all(); }
            set_hidden(&o.root, hide);
        }
        let menu = crate::pad::in_menu();
        if menu != o.menu && !o.hidden {
            o.grips.retain(|_, g| matches!(g, Grip::Button { .. }));
            o.menu = menu;
            eprintln!("[touch] menu cursor {}", if menu { "on" } else { "off" });
            o.apply();
        }
        let mode = (crate::pad::VEH_MODE.load(std::sync::atomic::Ordering::Relaxed) as usize).min(MODES - 1);
        let ctx = crate::pad::CONTEXT.load(std::sync::atomic::Ordering::Relaxed);
        if ctx != o.ctx && mode == o.mode && !o.hidden {
            o.ctx = ctx;
            o.relabel();
        }
        if mode != o.mode && !o.hidden {
            o.ctx = ctx;
            o.grips.retain(|_, g| !matches!(g, Grip::Button { .. }));
            o.mode = mode;
            o.in_car = mode != FOOT;
            if o.in_car { o.aim_latched = false; }
            o.apply();
            o.relabel();
        }
        o.lingering || wheeling
    })
}

pub fn release() {
    OVERLAY.with(|cell| { if let Some(o) = cell.borrow_mut().as_mut() { o.release_all(); } });
}
