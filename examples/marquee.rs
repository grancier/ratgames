//! `marquee` — the ratgames marquee demo: a scrolling oversized-text banner over
//! an anti-aliased input field, in a native framebuffer window.
//!
//! `ratgames` is a library; this is a consumer of it. It reuses the toolkit's
//! own window host: a [`MarqueeScreen`] on a [`ScreenStack`], driven by
//! [`MinifbHost::run`] — the host owns the window, the input pump, and the frame
//! loop, so this example writes none of that. Config comes from the built-in
//! defaults, or a `--config <file>` TOML/JSON file (e.g. `examples/marquee.toml`
//! / `examples/marquee.json`); an optional positional argument overrides the
//! banner text. Run with `cargo run --example marquee --features minifb`.

use anyhow::Result;
use ratgames::{
    ConfigSource, InputField, Marquee, MinifbHost, OverlayLayer, PixelLayer, Presentation, Screen,
    ScreenChange, ScreenStack, SystemFont, UiInput, parse_config_flag,
};

/// The one durable bit of state the host loop watches.
#[derive(Default)]
struct Ctx {
    quit: bool,
}

/// The whole demo as one screen: the scrolling banner (pixel-art world) and the
/// input field (device-space overlay). It owns both, scrolls the banner each
/// tick, and routes typing into the field.
struct MarqueeScreen {
    marquee: Marquee,
    input: InputField,
}

impl Screen<Ctx> for MarqueeScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Char(ch) => self.input.type_char(ch),
            UiInput::Backspace => self.input.backspace(),
            UiInput::Confirm => {
                self.input.submit();
            }
            UiInput::Cancel => ctx.quit = true,
            _ => {}
        }
        ScreenChange::None
    }

    fn tick(&mut self, _ctx: &mut Ctx) -> ScreenChange<Ctx> {
        self.marquee.advance();
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&self.marquee);
        overlays.push(&self.input);
    }
}

fn main() -> Result<()> {
    let (config_path, positionals) = parse_config_flag(std::env::args().skip(1))?;
    let config = ConfigSource::resolve(config_path).load()?;
    let text = positionals
        .into_iter()
        .next()
        .unwrap_or_else(|| "YOU WIN!!".to_string());

    // Pixel-art world: the marquee banner, through the configured glyph source.
    let marquee = Marquee::new(config.marquee.text_sprite(&text)?, config.marquee.speed);
    // Overlay: the input field, using a resolved system font.
    let input = InputField::new(config.input.clone(), SystemFont::load(&config.input.font)?);

    // The host owns the window, framebuffer, and per-frame loop; hand it a ready
    // presentation over the configured (fixed) virtual screen.
    let screen = config.screen;
    let presentation = Presentation::new(
        screen.size,
        screen.backdrop,
        screen.letterbox,
        screen.min_scale,
    );
    let mut host = MinifbHost::new(&config.window, presentation)?;
    let mut stack = ScreenStack::new(Box::new(MarqueeScreen { marquee, input }));
    let mut ctx = Ctx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}
