//! `room_scroll` — steer a block between rooms; each room is a lettered screen
//! and stepping off an edge scrolls to the neighbour.
//!
//! A thin native main over [`demos::room_scroll`]: the demo (world, letters,
//! block, steering) lives in the `demos` crate, shared with the browser build.
//! Because the demo's room view borrows its `Overworld` each frame it is not a
//! `Screen`; this binary drives the host's primitives directly
//! ([`MinifbHost::is_open`] / [`poll_inputs`](MinifbHost::poll_inputs) /
//! [`render`](MinifbHost::render)) — the escape hatch the host documents for
//! per-frame work. Run with `cargo run --example room_scroll --features minifb`;
//! arrows move, Esc quits.

use anyhow::Result;
use demos::room_scroll::RoomScrollDemo;
use ratgames::{MinifbHost, UiInput, WindowConfig};

fn main() -> Result<()> {
    let mut demo = RoomScrollDemo::new();

    let window = WindowConfig {
        title: "ratgames: room scroll".to_string(),
        width: Some(demos::room_scroll::VIRTUAL.w),
        height: Some(demos::room_scroll::VIRTUAL.h),
        ..WindowConfig::default()
    };
    let mut host = MinifbHost::new(&window, demos::room_scroll::presentation())?;

    let mut quit = false;
    while host.is_open() && !quit {
        for input in host.poll_inputs() {
            match input {
                UiInput::Cancel => quit = true,
                other => demo.steer(other),
            }
        }
        demo.advance(); // drive any in-progress slide
        demo.with_layers(|layers| host.render(layers, &[]))?;
    }
    Ok(())
}
