//! Cryon's launcher palette worn *inside* the game: the Cryon theme mod
//! (`cryon_theme*` content mods) dresses the game's own screens in the launcher's
//! skin — the near-black void, charcoal panels, silver text and one cold-blue
//! accent — and this module is the one place that answers "what colour" for it.
//!
//! The launcher (`crates/cryon-launcher/src/gui.rs`) holds the same nine values as
//! its own `Color32` constants; these are the `0xRRGGBB` form of them, so a token
//! changed there is changed here. [`Palette`] (in `lib.rs`) is the design system's
//! own violet-accented light/dark pair, which the game's Cryon-only overlays
//! (`ui_theme`) keep wearing; this palette is only ever read through [`text`],
//! [`selection`] and [`splash`], and every one of them returns its argument
//! unchanged while the theme is off ([`worn`]), which is how the look stays
//! vanilla with the mod disabled.
//!
//! Colours here are the HUD's encoding (`hud::rgba`: `AABBGGRR`); the tokens are
//! `0xRRGGBB` like the rest of this crate.

use std::sync::atomic::{AtomicBool, Ordering};

/// The page under everything: the launcher's void.
pub const VOID: u32 = 0x0505_06;
/// A panel's face (the launcher's charcoal).
pub const CHARCOAL: u32 = 0x1212_15;
/// A raised panel, a hovered plate (slate).
pub const SLATE: u32 = 0x1a1a_1e;
/// Text (silver).
pub const SILVER: u32 = 0xc4c9_d1;
/// Secondary text and labels (mist).
pub const MIST: u32 = 0x868c_96;
/// Rules, disabled text, the faintest ink (faint).
pub const FAINT: u32 = 0x5459_62;
/// The one cold-blue accent: focus, selection, the splash.
pub const ACCENT: u32 = 0x7aa2_c2;
/// Good news (the launcher's green).
pub const GOOD: u32 = 0x70b6_8b;
/// Bad news (the launcher's red).
pub const BAD: u32 = 0xc670_70;
/// A plate's hairline rim: the line between two of the launcher's surfaces
/// (`#2A2A33`), one step above the charcoal it rings. The theme's panels are
/// edged with this, never with a raised bevel — a stack of bevelled plates is
/// what reads as cubes rather than one sheet of glass.
pub const HAIRLINE: u32 = 0x2a2a_33;
/// The underside of a plate: the void a rim drops into.
pub const LIP: u32 = 0x0a0a_0c;

/// The namespace prefix a content mod wears to dress the game: a mod whose id
/// starts with this (`cryon_theme`, `cryon_theme_title`, …) may override vanilla's
/// GUI sprites by shipping `assets/<id>/textures/<sprite path>.png`, applied by
/// `cryon_client::hud` when it stitches the atlas.
pub const SPRITE_PREFIX: &str = "cryon_theme";

/// `0xRRGGBB` and an alpha as the HUD batches want it (`hud::rgba`).
pub const fn hud(color: u32, alpha: u8) -> u32 {
    (alpha as u32) << 24 | (color & 0xff) << 16 | (color >> 8 & 0xff) << 8 | (color >> 16 & 0xff)
}

/// The launcher's eclipse, as the game's text highlight (the additive blue a
/// `GuiGraphicsExtractor.textHighlight` adds over the inverted glyphs).
pub fn selection() -> u32 {
    hud(ACCENT, 255)
}

/// The title screen's bouncing splash: vanilla's yellow becomes the accent.
pub fn splash(color: u32) -> u32 {
    if worn() { hud(ACCENT, 255) } else { color }
}

/// Is a `cryon_theme` mod in the resource root this frame's atlas was built from?
/// Set by the atlas builder, never by an option: turning the mod off on the Mods
/// page and restarting is what puts vanilla's colours back.
pub fn worn() -> bool {
    WORN.load(Ordering::Relaxed)
}

/// Wear the theme (`true`) or go back to vanilla's colours, as the atlas's
/// contents decide.
pub fn wear(on: bool) {
    WORN.store(on, Ordering::Relaxed);
}

static WORN: AtomicBool = AtomicBool::new(false);

/// Vanilla draws its interface text in three neutral greys — white for a label,
/// `160` for a disabled one, `85`/`64` for hints and shadows. Wearing the theme,
/// those are the launcher's silver, mist and faint; anything with a hue (a chat
/// colour, a rarity, a player name, an enchanted glow) is left exactly as it is,
/// so the theme dresses the interface without recolouring the world's contents.
pub fn text(color: u32) -> u32 {
    if !worn() {
        return color;
    }
    let (a, b, g, r) = (
        (color >> 24) & 0xff,
        (color >> 16) & 0xff,
        (color >> 8) & 0xff,
        color & 0xff,
    );
    if a == 0 {
        return color;
    }
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    // A hue of any note is the text's own; only the greys are the interface's.
    if max - min > 8 {
        return color;
    }
    let ink = match max {
        0..=40 => FAINT,
        41..=199 => MIST,
        _ => SILVER,
    };
    hud(ink, a as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The launcher's nine palette constants, in `0xRRGGBB`: a change to
    /// `crates/cryon-launcher/src/gui.rs` has to show up here too.
    #[test]
    fn the_tokens_are_the_launchers_palette() {
        let rgb = |c: u32| [c >> 16, c >> 8 & 0xff, c & 0xff];
        assert_eq!(rgb(VOID), [5, 5, 6]);
        assert_eq!(rgb(CHARCOAL), [18, 18, 21]);
        assert_eq!(rgb(SLATE), [26, 26, 30]);
        assert_eq!(rgb(SILVER), [196, 201, 209]);
        assert_eq!(rgb(MIST), [134, 140, 150]);
        assert_eq!(rgb(FAINT), [84, 89, 98]);
        assert_eq!(rgb(ACCENT), [122, 162, 194]);
        assert_eq!(rgb(GOOD), [112, 182, 139]);
        assert_eq!(rgb(BAD), [198, 112, 112]);
    }

    #[test]
    fn hud_encoding_is_the_batches_abgr() {
        assert_eq!(hud(0xc4c9d1, 255), 0xffd1c9c4);
        assert_eq!(hud(0x050506, 128), 0x80060505);
    }

    /// Off, every colour is what it was; on, the greys move and the hues do not.
    /// One test, since [`wear`] is the process's one switch.
    #[test]
    fn text_maps_only_neutral_greys_and_only_while_worn() {
        let (white, disabled, hint, gold, green) = (
            0xffff_ffffu32,
            hud(0xa0a0a0, 255),
            hud(0x555555, 255),
            hud(0xffaa00, 255),
            hud(0x55ff55, 200),
        );
        let vanilla_splash = hud(0xffff_00, 255);
        wear(false);
        for c in [white, disabled, hint, gold, green] {
            assert_eq!(text(c), c, "vanilla keeps every colour");
        }
        assert_eq!(splash(vanilla_splash), vanilla_splash);
        wear(true);
        assert_eq!(text(white), hud(SILVER, 255));
        assert_eq!(text(disabled), hud(MIST, 255));
        assert_eq!(text(hint), hud(MIST, 255));
        assert_eq!(text(gold), gold, "a chat colour stays itself");
        assert_eq!(text(green), green);
        // The alpha rides along: a half-transparent label is still half transparent.
        assert_eq!(text(hud(0xffffff, 120)), hud(SILVER, 120));
        assert_eq!(
            text(hud(0x000000, 0)),
            hud(0, 0),
            "nothing drawn is nothing moved"
        );
        assert_eq!(splash(vanilla_splash), hud(ACCENT, 255));
        assert_eq!(selection(), hud(ACCENT, 255));
        wear(false);
        assert_eq!(text(white), white, "and the switch goes back");
    }
}
