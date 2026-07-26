//! `text_wave` — the ratgames `TextWave` effect: a line of big pixel-art text
//! that ripples up and back down, composited through the integer upscale.
//!
//! A thin native main over [`demos::text_wave`]: the demo lives in the `demos`
//! crate, shared with the browser build; this binary supplies the font choice
//! (Menlo bold — a caller decision, like any config), the window, and the
//! minifb frame loop. Run with
//! `cargo run --example text_wave --features minifb`; Esc (or close) quits.

use anyhow::Result;
use demos::DemoCtx;
use ratgames::{
    FontFamily, FontSource, FontStretch, FontStyle, FontWeight, MinifbHost, ScreenStack,
    WindowConfig,
};

fn main() -> Result<()> {
    // A crisp, high-resolution face for the wave's letters; the web build
    // passes the crate-bundled embedded bold instead.
    let font = FontSource::System {
        family: FontFamily::Named("Menlo".to_string()),
        weight: FontWeight(700),
        style: FontStyle::Normal,
        stretch: FontStretch::Normal,
    };
    let screen = demos::text_wave::build(&font)?;

    let window = WindowConfig {
        title: "ratgames: text wave".to_string(),
        width: Some(demos::text_wave::VIRTUAL.w * 2),
        height: Some(demos::text_wave::VIRTUAL.h * 2),
        ..WindowConfig::default()
    };
    let mut host = MinifbHost::new(&window, demos::text_wave::presentation())?;
    let mut stack = ScreenStack::new(Box::new(screen));
    let mut ctx = DemoCtx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}
