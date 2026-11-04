//! The controls a phone plays with, taken from the layout file the editor writes.
//!
//! A layout is a list of elements — buttons, a movement stick — each placed by an
//! expression rather than a number, so `${right} - ${margin} * 2 - ${width}` means
//! "in the corner, two margins in" on a phone of any size. That format, the dp
//! sizes, the per-button opacity and roundness, the held-versus-toggled keys and
//! the gestures of the bare glass (hold still to dig, tap to place, a second
//! finger for the other button) are the ones the Android launchers that came
//! before this one settled on, and the editor in `packaging/android` writes the
//! same file, so what a player arranges is what the game answers with.
//!
//! The buttons are drawn by the renderer rather than by a window of the launcher,
//! because the surface a `NativeActivity` is given is ordered above its own
//! window: a control laid over it is hidden by the game, and a window of its own
//! can be seen but never touched.

use super::event_loop::EventLoopCtl;
use super::{Game, Screen};
use crate::ui_theme::{Face, Ui};
use std::time::{Duration, Instant};
use winit::event::{DeviceId, ElementState, MouseButton, Touch, TouchPhase, WindowEvent};
use winit::window::WindowId;

/// How far a finger may travel before it stops counting as a tap, in pixels.
const TAP_SLOP: f64 = 22.0;
/// A press shorter than this, and still, is the right button rather than a dig.
const TAP: Duration = Duration::from_millis(150);
/// The stick has to be pushed this far before it walks, as a fraction of itself.
const DEADZONE: f32 = 0.2;

/// One control, as the layout file describes it.
#[derive(Clone, serde::Deserialize)]
struct Saved {
    #[serde(default)]
    name: String,
    #[serde(default)]
    keycodes: Vec<i32>,
    /// A stick's keys are stored under `properties` instead.
    #[serde(default)]
    properties: Vec<i32>,
    #[serde(rename = "dynamicX", default)]
    x: String,
    #[serde(rename = "dynamicY", default)]
    y: String,
    #[serde(default)]
    width: f32,
    #[serde(default)]
    height: f32,
    #[serde(default)]
    opacity: f32,
    #[serde(rename = "cornerRadius", default)]
    corner: f32,
    #[serde(rename = "bgColor", default = "grey")]
    /// A layout file writes ARGB as a Java `int`, which is signed, so it arrives
    /// as a negative number for any opaque colour.
    colour: i64,
    #[serde(rename = "strokeColor", default = "edge")]
    stroke: i64,
    #[serde(rename = "strokeWidth", default = "one")]
    stroke_width: f32,
    #[serde(rename = "isToggle", default)]
    toggle: bool,
    #[serde(rename = "isSwipeable", default)]
    swipeable: bool,
    #[serde(rename = "displayInGame", default = "yes")]
    in_game: bool,
    #[serde(rename = "displayInMenu", default)]
    in_menu: bool,
    #[serde(rename = "forwardLock", default)]
    forward_lock: bool,
}

fn grey() -> i64 {
    0xFF80_8080
}

/// The plate's outline: white, at the plate's own opacity, as every launcher of
/// this kind draws it.
fn edge() -> i64 {
    0xFFFF_FFFF
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

/// A control placed for this screen, in the surface's own pixels.
struct Pad {
    name: String,
    keys: Vec<i32>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    opacity: f32,
    corner: f32,
    colour: u32,
    stroke: u32,
    stroke_w: f32,
    toggle: bool,
    /// A finger sliding off this plate onto its neighbour presses the neighbour:
    /// how a cross of walking plates is used with one thumb.
    swipeable: bool,
    forward_lock: bool,
    /// Shown while the world has the pointer, and while a screen has it.
    in_game: bool,
    in_menu: bool,
    /// The four keys of a stick, in the order forward, left, back, right.
    stick: bool,
}

/// The three settings the controls answer to that are not in the layout file:
/// how big every plate is drawn, how long a finger rests on bare glass before it
/// becomes the left button, and whether the glass does anything at all. The
/// drawer changes them while a world is loaded, so they are carried in and out by
/// hand rather than read back through this module's own state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// A multiple of the sizes the layout saved.
    pub scale: f32,
    /// The hold that turns a still finger into a dig, in milliseconds.
    pub long_press: u64,
    pub gestures: bool,
}

/// The touch layer, between one report and the next.
pub struct Touches {
    /// The file as it was read, kept so a rotation can place it again.
    layout: Option<Layout>,
    pads: Vec<Pad>,
    fingers: Vec<Finger>,
    /// The finger that is the pointer, or the camera.
    first: Option<u64>,
    /// A control that is held down, and the finger holding it.
    pressed: Option<(usize, u64)>,
    /// A toggle that was tapped and stays down until it is tapped again.
    stuck: Vec<usize>,
    /// The stick's knob, for as long as its finger is on the glass.
    knob: Option<(f32, f32)>,
    stick: Option<usize>,
    held: Vec<usize>,
    long_press: Duration,
    gestures: bool,
    /// The screen's size and the device's density, which the layout needs.
    surface: (f32, f32),
    density: f32,
    scale: f32,
}

/// One finger, and what it has been made to mean so far.
struct Finger {
    id: u64,
    down: Instant,
    start: (f64, f64),
    last: (f64, f64),
    moved: bool,
    /// The left button was started by holding still and is still held.
    digging: bool,
    /// This finger is the other button, because another one already looks.
    other: bool,
    /// The control this finger is on.
    pad: Option<usize>,
}

impl Default for Touches {
    fn default() -> Self {
        let number = |key: &str, when_absent: f64| {
            std::env::var(key)
                .ok()
                .and_then(|text| text.trim().parse::<f64>().ok())
                .unwrap_or(when_absent)
        };
        Self {
            layout: None,
            pads: Vec::new(),
            fingers: Vec::new(),
            first: None,
            pressed: None,
            stuck: Vec::new(),
            knob: None,
            stick: None,
            held: Vec::new(),
            long_press: Duration::from_millis(number("CRYON_LONG_PRESS", 300.0) as u64),
            gestures: std::env::var("CRYON_GESTURES")
                .map(|text| text != "0")
                .unwrap_or(true),
            surface: (0.0, 0.0),
            density: number("CRYON_DENSITY", 3.0) as f32,
            scale: number("CRYON_BUTTON_SCALE", 100.0) as f32 / 100.0,
        }
    }
}

impl Touches {
    /// The layout the launcher chose, if it is on the device.
    fn read(&mut self) {
        let Some(path) = std::env::var_os("CRYON_CONTROLS") else {
            return;
        };
        let file = std::path::Path::new(&path);
        let Ok(text) = std::fs::read_to_string(file) else {
            log::warn!("Cryon: no control layout at {file:?}");
            return;
        };
        match serde_json::from_str::<Layout>(&text) {
            Ok(layout) => {
                self.layout = Some(layout);
                self.place();
            }
            Err(bad) => log::warn!("Cryon: control layout not read: {bad}"),
        }
    }

    /// Put every control where its expression says for the screen we have. The
    /// first frame knows the size and the layout file was read before it, so this
    /// is called again whenever the two change hands.
    fn place(&mut self) {
        let Some(layout) = self.layout.clone() else { return };
        self.pads = layout.place(self.surface, self.density, self.scale);
    }

    /// The screen the layout is placed against. The file is read once, but a
    /// `${bottom}` means nothing until the frame says how tall the screen is —
    /// which is why every control but the top-left one was drawn off-screen the
    /// first time this ran.
    fn resize(&mut self, surface: (f32, f32)) {
        if surface.0 <= 0.0 || self.surface == surface {
            return;
        }
        let first = self.surface == (0.0, 0.0);
        self.surface = surface;
        if first {
            self.read();
        } else {
            self.place();
        }
    }

    /// The settings the drawer reads, taken from the state we are already in.
    fn tuning(&self) -> Tuning {
        Tuning {
            scale: self.scale,
            long_press: self.long_press.as_millis() as u64,
            gestures: self.gestures,
        }
    }

    /// Put them back, and lay the controls out again if the plates changed size.
    fn apply(&mut self, tune: &Tuning) {
        self.gestures = tune.gestures;
        self.long_press = Duration::from_millis(tune.long_press);
        let scale = tune.scale.clamp(0.5, 2.0);
        if scale == self.scale {
            return;
        }
        self.scale = scale;
        self.place();
    }

    /// Turns one reported finger into the events the game already reads.
    fn report(
        &mut self,
        game: &mut Game,
        event_loop: &dyn EventLoopCtl,
        id: WindowId,
        finger: &Touch,
    ) {
        let screen = (
            game.renderer.as_ref().map_or(0, |r| r.config.width),
            game.renderer.as_ref().map_or(0, |r| r.config.height),
        );
        self.resize((screen.0 as f32, screen.1 as f32));
        // The drawer comes first: a finger on its handle or on the panel is
        // changing a setting, and must not also walk, look or click something
        // underneath it. The three switches it can reach belong to this layer, so
        // they travel by hand rather than by another borrow of the same state —
        // the drawer runs inside this one's borrow, and asking the module for them
        // here would be asking a `RefCell` to borrow itself twice.
        let before = self.tuning();
        let mut tune = before;
        let kept = super::quickset::finger(game, finger, self.surface, &mut tune);
        if tune != before {
            self.apply(&tune);
        }
        // "Force close" is the list's first word, and only the event loop can
        // leave: the drawer is asked from inside this borrow and cannot hold it.
        if super::quickset::wants_exit() {
            event_loop.exit();
        }
        if kept {
            return;
        }
        if traced() {
            let world = in_world(game);
            log::info!(
                "Cryon: touch {:?} #{} at ({:.0}, {:.0}) of {}x{} world={} hit={:?} of {}",
                finger.phase,
                finger.id,
                finger.location.x,
                finger.location.y,
                screen.0,
                screen.1,
                world,
                self.hit((finger.location.x, finger.location.y), world),
                self.pads.len()
            );
        }
        // winit cannot grab the pointer on a phone, and the world answers a click
        // that arrives while the pointer is free by trying to grab it and dropping
        // the click. Say it is grabbed, since a finger in the world is looked
        // through a crosshair and not aimed at a widget.
        if in_world(game) {
            game.captured = true;
        }
        match finger.phase {
            TouchPhase::Started => self.start(game, event_loop, id, finger),
            TouchPhase::Moved => self.drag(game, event_loop, id, finger),
            TouchPhase::Ended => self.lift(game, event_loop, id, finger, false),
            TouchPhase::Cancelled => self.lift(game, event_loop, id, finger, true),
        }
    }

    fn start(&mut self, game: &mut Game, event_loop: &dyn EventLoopCtl, id: WindowId, finger: &Touch) {
        let at = (finger.location.x, finger.location.y);
        let world = in_world(game);
        let pad = self.hit(at, world);
        self.fingers.push(Finger {
            id: finger.id,
            down: Instant::now(),
            start: at,
            last: at,
            moved: false,
            digging: false,
            other: false,
            pad,
        });
        if let Some(index) = pad {
            if self.pads[index].stick {
                self.stick = Some(index);
                self.knob = Some((0.0, 0.0));
                return;
            }
            self.press(index, game, event_loop, id, finger.id);
            return;
        }
        if !world {
            // Over a menu the finger is the pointer, and the widget under it is
            // what the player means. The position comes first, so the screen knows
            // what it is being clicked on.
            self.first = Some(finger.id);
            send(game, event_loop, id, cursor(finger.location));
            return;
        }
        if self.first.is_some() && self.first != Some(finger.id) {
            // A second finger is the other mouse button, so digging and placing at
            // once — a pillar, a tree — is possible with two thumbs.
            if let Some(last) = self.fingers.last_mut() {
                last.other = true;
            }
            send(game, event_loop, id, mouse(MouseButton::Right, ElementState::Pressed));
            return;
        }
        self.first = Some(finger.id);
    }

    fn drag(&mut self, game: &mut Game, event_loop: &dyn EventLoopCtl, id: WindowId, finger: &Touch) {
        let at = (finger.location.x, finger.location.y);
        let world = in_world(game);
        // A finger that began on a walking plate and slid onto its neighbour: the
        // old direction lets go and the new one is taken. This is how a cross of
        // plates is walked with one thumb, rather than lifted and re-pressed for
        // every turn.
        let sliding = self
            .fingers
            .iter()
            .find(|f| f.id == finger.id)
            .and_then(|f| f.pad)
            .filter(|&index| {
                self.pads
                    .get(index)
                    .is_some_and(|pad| pad.swipeable && !pad.stick)
            });
        if let Some(from) = sliding {
            if let Some(to) = self.hit(at, world) {
                if to != from && self.pads[to].swipeable {
                    self.pressed = Some((to, finger.id));
                    self.keys(game, event_loop, id, from, false);
                    self.keys(game, event_loop, id, to, true);
                    if let Some(tracked) = self.fingers.iter_mut().find(|f| f.id == finger.id) {
                        tracked.pad = Some(to);
                    }
                }
            }
        }
        let Some(tracked) = self.fingers.iter_mut().find(|f| f.id == finger.id) else {
            return;
        };
        let (dx, dy) = (at.0 - tracked.last.0, at.1 - tracked.last.1);
        tracked.last = at;
        if (at.0 - tracked.start.0).hypot(at.1 - tracked.start.1) > TAP_SLOP {
            tracked.moved = true;
            if tracked.digging {
                tracked.digging = false;
                send(game, event_loop, id, mouse(MouseButton::Left, ElementState::Released));
            }
        }
        let stick = tracked.pad.is_some_and(|index| self.pads[index].stick);
        if stick {
            let index = tracked.pad.unwrap();
            let centre = (
                self.pads[index].x + self.pads[index].w / 2.0,
                self.pads[index].y + self.pads[index].h / 2.0,
            );
            let reach = (self.pads[index].w / 2.0).min(self.pads[index].h / 2.0).max(1.0);
            self.knob = Some((
                ((at.0 as f32 - centre.0) / reach).clamp(-1.0, 1.0),
                ((at.1 as f32 - centre.1) / reach).clamp(-1.0, 1.0),
            ));
            self.walk(game);
            return;
        }
        if !in_world(game) {
            if self.first == Some(finger.id) {
                send(game, event_loop, id, cursor(finger.location));
            }
            return;
        }
        if tracked.other || self.first != Some(finger.id) {
            return;
        }
        // The same accumulator a real mouse feeds, so the player's sensitivity,
        // smooth camera and invert all still apply to a drag of the screen.
        game.mouse.add_motion(dx, dy);
    }

    fn lift(
        &mut self,
        game: &mut Game,
        event_loop: &dyn EventLoopCtl,
        id: WindowId,
        finger: &Touch,
        cancelled: bool,
    ) {
        let Some(index) = self.fingers.iter().position(|f| f.id == finger.id) else {
            return;
        };
        let tracked = self.fingers.remove(index);
        if tracked.other {
            send(game, event_loop, id, mouse(MouseButton::Right, ElementState::Released));
            return;
        }
        if let Some(pad) = tracked.pad {
            if self.pads[pad].stick {
                self.knob = None;
                self.stick = None;
                self.walk(game);
            } else if self.pressed == Some((pad, finger.id)) {
                self.pressed = None;
                if !self.pads[pad].toggle {
                    self.keys(game, event_loop, id, pad, false);
                }
            }
        }
        if tracked.digging {
            send(game, event_loop, id, mouse(MouseButton::Left, ElementState::Released));
        }
        if self.first == Some(finger.id) {
            self.first = self.fingers.first().map(|next| next.id);
        }
        if cancelled || tracked.pad.is_some() {
            return;
        }
        if !in_world(game) {
            // A lift over a menu is the click: the widget the pointer was moved
            // onto, pressed and released where the finger stands.
            send(game, event_loop, id, cursor(finger.location));
            send(game, event_loop, id, mouse(MouseButton::Left, ElementState::Pressed));
            send(game, event_loop, id, mouse(MouseButton::Left, ElementState::Released));
            return;
        }
        if !self.gestures || tracked.moved || tracked.down.elapsed() >= TAP {
            return;
        }
        // A tap too quick to have become a dig is the right button.
        send(game, event_loop, id, mouse(MouseButton::Right, ElementState::Pressed));
        send(game, event_loop, id, mouse(MouseButton::Right, ElementState::Released));
    }

    /// Called between frames: a finger that has not moved sends no events at all,
    /// so the dig it has become has to be noticed here.
    fn tick(&mut self, game: &mut Game, event_loop: &dyn EventLoopCtl, id: WindowId) {
        if !in_world(game) || !self.gestures {
            return;
        }
        for finger in self.fingers.iter_mut() {
            if finger.pad.is_some() || finger.other || finger.digging || finger.moved {
                continue;
            }
            if finger.down.elapsed() >= self.long_press {
                finger.digging = true;
                send(game, event_loop, id, mouse(MouseButton::Left, ElementState::Pressed));
            }
        }
    }

    /// Press a control: a toggle stays down until it is tapped again.
    fn press(
        &mut self,
        index: usize,
        game: &mut Game,
        event_loop: &dyn EventLoopCtl,
        id: WindowId,
        finger: u64,
    ) {
        if self.pads[index].toggle {
            let stuck = self.stuck.contains(&index);
            if stuck {
                self.stuck.retain(|&held| held != index);
            } else {
                self.stuck.push(index);
            }
            self.keys(game, event_loop, id, index, !stuck);
            return;
        }
        self.pressed = Some((index, finger));
        self.keys(game, event_loop, id, index, true);
    }

    /// Send every key and button one control names. A control may hold a key and a
    /// mouse button at once, which is how a layout gives "use item" or "pick block"
    /// without the player aiming the glass at anything.
    fn keys(
        &mut self,
        game: &mut Game,
        event_loop: &dyn EventLoopCtl,
        id: WindowId,
        index: usize,
        down: bool,
    ) {
        for key in self.pads[index].keys.clone() {
            // A layout names an unused slot 0 and the mouse, the wheel and the
            // escape with negative numbers rather than button indices, so those
            // are read here as the hand they stand for. The negatives that name
            // the launcher's own jobs — opening the typing, hiding the controls,
            // the virtual mouse — are the launcher's to answer, not the game's.
            let (key, wheel) = match key {
                0 => continue,
                -3 => (0, 0.0),
                -4 => (1, 0.0),
                -6 => (2, 0.0),
                -9 => (256, 0.0),
                -7 => (0, 1.0),
                -8 => (0, -1.0),
                -1 | -2 | -5 => return,
                other => (other, 0.0),
            };
            if wheel != 0.0 {
                send(
                    game,
                    event_loop,
                    id,
                    wheel_steps(if down { wheel } else { 0.0 }),
                );
                continue;
            }
            if (0..=4).contains(&key) {
                let button = match key {
                    0 => MouseButton::Left,
                    1 => MouseButton::Right,
                    2 => MouseButton::Middle,
                    3 => MouseButton::Back,
                    _ => MouseButton::Forward,
                };
                send(game, event_loop, id, mouse(button, pressed(down)));
                continue;
            }
            if let Some(code) = glfw(key) {
                hold(game, code, down);
            }
        }
    }

    /// Move with the stick. A lifted finger lets go of all four directions, so a
    /// finger leaving the screen never leaves the player walking.
    fn walk(&mut self, game: &mut Game) {
        let Some(index) = self.stick else { return };
        let wanted = match self.knob {
            Some((x, y)) => [
                y < -DEADZONE,
                x < -DEADZONE,
                y > DEADZONE,
                x > DEADZONE,
            ],
            None => [false; 4],
        };
        let keys = self.pads[index].keys.clone();
        for (slot, &want) in wanted.iter().enumerate() {
            let held = self.held.contains(&slot);
            if want == held {
                continue;
            }
            if want {
                self.held.push(slot);
            } else {
                self.held.retain(|&held| held != slot);
            }
            if let Some(&key) = keys.get(slot) {
                if let Some(code) = glfw(key) {
                    hold(game, code, want);
                }
            }
        }
    }

    /// The control under a finger, if the player can see one there.
    fn hit(&self, at: (f64, f64), world: bool) -> Option<usize> {
        let (x, y) = (at.0 as f32, at.1 as f32);
        self.pads.iter().enumerate().find_map(|(index, pad)| {
            let shown = if world { pad.in_game } else { pad.in_menu };
            if shown
                && x >= pad.x
                && x < pad.x + pad.w
                && y >= pad.y
                && y < pad.y + pad.h
            {
                Some(index)
            } else {
                None
            }
        })
    }

    /// Draw the controls over the frame, in the design system's own units.
    pub fn draw(&mut self, ui: &mut Ui, world: bool) {
        // The same density `Ui::new` measures in, so a control is drawn where the
        // finger that presses it is measured.
        let step = (self.surface.0 / 1040.0)
            .min(self.surface.1 / 700.0)
            .clamp(1.0, 4.0);
        for (number, pad) in self.pads.iter().enumerate() {
            let shown = if world { pad.in_game } else { pad.in_menu };
            if !shown {
                continue;
            }
            // A plate that is held, or a toggle that is on, lights up: the same
            // word for it Pojav uses, where a pressed plate takes the accent at
            // half strength so you can see which of them the thumb is on.
            let active = self.pressed.is_some_and(|(index, _)| index == number)
                || self.stuck.contains(&number);
            let (x, y, w, h) = (pad.x / step, pad.y / step, pad.w / step, pad.h / step);
            let radius = (w.min(h) / 2.0 * (pad.corner / 100.0)).min(w.min(h) / 2.0);
            let alpha = (pad.opacity.clamp(0.05, 1.0) * 255.0) as u32;
            let line = (alpha << 24) | (pad.stroke & 0x00FF_FFFF);
            if pad.stick {
                // A ring with a knob in it, and the knob sits in the middle until
                // a finger moves it: the player reads the ring as the walking
                // place without having to touch it first to find out.
                ui.rounded(x, y, w, h, radius, ((alpha / 3) << 24) | (pad.colour & 0x00FF_FFFF));
                ui.border(x, y, w, h, radius, line);
                let size = w.min(h) * 0.36;
                let (kx, ky) = self.knob.unwrap_or((0.0, 0.0));
                ui.rounded(
                    x + (w - size) / 2.0 + kx * (w - size) / 2.0,
                    y + (h - size) / 2.0 + ky * (h - size) / 2.0,
                    size,
                    size,
                    size / 2.0,
                    ((alpha * 3 / 4).min(255) << 24) | 0x00FF_FFFF,
                );
                continue;
            }
            let plate = if active {
                (128 << 24) | (crate::ui_theme::color::ACCENT & 0x00FF_FFFF)
            } else {
                (alpha << 24) | (pad.colour & 0x00FF_FFFF)
            };
            ui.rounded(x, y, w, h, radius, plate);
            if pad.stroke_w > 0.0 {
                ui.border(x, y, w, h, radius, line);
            }
            let marked = mark(
                ui,
                pad.keys.first().copied().unwrap_or(0),
                x,
                y,
                w,
                h,
                line,
                alpha,
            );
            if !marked && !pad.name.is_empty() {
                ui.text_centered(
                    Face::Medium,
                    crate::ui_theme::SMALL,
                    x + w / 2.0,
                    y + h / 2.0 - 2.0,
                    0xFFFF_FFFF,
                    &pad.name.to_uppercase(),
                );
            }
        }
    }
}
/// The layout file: the elements, in the names every launcher of this kind uses.
#[derive(Clone, serde::Deserialize)]
struct Layout {
    #[serde(rename = "mControlDataList", default)]
    controls: Vec<Saved>,
    #[serde(rename = "mJoystickDataList", default)]
    sticks: Vec<Saved>,
}

impl Layout {
    /// Place every element for this screen, in the surface's pixels.
    fn place(&self, surface: (f32, f32), density: f32, scale: f32) -> Vec<Pad> {
        let mut pads = Vec::new();
        for (saved, stick) in self
            .controls
            .iter()
            .map(|s| (s, false))
            .chain(self.sticks.iter().map(|s| (s, true)))
        {
            let w = (saved.width.max(8.0) * density * scale).round();
            let h = (saved.height.max(8.0) * density * scale).round();
            pads.push(Pad {
                name: saved.name.clone(),
                keys: if stick {
                    saved.properties.clone()
                } else {
                    saved.keycodes.clone()
                },
                x: anchor(&saved.x, surface.0, surface.1, density, w, h),
                y: anchor(&saved.y, surface.0, surface.1, density, w, h),
                w,
                h,
                opacity: if saved.opacity == 0.0 {
                    0.4
                } else {
                    saved.opacity
                },
                corner: saved.corner,
                colour: saved.colour as u32,
                stroke: saved.stroke as u32,
                stroke_w: saved.stroke_width,
                toggle: saved.toggle,
                swipeable: saved.swipeable,
                forward_lock: saved.forward_lock,
                in_game: saved.in_game,
                in_menu: saved.in_menu,
                stick,
            });
        }
        pads
    }
}


///
/// The little arithmetic a layout is written in: `${right} - ${margin} * 2 -
/// ${width}` is "in the corner, two margins in", and `px(24)` says pixels where a
/// bare number says dp. The names are the ones a layout file already uses, so a
/// layout made elsewhere is read here without being rewritten.
fn anchor(text: &str, width: f32, height: f32, density: f32, own: f32, own_height: f32) -> f32 {
    let names = [
        ("margin", 2.0 * density),
        ("top", 0.0),
        ("left", 0.0),
        // `${right}` and `${bottom}` are the furthest an element's own corner may
        // sit, not the screen's edge, which is what a layout written elsewhere
        // means when it says `${bottom} - ${margin}`: one margin above the bottom,
        // with the whole plate on the screen.
        ("right", width - own),
        ("bottom", height - own_height),
        ("screen_width", width),
        ("screen_height", height),
        ("width", own),
        ("height", own_height),
        ("preferred_scale", density),
    ];
    Expr::new(text, &names, density).sum()
}

/// A term at a time, over the characters of one expression.
struct Expr<'a> {
    text: &'a [u8],
    at: usize,
    names: &'a [(&'a str, f32)],
    density: f32,
}

impl<'a> Expr<'a> {
    fn new(text: &'a str, names: &'a [(&'a str, f32)], density: f32) -> Self {
        Self {
            text: text.as_bytes(),
            at: 0,
            names,
            density,
        }
    }

    fn sum(&mut self) -> f32 {
        let mut value = self.term();
        loop {
            self.skip();
            if self.accept(b'+') {
                value += self.term();
            } else if self.accept(b'-') {
                value -= self.term();
            } else {
                return value;
            }
        }
    }

    fn term(&mut self) -> f32 {
        let mut value = self.factor();
        loop {
            self.skip();
            if self.accept(b'*') {
                value *= self.factor();
            } else if self.accept(b'/') {
                value /= self.factor();
            } else {
                return value;
            }
        }
    }

    fn factor(&mut self) -> f32 {
        self.skip();
        if self.accept(b'-') {
            return -self.factor();
        }
        if self.accept(b'(') {
            let value = self.sum();
            self.skip();
            self.accept(b')');
            return value;
        }
        if self.accept(b'$') {
            return self.name();
        }
        if self.word("dp(") {
            let value = self.number() * self.density;
            self.skip();
            self.accept(b')');
            return value;
        }
        if self.word("px(") {
            let value = self.number();
            self.skip();
            self.accept(b')');
            return value;
        }
        self.number()
    }

    /// `${name}`, with the braces a layout always writes.
    fn name(&mut self) -> f32 {
        self.skip();
        if !self.accept(b'{') {
            return 0.0;
        }
        let start = self.at;
        while self.at < self.text.len() && self.text[self.at] != b'}' {
            self.at += 1;
        }
        let word = std::str::from_utf8(&self.text[start..self.at]).unwrap_or("");
        self.accept(b'}');
        self.names
            .iter()
            .find(|(known, _)| *known == word.trim())
            .map_or(0.0, |(_, value)| *value)
    }

    fn number(&mut self) -> f32 {
        self.skip();
        let start = self.at;
        while self.at < self.text.len()
            && (self.text[self.at].is_ascii_digit() || self.text[self.at] == b'.')
        {
            self.at += 1;
        }
        if start == self.at {
            return 0.0;
        }
        std::str::from_utf8(&self.text[start..self.at])
            .ok()
            .and_then(|text| text.parse().ok())
            .unwrap_or(0.0)
    }

    fn word(&mut self, want: &str) -> bool {
        self.skip();
        if self.text[self.at.min(self.text.len())..].starts_with(want.as_bytes()) {
            self.at += want.len();
            return true;
        }
        false
    }

    fn skip(&mut self) {
        while self.at < self.text.len() && self.text[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn accept(&mut self, byte: u8) -> bool {
        self.skip();
        if self.at < self.text.len() && self.text[self.at] == byte {
            self.at += 1;
            return true;
        }
        false
    }
}

/// Whether the world has the pointer, which is the same question as whether a
/// finger is a camera or a cursor.
fn in_world(game: &Game) -> bool {
    game.screen == Screen::Playing && game.player.is_some()
}

/// `CRYON_TOUCH_DEBUG=1` logs every finger and which control it landed on: a
/// button that does not answer is either in the wrong place or not being read,
/// and the two look identical from the outside.
fn traced() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CRYON_TOUCH_DEBUG").is_some())
}

// Which finger is doing what, kept here rather than on the game: the phone's
// fingers belong to the thread that reads them, and nothing else has a reason to
// know they exist.
thread_local! {
    static FINGERS: std::cell::RefCell<Touches> =
        std::cell::RefCell::new(Touches::default());
}

/// One reported finger, from the phone's event loop.
pub fn report(game: &mut Game, event_loop: &dyn EventLoopCtl, id: WindowId, finger: &Touch) {
    FINGERS.with(|state| state.borrow_mut().report(game, event_loop, id, finger));
}

/// Between frames, so a finger that has not moved still becomes a dig.
pub fn tick(game: &mut Game, event_loop: &dyn EventLoopCtl, id: WindowId) {
    FINGERS.with(|state| state.borrow_mut().tick(game, event_loop, id));
}

/// Draw the controls over a frame, against the screen that frame is on.
pub fn draw(ui: &mut Ui, world: bool, surface: (f32, f32)) {
    FINGERS.with(|state| {
        let mut touches = state.borrow_mut();
        touches.resize(surface);
        touches.draw(ui, world);
    });
}


fn send(game: &mut Game, event_loop: &dyn EventLoopCtl, id: WindowId, event: WindowEvent) {
    game.handle_window_event(event_loop, id, event);
}

/// A pointer position, in the pixels the game's own surface is measured in.
fn cursor(at: winit::dpi::PhysicalPosition<f64>) -> WindowEvent {
    WindowEvent::CursorMoved {
        device_id: DeviceId::dummy(),
        position: at,
    }
}

fn mouse(button: MouseButton, state: ElementState) -> WindowEvent {
    WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        button,
        state,
    }
}

fn pressed(down: bool) -> ElementState {
    if down {
        ElementState::Pressed
    } else {
        ElementState::Released
    }
}

/// The wheel, in notches, for a control that names it instead of a key.
fn wheel_steps(notches: f32) -> WindowEvent {
    WindowEvent::MouseWheel {
        device_id: DeviceId::dummy(),
        delta: winit::event::MouseScrollDelta::LineDelta(0.0, notches),
        phase: TouchPhase::Moved,
    }
}

/// The mark a control shows instead of a word: a mouse with the half this button
/// presses filled in, or a keyboard. This is how the plates read at a glance on a
/// phone, where the label would be the only thing small enough to squint at.
/// Returns whether it drew one, so the word is only used when there is no mark.
fn mark(
    ui: &mut Ui,
    key: i32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    line: u32,
    alpha: u32,
) -> bool {
    let lit = ((alpha * 3 / 4).min(255) << 24) | 0x00FF_FFFF;
    if key == -1 {
        // The typing board: a case with two rows of keys in it.
        let (bw, bh) = (w * 0.58, h * 0.38);
        let (bx, by) = (x + (w - bw) / 2.0, y + (h - bh) / 2.0);
        ui.rounded(bx, by, bw, bh, bh * 0.22, ((alpha / 2) << 24) | 0x00FF_FFFF);
        ui.border(bx, by, bw, bh, bh * 0.22, line);
        for row in 0..2 {
            for column in 0..4 {
                ui.rounded(
                    bx + bw * 0.12 + column as f32 * bw * 0.2,
                    by + bh * 0.22 + row as f32 * bh * 0.42,
                    bw * 0.13,
                    bh * 0.26,
                    0.0,
                    lit,
                );
            }
        }
        return true;
    }
    let (left, right, middle) = match key {
        -3 => (true, false, false),
        -4 => (false, true, false),
        -6 => (false, false, true),
        _ => return false,
    };
    // The mouse: an outlined body, with the half this button presses filled.
    let (mw, mh) = (w * 0.36, h * 0.54);
    let (mx, my) = (x + (w - mw) / 2.0, y + (h - mh) / 2.0);
    let radius = mw * 0.3;
    ui.border(mx, my, mw, mh, radius, line);
    let half = mw / 2.0;
    if left || middle {
        ui.rounded(mx, my, half, mh * 0.42, radius, lit);
    }
    if right || middle {
        ui.rounded(mx + half, my, half, mh * 0.42, radius, lit);
    }
    if middle {
        ui.rounded(mx + half * 0.55, my + mh * 0.08, half * 0.9, mh * 0.18, 0.0, lit);
    }
    true
}

/// A key the controls hold for the player. It goes the same road a pressed key
/// goes, so a control named E opens the inventory exactly when the E on the
/// keyboard does — including a rebinding the player made in the options.
fn hold(game: &mut Game, key: winit::keyboard::KeyCode, down: bool) {
    if down {
        game.physical_keys.insert(key);
        game.keys.insert(key);
    } else {
        game.physical_keys.remove(&key);
        game.keys.remove(&key);
    }
    if let Err(error) = game.control_key(key, down) {
        game.error = Some(error);
    }
}

/// A layout names a key by its GLFW number — the same numbers every other
/// launcher's layout uses, so a layout a player made elsewhere asks for the same
/// keys here.
fn glfw(code: i32) -> Option<winit::keyboard::KeyCode> {
    use winit::keyboard::KeyCode;
    Some(match code {
        32 => KeyCode::Space,
        39 => KeyCode::Quote,
        44 => KeyCode::Comma,
        45 => KeyCode::Minus,
        46 => KeyCode::Period,
        47 => KeyCode::Slash,
        59 => KeyCode::Semicolon,
        61 => KeyCode::Equal,
        91 => KeyCode::BracketLeft,
        92 => KeyCode::Backslash,
        93 => KeyCode::BracketRight,
        96 => KeyCode::Backquote,
        256 => KeyCode::Escape,
        257 => KeyCode::Enter,
        258 => KeyCode::Tab,
        259 => KeyCode::Backspace,
        260 => KeyCode::Insert,
        261 => KeyCode::Delete,
        262 => KeyCode::ArrowRight,
        263 => KeyCode::ArrowLeft,
        264 => KeyCode::ArrowDown,
        265 => KeyCode::ArrowUp,
        266 => KeyCode::PageUp,
        267 => KeyCode::PageDown,
        268 => KeyCode::Home,
        269 => KeyCode::End,
        340 => KeyCode::ShiftLeft,
        341 => KeyCode::ControlLeft,
        342 => KeyCode::AltLeft,
        344 => KeyCode::ShiftRight,
        345 => KeyCode::ControlRight,
        346 => KeyCode::AltRight,
        348 => KeyCode::ContextMenu,
        _ => letter(code)?,
    })
}

/// Letters, digits and function keys are runs in GLFW's numbering and named runs
/// in winit's, so they are worth their own table rather than ninety arms above.
fn letter(code: i32) -> Option<winit::keyboard::KeyCode> {
    use winit::keyboard::KeyCode;
    Some(match code {
        48 => KeyCode::Digit0,
        49 => KeyCode::Digit1,
        50 => KeyCode::Digit2,
        51 => KeyCode::Digit3,
        52 => KeyCode::Digit4,
        53 => KeyCode::Digit5,
        54 => KeyCode::Digit6,
        55 => KeyCode::Digit7,
        56 => KeyCode::Digit8,
        57 => KeyCode::Digit9,
        65 => KeyCode::KeyA,
        66 => KeyCode::KeyB,
        67 => KeyCode::KeyC,
        68 => KeyCode::KeyD,
        69 => KeyCode::KeyE,
        70 => KeyCode::KeyF,
        71 => KeyCode::KeyG,
        72 => KeyCode::KeyH,
        73 => KeyCode::KeyI,
        74 => KeyCode::KeyJ,
        75 => KeyCode::KeyK,
        76 => KeyCode::KeyL,
        77 => KeyCode::KeyM,
        78 => KeyCode::KeyN,
        79 => KeyCode::KeyO,
        80 => KeyCode::KeyP,
        81 => KeyCode::KeyQ,
        82 => KeyCode::KeyR,
        83 => KeyCode::KeyS,
        84 => KeyCode::KeyT,
        85 => KeyCode::KeyU,
        86 => KeyCode::KeyV,
        87 => KeyCode::KeyW,
        88 => KeyCode::KeyX,
        89 => KeyCode::KeyY,
        90 => KeyCode::KeyZ,
        290 => KeyCode::F1,
        291 => KeyCode::F2,
        292 => KeyCode::F3,
        293 => KeyCode::F4,
        294 => KeyCode::F5,
        295 => KeyCode::F6,
        296 => KeyCode::F7,
        297 => KeyCode::F8,
        298 => KeyCode::F9,
        299 => KeyCode::F10,
        300 => KeyCode::F11,
        301 => KeyCode::F12,
        _ => return None,
    })
}
