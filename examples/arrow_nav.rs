//! `arrow_nav` — steer a block around the screen with the arrow keys.
//!
//! A thin native main over [`demos::arrow_nav`]: the demo itself (screen,
//! layers, values) lives in the `demos` crate, shared with the browser build;
//! this binary only supplies the window and the minifb frame loop. Run with
//! `cargo run --example arrow_nav --features minifb`; arrows move, Esc quits.

use anyhow::Result;
use demos::DemoCtx;
use ratgames::{MinifbHost, ScreenStack, WindowConfig};

fn main() -> Result<()> {
    let window = WindowConfig {
        title: "ratgames: arrow nav".to_string(),
        width: Some(demos::arrow_nav::VIRTUAL.w),
        height: Some(demos::arrow_nav::VIRTUAL.h),
        ..WindowConfig::default()
    };
    let mut host = MinifbHost::new(&window, demos::arrow_nav::presentation())?;
    let mut stack = ScreenStack::new(Box::new(demos::arrow_nav::build()));
    let mut ctx = DemoCtx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}
