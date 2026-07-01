//! Scenes: rooms and the Zelda-style screen-to-screen scroll.
//!
//! A [`Room`] is one screenful of the world. Moving between rooms is a
//! [`Transition`] — a closed state machine that is either settled in a room or
//! sliding toward an adjacent one. The state machine and geometry are real and
//! tested; tile storage and the slide *rendering* are stubbed.

use crate::color::Color;
use crate::geometry::{Point, Size};
use crate::present::PixelLayer;
use crate::surface::Surface;

/// Cardinal direction of a room transition (screen space; `y` grows downward).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    North,
    South,
    East,
    West,
}

impl Direction {
    /// Unit step in screen space.
    #[must_use]
    pub const fn delta(self) -> Point {
        match self {
            Direction::North => Point::new(0, -1),
            Direction::South => Point::new(0, 1),
            Direction::East => Point::new(1, 0),
            Direction::West => Point::new(-1, 0),
        }
    }

    #[must_use]
    pub const fn opposite(self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
            Direction::West => Direction::East,
        }
    }
}

/// One Zelda-style screen that exactly fills the virtual screen.
///
/// **Stub:** currently just a flat backdrop colour; tile/sprite storage is TODO.
#[derive(Debug, Clone)]
pub struct Room {
    size: Size,
    backdrop: Color,
}

impl Room {
    #[must_use]
    pub fn new(size: Size, backdrop: Color) -> Self {
        Self { size, backdrop }
    }

    #[must_use]
    pub fn size(&self) -> Size {
        self.size
    }

    #[must_use]
    pub fn backdrop(&self) -> Color {
        self.backdrop
    }
}

/// Draws a room into the virtual screen.
///
/// **Stub:** paints the room's flat backdrop. Real tile/sprite rendering is TODO.
#[derive(Debug, Clone)]
pub struct RoomView {
    room: Room,
}

impl RoomView {
    #[must_use]
    pub fn new(room: Room) -> Self {
        Self { room }
    }

    #[must_use]
    pub fn room(&self) -> &Room {
        &self.room
    }
}

impl PixelLayer for RoomView {
    fn render(&self, screen: &mut Surface) {
        screen.fill(self.room.backdrop());
    }
}

/// Room-to-room scroll: a closed state machine, either settled or sliding toward
/// an adjacent room over a fixed number of frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transition {
    #[default]
    Settled,
    Sliding {
        dir: Direction,
        frame: u16,
        frames: u16,
    },
}

impl Transition {
    /// Begin sliding `dir` over `frames` frames (at least 1).
    #[must_use]
    pub fn start(dir: Direction, frames: u16) -> Self {
        Transition::Sliding {
            dir,
            frame: 0,
            frames: frames.max(1),
        }
    }

    /// Advance one frame. Returns `true` on the frame it settles.
    pub fn advance(&mut self) -> bool {
        // `Transition` is `Copy`, so match by value — no `&mut` binding held
        // across the reassignment of `*self`.
        let (dir, next, frames) = match *self {
            Transition::Settled => return false,
            Transition::Sliding { dir, frame, frames } => (dir, frame + 1, frames),
        };
        if next >= frames {
            *self = Transition::Settled;
            true
        } else {
            *self = Transition::Sliding {
                dir,
                frame: next,
                frames,
            };
            false
        }
    }

    /// Progress in `0.0..=1.0` (`1.0` once settled).
    #[must_use]
    pub fn progress(&self) -> f32 {
        match self {
            Transition::Settled => 1.0,
            Transition::Sliding { frame, frames, .. } => f32::from(*frame) / f32::from(*frames),
        }
    }

    #[must_use]
    pub fn is_settled(&self) -> bool {
        matches!(self, Transition::Settled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_delta_and_opposite() {
        assert_eq!(Direction::North.delta(), Point::new(0, -1));
        assert_eq!(Direction::South.delta(), Point::new(0, 1));
        assert_eq!(Direction::East.delta(), Point::new(1, 0));
        assert_eq!(Direction::West.delta(), Point::new(-1, 0));
        assert_eq!(Direction::East.opposite(), Direction::West);
        assert_eq!(Direction::North.opposite(), Direction::South);
    }

    #[test]
    fn transition_runs_to_completion_once() {
        let mut t = Transition::start(Direction::East, 2);
        assert!(!t.is_settled());
        assert!((t.progress() - 0.0).abs() < f32::EPSILON);

        assert!(!t.advance()); // frame 1 of 2
        assert!((t.progress() - 0.5).abs() < f32::EPSILON);

        assert!(t.advance()); // reaches 2 -> settles, returns true exactly once
        assert!(t.is_settled());
        assert!((t.progress() - 1.0).abs() < f32::EPSILON);

        assert!(!t.advance()); // already settled
    }

    #[test]
    fn start_clamps_zero_frames() {
        let t = Transition::start(Direction::North, 0);
        // One advance from a 1-frame slide settles immediately.
        let mut t = t;
        assert!(t.advance());
    }

    #[test]
    fn room_view_paints_backdrop() {
        let room = Room::new(Size::new(2, 2), Color::rgb(1, 2, 3));
        let view = RoomView::new(room);
        assert_eq!(view.room().size(), Size::new(2, 2));

        let mut screen = Surface::new(Size::new(2, 2), Color::rgb(0, 0, 0));
        view.render(&mut screen);
        assert_eq!(screen.as_slice()[0], Color::rgb(1, 2, 3).packed());
    }
}
