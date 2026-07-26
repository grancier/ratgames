//! `marquee` — the ratgames marquee demo: a scrolling oversized-text banner over
//! an anti-aliased input field, in a native framebuffer window.
//!
//! A thin native main over [`demos::marquee`]: the demo lives in the `demos`
//! crate, shared with the browser build; this binary supplies the config (the
//! built-in defaults, or a `--config <file>` TOML/JSON file such as
//! `examples/marquee.toml` / `examples/marquee.json`), an optional positional
//! banner text, the window, and the minifb frame loop. Run with
//! `cargo run --example marquee --features minifb`.

use anyhow::Result;
use demos::DemoCtx;
use ratgames::{ConfigSource, MinifbHost, Presentation, ScreenStack, parse_config_flag};

fn main() -> Result<()> {
    let (config_path, positionals) = parse_config_flag(std::env::args().skip(1))?;
    let config = ConfigSource::resolve(config_path).load()?;
    let text = positionals
        .into_iter()
        .next()
        .unwrap_or_else(|| "YOU WIN!!".to_string());

    let screen = demos::marquee::build(&config, &text)?;

    // The host owns the window, framebuffer, and per-frame loop; hand it a ready
    // presentation over the configured (fixed) virtual screen.
    let s = config.screen;
    let presentation = Presentation::new(s.size, s.backdrop, s.letterbox, s.min_scale);
    let mut host = MinifbHost::new(&config.window, presentation)?;
    let mut stack = ScreenStack::new(Box::new(screen));
    let mut ctx = DemoCtx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}
