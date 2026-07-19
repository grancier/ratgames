//! Visual effects — animated presentation layers a game steps once per frame.
//!
//! Like [`Marquee`](crate::Marquee) / [`Blink`](crate::Blink) /
//! [`Flash`](crate::Flash), an effect is a drawable layer carrying a little
//! animation state that advances on [`advance`](TextWave::advance). Effects are
//! **pixel-art and integer** — they compose through [`Presentation`](crate::Presentation)'s
//! crisp upscale — so they need no floating-point rasteriser: a
//! [`GlyphSource`](crate::GlyphSource) supplies the glyphs and [`Surface`](crate::Surface)
//! does the blitting. [`TextWave`] is the first: a row of glyphs that ripples up
//! and back down in a delayed cascade.

mod wave;

pub use wave::TextWave;
