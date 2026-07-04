//! Headless compositor-acceptance test for the pixel-art layer path.
//!
//! The `math_game` example's per-phase layer choice can't open a window in CI, so
//! this reconstructs the same pixel-art composition — the reject cross, the
//! GAME OVER sign, the win marquee — with the generic [`Placard`], [`Marquee`],
//! and [`Presentation`], and asserts each banner paints its colour into the
//! framebuffer. The banners are `font8x8` pixel art (via [`BigText::build`]), so
//! no system font is needed and the result is deterministic. The input overlay
//! (which needs a system font) is intentionally left out — its model is covered
//! by unit tests.

use ratgames::{
    BigText, Color, Config, Marquee, OverlayLayer, PixelLayer, Placard, Presentation, Size, Sprite,
    Surface, TextColors, palette,
};

/// Render `world` (no overlays) into a fresh window-sized surface at 1:1.
fn frame(pres: &mut Presentation, world: &[&dyn PixelLayer]) -> Surface {
    let mut window = Surface::new(pres.virtual_size(), Color::rgb(0, 0, 0));
    let no_overlays: [&dyn OverlayLayer; 0] = [];
    pres.render(world, &no_overlays, &mut window);
    window
}

fn contains(surface: &Surface, color: Color) -> bool {
    surface.as_slice().iter().any(|&w| w == color.packed())
}

/// A presentation whose window matches the virtual screen (integer scale 1), so
/// pixel-art colours land in the framebuffer verbatim.
fn presentation(config: &Config) -> Presentation {
    let s = config.screen;
    Presentation::new(s.size, s.backdrop, s.letterbox, s.min_scale)
}

/// Bake `text` into a pixel-art banner sprite in `fill`, through the deterministic
/// `font8x8` bitmap source (no system font).
fn banner(text: &str, scale: u32, fill: Color) -> Sprite {
    BigText::new(scale)
        .outline(1)
        .colors(TextColors {
            fill,
            outline: palette::OUTLINE,
            shadow: palette::OUTLINE,
        })
        .build(text)
}

#[test]
fn the_red_cross_and_gameover_yellow_composite_through_the_presentation() {
    let config = Config::default();
    let red = palette::DANGER;
    let yellow = palette::WARNING;
    let cross = Placard::new(banner("X", 8, red));
    let game_over = Placard::new(banner("GAME OVER", 3, yellow));

    let mut pres = presentation(&config);
    // The reject beat centres the red cross.
    assert!(
        contains(&frame(&mut pres, &[&cross]), red),
        "the reject cross should paint its red fill"
    );
    // The game-over beat centres the gold-shadowed yellow sign.
    assert!(
        contains(&frame(&mut pres, &[&game_over]), yellow),
        "the GAME OVER sign should paint its yellow fill"
    );
}

#[test]
fn the_win_marquee_composites_its_green_fill() {
    let config = Config::default();
    let win_banner = config
        .marquee
        .text_sprite("YOU WIN")
        .expect("bitmap source");
    let win = Marquee::new(win_banner, config.marquee.speed);
    let green = config.marquee.colors.fill;

    let mut pres = presentation(&config);
    assert!(
        contains(&frame(&mut pres, &[&win]), green),
        "the YOU WIN marquee should paint its green fill"
    );
}

#[test]
fn an_empty_world_shows_no_banner_colour() {
    let config = Config::default();
    let mut pres = presentation(&config);

    // Nothing but the backdrop: none of the banner fills are on screen.
    let blank = frame(&mut pres, &[]);
    assert!(!contains(&blank, palette::DANGER));
    assert!(!contains(&blank, palette::WARNING));
    assert!(!contains(&blank, config.marquee.colors.fill));
}

/// The virtual screen dimension must survive `Config`; a scale-1 window means
/// `Size` equality between the two is the contract this test relies on.
#[test]
fn window_matches_virtual_screen_at_scale_one() {
    let config = Config::default();
    let pres = presentation(&config);
    assert_eq!(pres.virtual_size(), config.screen.size);
    assert_eq!(pres.fit_scale(config.screen.size), 1);
    assert_eq!(pres.fit_scale(Size::new(256, 256)), 1);
}
