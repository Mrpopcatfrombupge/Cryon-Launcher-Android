//! The settings a player reaches without leaving the world, in the shape the
//! Android launchers before this one settled on: a gear tab on the top edge, a
//! list of actions down the right side, and a card that slides in from the left
//! when the list is asked for something.
//!
//! Everything on the card takes effect on the frame after it is changed, which is
//! the point of the drawer: a resolution is tried rather than saved, quit,
//! restarted and tried again. `CANCEL` puts the numbers back the way they were
//! found and `OK` writes them to the files the game and the launcher both read,
//! so what was tuned while playing is what the phone starts with next time.
//!
//! The colours are the launcher's own, taken from its `colors.xml` and its dark
//! AppCompat theme: a `#181818` card with `#8sdp` corners floating over an
//! undimmed world, white words and `#B2B2B2` values, `#464646` rules, and teal
//! for anything the player can move.
//!
//! All of it is drawn by the renderer, like the controls, because the surface a
//! `NativeActivity` is given is ordered above its own window: a Java dialog laid
//! over the game is hidden by it, and a window of its own can be seen but never
//! touched.

use super::controls;
use super::Game;
use crate::ui_theme::{color, Face, Ui, SMALL};
use std::time::Instant;
use winit::event::{Touch, TouchPhase};
use winit::keyboard::KeyCode;

/// `background_app`, the card.
const CARD: u32 = color::hex(0x18_1818, 255);
/// `colorBackgroundFloating` in the dark theme, the drawer's list.
const FLOAT: u32 = color::hex(0x42_4242, 255);
/// `primary_text`.
const INK: u32 = color::hex(0xFF_FFFF, 255);
/// `secondary_text`, for a row's value.
const SOFT: u32 = color::hex(0xB2_B2B2, 255);
/// `divider`, for rules and an untouched track.
const RULE: u32 = color::hex(0x46_4646, 255);
/// The dark AppCompat theme's accent: a thumb, a fill, a button's word.
const TEAL: u32 = color::hex(0x80_CBC4, 255);
/// The handle's plate, at the launcher's own third-strength alpha.
const DARK: u32 = color::hex(0x00_0000, 255);

/// One row of the card. The set is the launcher's quick-settings dialog: the
/// resolution, the mouse, the glass's gestures and the hold that turns a still
/// finger into a dig. What the launcher keeps for its own video and sound pages
/// lives here in the game's options screen, which the list also reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Id {
    Resolution,
    Speed,
    Gestures,
    Delay,
    Buttons,
}

/// The card's two pages of rows, reached from different list items.
const SETTINGS: [Id; 4] = [Id::Resolution, Id::Speed, Id::Gestures, Id::Delay];
const CONTROLS: [Id; 3] = [Id::Buttons, Id::Delay, Id::Gestures];

impl Id {
    fn label(self) -> &'static str {
        match self {
            Self::Resolution => "Resolution scaler",
            Self::Speed => "Mouse speed",
            Self::Gestures => "Gestures",
            Self::Delay => "Long-press delay",
            Self::Buttons => "Button size",
        }
    }

    /// A switch instead of a bar. The launcher's switches hide the rows that
    /// belong to them, which is why the delay sits under the gesture word.
    fn switch(self) -> bool {
        self == Self::Gestures
    }

    /// The least, the most and the notch it moves by — the launcher's own
    /// seekbar ranges: resolution 25 to 100 by fives, mouse 25 to 300 by fives,
    /// the trigger 100 to 1000 milliseconds.
    fn range(self) -> (f32, f32, f32) {
        match self {
            Self::Resolution => (50.0, 200.0, 5.0),
            Self::Speed => (25.0, 300.0, 5.0),
            Self::Delay => (100.0, 1000.0, 10.0),
            Self::Buttons => (50.0, 200.0, 5.0),
            Self::Gestures => (0.0, 1.0, 1.0),
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Resolution | Self::Speed | Self::Buttons => "%",
            Self::Delay => " ms",
            Self::Gestures => "",
        }
    }

    fn get(self, values: Values) -> f32 {
        match self {
            Self::Resolution => values.resolution,
            Self::Speed => values.speed,
            Self::Gestures => values.gestures as u8 as f32,
            Self::Delay => values.delay,
            Self::Buttons => values.buttons,
        }
    }

    fn set(self, values: &mut Values, at: f32) {
        match self {
            Self::Resolution => values.resolution = at,
            Self::Speed => values.speed = at,
            Self::Gestures => values.gestures = at > 0.5,
            Self::Delay => values.delay = at,
            Self::Buttons => values.buttons = at,
        }
    }

    /// The word a row shows for its number, snapped to the notch it moves by.
    fn shown(self, values: Values) -> String {
        let at = self.snap(self.get(values));
        if self.switch() {
            return if at > 0.5 { "ON" } else { "OFF" }.to_owned();
        }
        let (_, _, notch) = self.range();
        let digits = if notch >= 1.0 { 0 } else { 1 };
        format!("{:.*}{}", digits, at, self.suffix())
    }

    /// Pull a number onto the notch this row moves by, and inside its range.
    fn snap(self, at: f32) -> f32 {
        let (low, high, notch) = self.range();
        let steps = ((at - low) / notch).round();
        (steps * notch + low).clamp(low, high)
    }
}

/// The keys the list offers to press once, in place of a keyboard: the launcher
/// sends a raw GLFW code, and these are the codes a player reaches for.
const KEYS: [(KeyCode, &str); 18] = [
    (KeyCode::Escape, "ESC"),
    (KeyCode::Tab, "TAB"),
    (KeyCode::Enter, "ENTER"),
    (KeyCode::Space, "JUMP"),
    (KeyCode::KeyE, "E"),
    (KeyCode::KeyQ, "Q"),
    (KeyCode::KeyT, "T"),
    (KeyCode::F3, "F3"),
    (KeyCode::F5, "F5"),
    (KeyCode::ShiftLeft, "SHIFT"),
    (KeyCode::ControlLeft, "CTRL"),
    (KeyCode::KeyW, "W"),
    (KeyCode::KeyA, "A"),
    (KeyCode::KeyS, "S"),
    (KeyCode::KeyD, "D"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit9, "9"),
];

/// The list down the right edge, in the launcher's order. Its first four words
/// are the launcher's own; the last reaches the game's full options screen,
/// which is where the launcher keeps the settings this card does not.
const MENU: [&str; 6] = [
    "Force close",
    "Log output",
    "Send custom keycode",
    "Quick settings",
    "Custom controls",
    "Video settings",
];

/// Which face of the drawer is out.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pane {
    Closed,
    List,
    Card,
}

/// What the card is showing.
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Settings,
    Controls,
    Keys,
    Log,
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Settings => "Quick settings",
            Self::Controls => "Custom controls",
            Self::Keys => "Send custom keycode",
            Self::Log => "Log output",
        }
    }

    /// The card's bottom row: a page of rows accepts or discards, a page of
    /// words only goes back to the list.
    fn editable(self) -> bool {
        matches!(self, Self::Settings | Self::Controls)
    }
}

/// Every number the card offers, as the card shows it.
#[derive(Clone, Copy)]
struct Values {
    resolution: f32,
    speed: f32,
    gestures: bool,
    delay: f32,
    buttons: f32,
}

impl Values {
    /// Read the game's own settings, which is what the card opens on.
    fn read(game: &Game, tune: &controls::Tuning) -> Self {
        Self {
            resolution: game.settings.cryon.resolution_scale as f32,
            // The launcher's mouse speed is a multiplier and the game's is a
            // fraction of its own range, so 100% is the vanilla middle.
            speed: game.settings.sensitivity * 200.0,
            gestures: tune.gestures,
            delay: tune.long_press as f32,
            buttons: tune.scale * 100.0,
        }
    }

    /// Push them into the game and make the running frame answer to them. The
    /// three rows that belong to the controls are handed back to the touch layer,
    /// which is the only thing that can change them from inside a finger it is
    /// already holding.
    fn write(self, game: &mut Game, tune: &mut controls::Tuning) {
        game.settings.cryon.resolution_scale = self.resolution as u32;
        game.settings.sensitivity = (self.speed / 200.0).clamp(0.0, 1.0);
        tune.gestures = self.gestures;
        tune.long_press = self.delay as u64;
        tune.scale = (self.buttons / 100.0).clamp(0.5, 2.0);
        game.apply_settings();
        game.apply_ui_settings();
    }

    /// The rows the launcher owns rather than the game, written back to the
    /// switch file it reads before the next start, so its own settings screen
    /// shows what was changed in the world rather than what it was first told.
    fn keep(self) {
        keep("CRYON_BUTTON_SCALE", &format!("{:.0}", self.buttons));
        keep("CRYON_LONG_PRESS", &format!("{:.0}", self.delay));
        keep("CRYON_GESTURES", if self.gestures { "1" } else { "0" });
    }
}

/// What a finger landed on, asked in this order.
#[derive(Clone, Copy, PartialEq)]
enum Hit {
    Gear,
    Item(usize),
    Slider(Id),
    Switch(Id),
    Key(usize),
    Back,
    Cancel,
    Apply,
    /// The card's own body, between the rows: nothing to do, but the drawer
    /// stays out. Only a tap past its edge is taken as stepping away.
    Panel,
    /// The glass beside everything: the drawer goes away, as it does in the
    /// launcher, where tapping past the dialog is how a player backs out.
    Outside,
}

/// Where each of those is, in the screen's own units, measured as the frame draws.
type Rect = [f32; 4];

struct Drawer {
    pane: Pane,
    page: Page,
    /// The two faces slide on their own clocks, so a card coming out over a list
    /// does not drag the list back with it.
    t_list: f32,
    t_card: f32,
    list: f32,
    card: f32,
    since: Option<Instant>,
    values: Values,
    before: Values,
    dragging: Option<Id>,
    places: Vec<(Hit, Rect)>,
    /// The screen's size, kept for the trace.
    size: (f32, f32),
    /// Set when the list's first word is tapped: only the event loop can leave.
    exit: bool,
    /// The log's lines, taken once when the page is opened rather than every
    /// frame, so they can be read instead of watched scroll away.
    lines: Vec<String>,
}

impl Default for Drawer {
    fn default() -> Self {
        let values = Values {
            resolution: 100.0,
            speed: 100.0,
            gestures: true,
            delay: 300.0,
            buttons: 100.0,
        };
        Self {
            pane: Pane::Closed,
            page: Page::Settings,
            t_list: 0.0,
            t_card: 0.0,
            list: 0.0,
            card: 0.0,
            since: None,
            values,
            before: values,
            dragging: None,
            places: Vec::new(),
            size: (0.0, 0.0),
            exit: false,
            lines: Vec::new(),
        }
    }
}

impl Drawer {
    /// The drawer takes a finger, and says whether it kept it: a finger it keeps
    /// never reaches the controls underneath.
    fn finger(
        &mut self,
        game: &mut Game,
        touch: &Touch,
        step: f32,
        tune: &mut controls::Tuning,
    ) -> bool {
        let at = (touch.location.x as f32 / step, touch.location.y as f32 / step);
        let hit = self
            .places
            .iter()
            .find(|(_, rect)| inside(*rect, at))
            .map(|(place, rect)| (*place, *rect));
        if traced() {
            log::info!(
                "Cryon: drawer {:?} #{} at ({:.0}, {:.0}) of {:.0}x{:.0} -> {} of {} pane={:?}",
                touch.phase,
                touch.id,
                at.0,
                at.1,
                self.size.0,
                self.size.1,
                hit.map(|(place, _)| name(place))
                    .unwrap_or_else(|| "nothing".to_owned()),
                self.places.len(),
                self.pane,
            );
        }
        match touch.phase {
            TouchPhase::Started => match hit {
                Some((Hit::Gear, _)) => {
                    self.pane = Pane::List;
                    true
                }
                Some((Hit::Item(number), _)) => {
                    self.choose(game, tune, number, at);
                    true
                }
                Some((Hit::Slider(id), rect)) => {
                    self.dragging = Some(id);
                    self.slide(game, tune, id, at, rect);
                    true
                }
                Some((Hit::Switch(id), _)) => {
                    let flip = id.get(self.values) < 0.5;
                    id.set(&mut self.values, flip as u8 as f32);
                    self.values.write(game, tune);
                    true
                }
                Some((Hit::Key(number), _)) => {
                    if let Some((code, _)) = KEYS.get(number) {
                        // A word on the page is a key pressed and let go, which is
                        // what a phone has no way of doing to a glass plate.
                        let _ = game.control_key(*code, true);
                        let _ = game.control_key(*code, false);
                    }
                    true
                }
                Some((Hit::Back, _)) => {
                    self.pane = Pane::List;
                    true
                }
                Some((Hit::Cancel, _)) => {
                    self.close(game, tune, false);
                    true
                }
                Some((Hit::Apply, _)) => {
                    self.close(game, tune, true);
                    true
                }
                Some((Hit::Outside, _)) => {
                    self.close(game, tune, false);
                    true
                }
                Some((Hit::Panel, _)) => true,
                None => self.slide_to(game, tune, at),
            },
            TouchPhase::Moved => self.slide_to(game, tune, at),
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.dragging = None;
                self.pane != Pane::Closed
            }
        }
    }

    /// One word of the list, asked for.
    fn choose(
        &mut self,
        game: &mut Game,
        tune: &mut controls::Tuning,
        number: usize,
        at: (f32, f32),
    ) {
        let _ = at;
        match number {
            0 => self.exit = true,
            1 => {
                self.lines = cryon_client::logging::recent_lines();
                self.show(Page::Log, game, tune);
            }
            2 => self.show(Page::Keys, game, tune),
            3 => self.show(Page::Settings, game, tune),
            4 => self.show(Page::Controls, game, tune),
            5 => {
                self.pane = Pane::Closed;
                game.open_options(false);
            }
            _ => self.pane = Pane::Closed,
        }
    }

    /// Open the card on a page. A page of rows reads the settings it is about to
    /// change, so that `CANCEL` has something to go back to.
    fn show(&mut self, page: Page, game: &mut Game, tune: &controls::Tuning) {
        if page.editable() && self.page != page {
            self.values = Values::read(game, tune);
            self.before = self.values;
        }
        self.page = page;
        self.pane = Pane::Card;
        self.dragging = None;
    }

    /// A finger on a track, moving it: the row is the one already being held, so
    /// a thumb that wanders off the narrow bar still sets the number it started.
    fn slide_to(&mut self, game: &mut Game, tune: &mut controls::Tuning, at: (f32, f32)) -> bool {
        if self.pane != Pane::Card {
            return self.pane != Pane::Closed;
        }
        let Some(id) = self.dragging else {
            return true;
        };
        let Some((_, rect)) = self
            .places
            .iter()
            .find(|(place, _)| *place == Hit::Slider(id))
        else {
            return true;
        };
        let rect = *rect;
        self.slide(game, tune, id, at, rect);
        true
    }

    fn slide(
        &mut self,
        game: &mut Game,
        tune: &mut controls::Tuning,
        id: Id,
        at: (f32, f32),
        track: Rect,
    ) {
        let (low, high, _) = id.range();
        let part = ((at.0 - track[0]) / track[2].max(1.0)).clamp(0.0, 1.0);
        let wanted = id.snap(low + part * (high - low));
        if wanted == id.get(self.values) {
            return;
        }
        let mut next = self.values;
        id.set(&mut next, wanted);
        self.values = next;
        if id == Id::Buttons {
            // Only the plates change, so the controls are re-placed rather than
            // the whole set of settings pushed through the renderer again.
            tune.scale = (next.buttons / 100.0).clamp(0.5, 2.0);
            return;
        }
        next.write(game, tune);
    }

    /// `CANCEL` puts the numbers back the way they were found; `OK` keeps them and
    /// writes them where the game and the launcher will both read them again.
    fn close(&mut self, game: &mut Game, tune: &mut controls::Tuning, accept: bool) {
        if self.page.editable() {
            if accept {
                self.values.write(game, tune);
                game.settings.save(&Game::options_path());
                self.values.keep();
            } else {
                self.before.write(game, tune);
            }
        }
        self.pane = Pane::Closed;
        self.dragging = None;
    }

    /// Between one frame and the next, move both faces toward where they belong.
    /// The launcher's dialog takes six tenths of a second and gathers speed
    /// leaving the edge and loses it arriving, which is what makes it read as an
    /// object moved rather than as a screen redrawn.
    fn ease(&mut self) {
        let now = Instant::now();
        let dt = self
            .since
            .map(|then| now.duration_since(then).as_secs_f32())
            .unwrap_or_default();
        self.since = Some(now);
        let part = (dt / 0.6).min(1.0);
        let list = self.pane == Pane::List;
        let card = self.pane == Pane::Card;
        self.t_list = if list {
            (self.t_list + part).min(1.0)
        } else {
            (self.t_list - part).max(0.0)
        };
        self.t_card = if card {
            (self.t_card + part).min(1.0)
        } else {
            (self.t_card - part).max(0.0)
        };
        self.list = self.t_list * self.t_list * (3.0 - 2.0 * self.t_list);
        self.card = self.t_card * self.t_card * (3.0 - 2.0 * self.t_card);
    }

    /// Draw the tab, the list and the card, and remember where each of them was so
    /// the next finger can be read against it.
    fn draw(&mut self, ui: &mut Ui) {
        self.ease();
        self.places.clear();
        self.size = (ui.w, ui.h);
        if self.list <= 0.0 && self.card <= 0.0 {
            self.gear(ui);
            return;
        }
        if self.list > 0.0 {
            self.menu(ui);
        }
        if self.card > 0.0 {
            self.sheet(ui);
        }
    }

    /// The tab on the top edge, in the middle: the launcher's handle, a black
    /// half-disc at a third of its own strength with a gear in it, 40 by 20 dp,
    /// with a hitbox bigger than the drawing because a thumb cannot find a tab
    /// that small on a phone held in one hand.
    fn gear(&mut self, ui: &mut Ui) {
        let width = ui.w * 0.05;
        let tall = width * 0.5;
        let x = ui.w / 2.0 - width / 2.0;
        let reach = tall * 0.6;
        self.places.push((
            Hit::Gear,
            [x - reach, 0.0, width + reach * 2.0, tall + reach],
        ));
        ui.rounded(x, 0.0, width, tall, tall * 0.5, color::with_alpha(DARK, 0.33));
        gear(ui, x + width / 2.0, tall * 0.5, tall * 0.3, color::with_alpha(INK, 0.9));
    }

    /// The list down the right edge: 200 dp of `colorBackgroundFloating`, one
    /// word per row, 48 dp tall, nothing else on it.
    fn menu(&mut self, ui: &mut Ui) {
        let (w, h) = (ui.w, ui.h);
        let out = self.list;
        let width = w * 0.25;
        let left = w - width * out;
        let row = h * 0.133;
        ui.rounded(left, 0.0, width, h, 0.0, FLOAT);
        ui.rounded(
            left - 4.0,
            0.0,
            4.0,
            h,
            0.0,
            color::with_alpha(DARK, 0.45 * out),
        );
        let size = SMALL * 1.2;
        for (number, word) in MENU.iter().enumerate() {
            let top = number as f32 * row;
            self.places.push((Hit::Item(number), [left, top, width, row]));
            ui.text(
                Face::Regular,
                size,
                left + width * 0.06,
                top + (row - size * 1.45) / 2.0,
                color::with_alpha(INK, out),
                word,
            );
        }
    }

    /// The card: `background_app` with `8sdp` corners, 20 dp from the left edge,
    /// as tall as what it holds, and the world still bright behind it because the
    /// launcher never dims the game for a dialog.
    fn sheet(&mut self, ui: &mut Ui) {
        let (w, h) = (ui.w, ui.h);
        let out = self.card;
        let edge = w * 0.025;
        let pad = h * 0.022;
        let width = w * 0.35;
        let radius = h * 0.022;
        let title = SMALL * 1.5;
        let line = SMALL * 1.45;
        let bar = h * 0.075;
        let track = (h * 0.006).clamp(2.0, 4.0);
        let knob = (h * 0.017).clamp(5.0, 12.0);
        let rows = self.rows();
        // A row is a word and, under it, the bar's own band — the launcher puts
        // the seekbar on a line of its own rather than beside the label.
        let body = match self.page {
            Page::Settings | Page::Controls => rows as f32 * (line + h * 0.075),
            Page::Keys => key_rows(rows) as f32 * (h * 0.075),
            Page::Log => (self.lines.len().min(14) as f32) * line,
        };
        let head = pad + title * 1.45 + pad * 0.5;
        let tall = (head + body + bar + pad).min(h - pad * 2.0);
        let top = (h - tall) / 2.0;
        let left = -width + (edge + width) * out;

        // The shadow the launcher gives the card by elevation: drawn a little
        // lower and only its edge shows.
        ui.rounded(
            left + 3.0,
            top + 5.0,
            width,
            tall,
            radius,
            color::with_alpha(DARK, 0.5 * out),
        );
        ui.rounded(left, top, width, tall, radius, CARD);

        ui.text(
            Face::Medium,
            title,
            left + pad,
            top + pad,
            color::with_alpha(INK, out),
            self.page.title(),
        );
        let rule = top + head;
        ui.rounded(
            left + pad,
            rule,
            width - pad * 2.0,
            1.0,
            0.0,
            color::with_alpha(RULE, out),
        );

        let first = rule + pad * 0.6;
        let bar_y = top + tall - bar;
        match self.page {
            Page::Settings | Page::Controls => {
                let band = (bar_y - first - pad * 0.6) / rows as f32;
                for (number, id) in self.ids().iter().enumerate() {
                    let row = first + number as f32 * band;
                    let value = id.shown(self.values);
                    ui.text(
                        Face::Regular,
                        SMALL,
                        left + pad,
                        row,
                        color::with_alpha(INK, 0.95 * out),
                        id.label(),
                    );
                    let text = ui.measure(Face::Regular, SMALL, &value);
                    ui.text(
                        Face::Regular,
                        SMALL,
                        left + width - pad - text,
                        row,
                        color::with_alpha(SOFT, out),
                        &value,
                    );
                    let x0 = left + pad;
                    let wide = width - pad * 2.0;
                    let mid = row + line + (band - line) / 2.0;
                    if id.switch() {
                        let on = id.get(self.values) > 0.5;
                        self.places.push((Hit::Switch(*id), [x0, row, wide, band]));
                        // The system's switch: a groove with a disc riding over
                        // its ends, not a plate painted in.
                        let groove = knob * 3.2;
                        let depth = knob * 1.2;
                        let x = left + width - pad - groove;
                        ui.rounded(
                            x,
                            mid - depth / 2.0,
                            groove,
                            depth,
                            depth / 2.0,
                            color::with_alpha(if on { TEAL } else { SOFT }, if on { 0.7 } else { 0.4 } * out),
                        );
                        let at = if on { x + groove - knob } else { x };
                        ui.rounded(
                            at - knob,
                            mid - knob,
                            knob * 2.0,
                            knob * 2.0,
                            knob,
                            color::with_alpha(if on { TEAL } else { INK }, out),
                        );
                        continue;
                    }
                    let travel = (wide - knob * 2.0).max(1.0);
                    self.places.push((
                        Hit::Slider(*id),
                        [x0 + knob, row + line, travel, band - line],
                    ));
                    ui.rounded(
                        x0 + knob,
                        mid - track / 2.0,
                        travel,
                        track,
                        track / 2.0,
                        color::with_alpha(RULE, out),
                    );
                    let (low, high, _) = id.range();
                    let part = (id.get(self.values) - low) / (high - low);
                    ui.rounded(
                        x0 + knob,
                        mid - track / 2.0,
                        (part * travel).max(track),
                        track,
                        track / 2.0,
                        color::with_alpha(TEAL, out),
                    );
                    let at = x0 + knob + part * travel;
                    ui.rounded(
                        at - knob,
                        mid - knob,
                        knob * 2.0,
                        knob * 2.0,
                        knob,
                        color::with_alpha(TEAL, out),
                    );
                }
            }
            Page::Keys => {
                let band = (bar_y - first - pad * 0.6) / key_rows(rows) as f32;
                let each = (width - pad * 2.0) / 6.0;
                for (number, (_, word)) in KEYS.iter().enumerate() {
                    let column = (number % 6) as f32;
                    let row = (number / 6) as f32;
                    let x = left + pad + column * each + each * 0.08;
                    let y = first + row * band + band * 0.1;
                    let (wide, deep) = (each * 0.84, band * 0.8);
                    self.places.push((Hit::Key(number), [x, y, wide, deep]));
                    ui.rounded(x, y, wide, deep, h * 0.006, color::with_alpha(RULE, out));
                    ui.text_centered(
                        Face::Regular,
                        SMALL * 0.9,
                        x + wide / 2.0,
                        y + (deep - SMALL * 0.9 * 1.45) / 2.0,
                        color::with_alpha(INK, out),
                        word,
                    );
                }
            }
            Page::Log => {
                for (number, entry) in self.lines.iter().rev().take(14).enumerate() {
                    let y = first + number as f32 * line;
                    let text = shorten(entry, (width - pad * 2.0) / SMALL * 1.7);
                    ui.text(
                        Face::Mono,
                        SMALL * 0.85,
                        left + pad,
                        y,
                        color::with_alpha(SOFT, out),
                        &text,
                    );
                }
            }
        }

        // The words along the bottom are borderless, as the launcher's are: they
        // are buttons of text, not plates to be filled in.
        for (place, word, at) in [
            (
                if self.page.editable() { Hit::Cancel } else { Hit::Back },
                if self.page.editable() { "CANCEL" } else { "BACK" },
                left + pad,
            ),
            (Hit::Apply, "OK", left + width - pad),
        ] {
            if place == Hit::Apply && !self.page.editable() {
                continue;
            }
            let text = ui.measure(Face::Medium, SMALL, word);
            let wide = text + pad * 1.6;
            let x = match place {
                Hit::Apply => at - wide,
                _ => at,
            };
            self.places.push((place, [x, bar_y, wide, bar]));
            ui.text(
                Face::Medium,
                SMALL,
                x + pad * 0.8,
                bar_y + (bar - line) / 2.0,
                color::with_alpha(TEAL, out),
                word,
            );
        }

        // Last, so a finger anywhere inside the card is read as being on one of
        // the rows above rather than as a tap past the edge of it.
        self.places.push((Hit::Panel, [left, top, width, tall]));
        self.places.push((Hit::Outside, [0.0, 0.0, w, h]));
    }

    /// The rows the current page shows.
    fn ids(&self) -> &'static [Id] {
        match self.page {
            Page::Controls => &CONTROLS,
            _ => &SETTINGS,
        }
    }

    fn rows(&self) -> usize {
        match self.page {
            Page::Settings => SETTINGS.len(),
            Page::Controls => CONTROLS.len(),
            Page::Keys => KEYS.len(),
            Page::Log => self.lines.len(),
        }
    }
}

/// How many lines of key words a page of this many keys needs.
fn key_rows(keys: usize) -> usize {
    (keys + 5) / 6
}

/// A log line cut to the card's width, from the left, so the newest part of a
/// frame's message is the part that is lost.
fn shorten(text: &str, room: f32) -> String {
    let keep = room.max(24.0) as usize;
    if text.chars().count() <= keep {
        return text.to_owned();
    }
    let cut: String = text.chars().take(keep.saturating_sub(1)).collect();
    format!("{cut}…")
}

/// A gear, drawn the way the plates are drawn: a ring, teeth around it and a hub.
/// The launcher's handle carries the system's own settings icon, which a renderer
/// has no reason to own a copy of.
fn gear(ui: &mut Ui, cx: f32, cy: f32, radius: f32, ink: u32) {
    let ring = radius * 2.0;
    ui.rounded(cx - radius, cy - radius, ring, ring, radius, color::with_alpha(ink, 0.18));
    for tooth in 0..8 {
        let angle = tooth as f32 * std::f32::consts::PI / 4.0;
        let size = radius * 0.52;
        ui.rounded(
            cx + angle.cos() * radius * 0.98 - size / 2.0,
            cy + angle.sin() * radius * 0.98 - size / 2.0,
            size,
            size,
            size * 0.3,
            ink,
        );
    }
    let hub = radius * 0.86;
    ui.rounded(cx - hub / 2.0, cy - hub / 2.0, hub, hub, hub / 2.0, ink);
    let hole = radius * 0.4;
    ui.rounded(
        cx - hole / 2.0,
        cy - hole / 2.0,
        hole,
        hole,
        hole / 2.0,
        color::with_alpha(DARK, 0.9),
    );
}

fn inside(rect: Rect, at: (f32, f32)) -> bool {
    at.0 >= rect[0] && at.0 < rect[0] + rect[2] && at.1 >= rect[1] && at.1 < rect[1] + rect[3]
}

/// `CRYON_TOUCH_DEBUG=1` traces the drawer too: a row that answers a tap meant
/// for its neighbour is a row measured differently from the finger, and from the
/// outside the two look the same.
fn traced() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CRYON_TOUCH_DEBUG").is_some())
}

fn name(place: Hit) -> String {
    match place {
        Hit::Gear => "the gear".to_owned(),
        Hit::Item(number) => MENU
            .get(number)
            .map(|word| (*word).to_owned())
            .unwrap_or_else(|| "a missing word".to_owned()),
        Hit::Slider(id) => format!("the {} bar", id.label()),
        Hit::Switch(id) => format!("the {} switch", id.label()),
        Hit::Key(number) => KEYS
            .get(number)
            .map(|(_, word)| format!("the {word} key"))
            .unwrap_or_else(|| "a missing key".to_owned()),
        Hit::Back => "back".to_owned(),
        Hit::Cancel => "cancel".to_owned(),
        Hit::Apply => "ok".to_owned(),
        Hit::Panel => "the card".to_owned(),
        Hit::Outside => "past the card".to_owned(),
    }
}

/// One line of the launcher's switch file, replaced rather than appended to, so a
/// player who has changed a setting twice does not find the file full of the
/// numbers they tried on the way.
fn keep(key: &str, value: &str) {
    let Some(path) = std::env::var_os("CRYON_ENV_FILE") else {
        return;
    };
    let file = std::path::Path::new(&path);
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let mut lines = Vec::new();
    let mut wrote = false;
    for line in text.lines() {
        match line.split_once('=') {
            Some((named, _)) if named.trim() == key && !wrote => {
                lines.push(format!("{key}={value}"));
                wrote = true;
            }
            _ => lines.push(line.to_owned()),
        }
    }
    if !wrote {
        lines.push(format!("{key}={value}"));
    }
    if let Err(error) = std::fs::write(file, lines.join("\n") + "\n") {
        log::warn!("Cryon: env.txt: would not keep {key}: {error}");
    }
}

thread_local! {
    static DRAWER: std::cell::RefCell<Drawer> = std::cell::RefCell::new(Drawer::default());
}

/// A reported finger, which the drawer answers with before the controls do. True
/// means it kept it: the game never sees that finger. The tuning of the controls
/// comes in by hand, because the drawer is asked from inside the touch layer's own
/// state and cannot borrow it a second time to read or change them.
pub fn finger(
    game: &mut Game,
    touch: &Touch,
    surface: (f32, f32),
    tune: &mut controls::Tuning,
) -> bool {
    if surface.0 <= 0.0 || surface.1 <= 0.0 {
        return false;
    }
    // The same measure `Ui::new` takes, so a place remembered from a frame is read
    // against the finger that was put on it.
    let step = (surface.0 / 1040.0)
        .min(surface.1 / 700.0)
        .clamp(1.0, 4.0);
    DRAWER.with(|state| state.borrow_mut().finger(game, touch, step, tune))
}

/// Whether the list's first word was tapped: only the event loop can leave, and
/// the touch layer is the one holding it.
pub fn wants_exit() -> bool {
    DRAWER.with(|state| state.borrow().exit)
}

/// Draw the tab, the list and the card over a frame.
pub fn draw(ui: &mut Ui) {
    DRAWER.with(|state| state.borrow_mut().draw(ui));
}
