//! Headless composition test for the `math_game` wiring.
//!
//! The example's per-phase layer choice can't open a window in CI, so this
//! reconstructs the same pixel-art composition (the reject cross, the game-over
//! sign, the win marquee) with the real [`Quiz`], [`Placard`], [`Marquee`], and
//! [`Presentation`], and asserts each phase paints its banner colour into the
//! framebuffer. The banners are `font8x8` pixel art, so no system font is
//! needed and the result is deterministic. The input overlay (which needs a
//! system font) is intentionally left out — its model is covered by unit tests.

use ratgames::{
    BigText, Color, Config, Marquee, OverlayLayer, Phase, PixelLayer, Placard, Presentation, Quiz,
    Size, Surface,
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

#[test]
fn wrong_answer_shows_the_red_cross_then_the_gameover_yellow() {
    let config = Config::default();
    let cross = Placard::new(config.quiz.cross.sprite());
    let game_over = Placard::new(config.quiz.game_over.sprite());
    let red = config.quiz.cross.colors.fill;
    let yellow = config.quiz.game_over.colors.fill;

    let mut pres = presentation(&config);
    let mut quiz = Quiz::from_config(&config.quiz);

    // A wrong answer starts the blink: on frame 0 the red cross is on screen.
    assert_eq!(quiz.submit("7"), Some(ratgames::Outcome::Wrong));
    assert_eq!(quiz.phase(), Phase::Rejecting);
    assert!(quiz.cross_visible());
    assert!(
        contains(&frame(&mut pres, &[&cross]), red),
        "red cross should be visible on the first reject frame"
    );

    // Run the flashes out; the game-over sign takes over in gold-shadowed yellow.
    for _ in 0..config.quiz.flash.total_frames() {
        quiz.advance();
    }
    assert_eq!(quiz.phase(), Phase::GameOver);
    assert!(
        contains(&frame(&mut pres, &[&game_over]), yellow),
        "GAME OVER sign should paint its yellow fill"
    );

    // And it loops back to the question.
    for _ in 0..config.quiz.game_over_frames {
        quiz.advance();
    }
    assert_eq!(quiz.phase(), Phase::Asking);
}

#[test]
fn correct_answer_shows_the_green_win_banner() {
    let config = Config::default();
    let win_banner = BigText::new(config.marquee.text_scale)
        .tracking(config.marquee.tracking)
        .shadow_depth(config.marquee.shadow_depth)
        .gap(config.marquee.gap)
        .colors(config.marquee.colors)
        .build(&config.quiz.win_text);
    let win = Marquee::new(win_banner, config.marquee.speed);
    let green = config.marquee.colors.fill;

    let mut pres = presentation(&config);
    let mut quiz = Quiz::from_config(&config.quiz);

    assert_eq!(quiz.submit("12"), Some(ratgames::Outcome::Correct));
    assert_eq!(quiz.phase(), Phase::Won);
    assert!(
        contains(&frame(&mut pres, &[&win]), green),
        "YOU WIN marquee should paint its green fill"
    );
}

#[test]
fn asking_phase_shows_no_banner_colour() {
    let config = Config::default();
    let mut pres = presentation(&config);
    let quiz = Quiz::from_config(&config.quiz);
    assert_eq!(quiz.phase(), Phase::Asking);

    // Nothing but the backdrop: none of the banner fills are on screen.
    let blank = frame(&mut pres, &[]);
    assert!(!contains(&blank, config.quiz.cross.colors.fill));
    assert!(!contains(&blank, config.quiz.game_over.colors.fill));
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
