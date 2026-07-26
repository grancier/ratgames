//! `level_complete` — press Enter to reveal a 32px, drop-shadowed "YOU WIN!"
//! banner centred on the screen.
//!
//! A thin native main over [`demos::level_complete`] (two chained
//! `PromptScreen`s): the demo lives in the `demos` crate, shared with the
//! browser build; this binary supplies the font choice (the system's default
//! monospace), the window, and the minifb frame loop. Run with
//! `cargo run --example level_complete --features minifb`; Enter reveals the
//! banner, Enter or Esc (or close) quits from there.

use anyhow::Result;
use demos::DemoCtx;
use ratgames::{FontConfig, MinifbHost, ScreenStack, WindowConfig};

fn main() -> Result<()> {
    let screen = demos::level_complete::build(&FontConfig::default())?;

    let window = WindowConfig {
        title: "ratgames: level complete".to_string(),
        width: Some(demos::level_complete::VIRTUAL.w * 2),
        height: Some(demos::level_complete::VIRTUAL.h * 2),
        ..WindowConfig::default()
    };
    let mut host = MinifbHost::new(&window, demos::level_complete::presentation())?;
    let mut stack = ScreenStack::new(Box::new(screen));
    let mut ctx = DemoCtx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}
