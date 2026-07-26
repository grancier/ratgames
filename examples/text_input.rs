//! `text_input` — type into the field; press Enter to show it as a big banner.
//!
//! A thin native main over [`demos::text_input`]: the demo lives in the
//! `demos` crate, shared with the browser build; this binary supplies the font
//! choice (the system's default monospace), the window, and the minifb frame
//! loop. Run with `cargo run --example text_input --features minifb`; type,
//! Enter shows it, Backspace edits, Esc quits.

use anyhow::Result;
use demos::DemoCtx;
use ratgames::{FontConfig, MinifbHost, ScreenStack, WindowConfig};

fn main() -> Result<()> {
    let screen = demos::text_input::build(&FontConfig::default())?;

    let window = WindowConfig {
        title: "ratgames: text input".to_string(),
        width: Some(demos::text_input::VIRTUAL.w * 2),
        height: Some(demos::text_input::VIRTUAL.h * 2),
        ..WindowConfig::default()
    };
    let mut host = MinifbHost::new(&window, demos::text_input::presentation())?;
    let mut stack = ScreenStack::new(Box::new(screen));
    let mut ctx = DemoCtx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}
