//! Scenes: rooms and the Zelda-style screen-to-screen scroll.
//!
//! A [`Room`] is one screenful of the world, identified by a [`RoomId`]. A
//! [`RoomMap`] is the graph of rooms and their cardinal [`Direction`]
//! adjacencies. An [`Overworld`] is the active-room controller: it owns a map,
//! tracks the current room, and turns a move in a [`Direction`] into a
//! [`Transition`] — a closed state machine that is either settled in a room or
//! sliding toward an adjacent one. [`RoomView`] renders the current room, or the
//! current-plus-incoming pair offset by the slide's progress.
//!
//! The graph, the controller, the transition (now carrying its destination), and
//! the backdrop slide are all real and deterministically tested. Rooms are still
//! **backdrop-only** (a flat colour); tile/sprite storage *inside* a room is the
//! remaining stub.

use crate::color::Color;
use crate::geometry::{Point, Rect, Size};
use crate::present::PixelLayer;
use crate::surface::Surface;
use std::collections::HashMap;

/// Cardinal direction of a room transition (screen space; `y` grows downward).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// A stable handle to a [`Room`] within a [`RoomMap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoomId(u32);

impl RoomId {
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// The underlying index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One Zelda-style screen that exactly fills the virtual screen.
///
/// **Stub:** currently just an id plus a flat backdrop colour; tile/sprite
/// storage inside a room is TODO.
#[derive(Debug, Clone)]
pub struct Room {
    id: RoomId,
    size: Size,
    backdrop: Color,
}

impl Room {
    #[must_use]
    pub fn new(id: RoomId, size: Size, backdrop: Color) -> Self {
        Self { id, size, backdrop }
    }

    #[must_use]
    pub fn id(&self) -> RoomId {
        self.id
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

/// The room graph: rooms keyed by [`RoomId`], plus one-way `(RoomId, Direction)`
/// adjacencies.
///
/// A room may link to a neighbour that has not been inserted (yet); the map keeps
/// the edge, and [`Overworld`] simply refuses to slide toward a room that does
/// not exist. Keying inserts off [`Room::id`] keeps the room its own single
/// source of identity.
#[derive(Debug, Clone, Default)]
pub struct RoomMap {
    rooms: HashMap<RoomId, Room>,
    links: HashMap<(RoomId, Direction), RoomId>,
}

impl RoomMap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or replace) a room, keyed by its own [`Room::id`].
    pub fn insert(&mut self, room: Room) {
        self.rooms.insert(room.id(), room);
    }

    /// Link `from` to `to` when moving `dir` (one-way).
    pub fn link(&mut self, from: RoomId, dir: Direction, to: RoomId) {
        self.links.insert((from, dir), to);
    }

    /// Link `a` and `b` both ways: `a --dir--> b` and `b --opposite--> a`.
    pub fn connect(&mut self, a: RoomId, dir: Direction, b: RoomId) {
        self.link(a, dir, b);
        self.link(b, dir.opposite(), a);
    }

    #[must_use]
    pub fn room(&self, id: RoomId) -> Option<&Room> {
        self.rooms.get(&id)
    }

    #[must_use]
    pub fn contains(&self, id: RoomId) -> bool {
        self.rooms.contains_key(&id)
    }

    /// The room reached by leaving `id` in `dir`, if that edge is linked. The
    /// target need not exist as a room — callers that require it must check.
    #[must_use]
    pub fn neighbor(&self, id: RoomId, dir: Direction) -> Option<RoomId> {
        self.links.get(&(id, dir)).copied()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }
}

/// Room-to-room scroll: a closed state machine, either settled or sliding toward
/// a `target` room over a fixed number of frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transition {
    #[default]
    Settled,
    Sliding {
        dir: Direction,
        frame: u16,
        frames: u16,
        target: RoomId,
    },
}

impl Transition {
    /// Begin sliding `dir` toward `target` over `frames` frames (at least 1).
    #[must_use]
    pub fn start(dir: Direction, frames: u16, target: RoomId) -> Self {
        Transition::Sliding {
            dir,
            frame: 0,
            frames: frames.max(1),
            target,
        }
    }

    /// Advance one frame. Returns `true` on the frame it settles.
    pub fn advance(&mut self) -> bool {
        // `Transition` is `Copy`, so match by value — no `&mut` binding held
        // across the reassignment of `*self`.
        let (dir, next, frames, target) = match *self {
            Transition::Settled => return false,
            Transition::Sliding {
                dir,
                frame,
                frames,
                target,
            } => (dir, frame + 1, frames, target),
        };
        if next >= frames {
            *self = Transition::Settled;
            true
        } else {
            *self = Transition::Sliding {
                dir,
                frame: next,
                frames,
                target,
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

    /// The destination room while sliding; `None` once settled.
    #[must_use]
    pub fn target(&self) -> Option<RoomId> {
        match self {
            Transition::Settled => None,
            Transition::Sliding { target, .. } => Some(*target),
        }
    }

    #[must_use]
    pub fn is_settled(&self) -> bool {
        matches!(self, Transition::Settled)
    }
}

/// The active-room controller: owns a [`RoomMap`], tracks the current room, and
/// turns a move into a [`Transition`]. Ticking the transition to completion
/// commits the move.
#[derive(Debug, Clone)]
pub struct Overworld {
    map: RoomMap,
    current: RoomId,
    transition: Transition,
    slide_frames: u16,
}

impl Overworld {
    /// Place the player in `start` within `map`, with each room-to-room slide
    /// lasting `slide_frames` frames (clamped to at least 1). Returns `None` if
    /// `start` is not a room in `map`.
    #[must_use]
    pub fn new(map: RoomMap, start: RoomId, slide_frames: u16) -> Option<Self> {
        if !map.contains(start) {
            return None;
        }
        Some(Self {
            map,
            current: start,
            transition: Transition::Settled,
            slide_frames: slide_frames.max(1),
        })
    }

    #[must_use]
    pub fn map(&self) -> &RoomMap {
        &self.map
    }

    #[must_use]
    pub fn current(&self) -> RoomId {
        self.current
    }

    /// The room the player is in. Always present: `current` is checked at
    /// construction and rooms are never removed.
    #[must_use]
    pub fn current_room(&self) -> &Room {
        self.map
            .room(self.current)
            .expect("current room is always present in the map")
    }

    #[must_use]
    pub fn transition(&self) -> Transition {
        self.transition
    }

    /// The room being slid toward, while a slide is in progress.
    #[must_use]
    pub fn target_room(&self) -> Option<&Room> {
        self.transition.target().and_then(|id| self.map.room(id))
    }

    /// Start sliding toward the neighbour in `dir`. A no-op returning `false`
    /// when already sliding, when there is no linked neighbour that way, or when
    /// the linked neighbour is not a room in the map (a dangling link).
    pub fn go(&mut self, dir: Direction) -> bool {
        if !self.transition.is_settled() {
            return false;
        }
        match self.map.neighbor(self.current, dir) {
            Some(target) if self.map.contains(target) => {
                self.transition = Transition::start(dir, self.slide_frames, target);
                true
            }
            _ => false,
        }
    }

    /// Advance an in-progress slide one frame; commit the move (`current` becomes
    /// the target) on the frame it settles. Returns `true` on that settling frame.
    pub fn advance(&mut self) -> bool {
        let target = self.transition.target();
        let settled = self.transition.advance();
        if settled && let Some(id) = target {
            self.current = id;
        }
        settled
    }

    /// A [`RoomView`] snapshot for this frame: the current room, plus the incoming
    /// room and direction while a slide is in progress.
    #[must_use]
    pub fn view(&self) -> RoomView<'_> {
        match self.transition {
            Transition::Sliding { dir, .. } => match self.target_room() {
                Some(incoming) => RoomView::sliding(
                    self.current_room(),
                    incoming,
                    dir,
                    self.transition.progress(),
                ),
                None => RoomView::settled(self.current_room()),
            },
            Transition::Settled => RoomView::settled(self.current_room()),
        }
    }
}

/// Draws the current room — or, mid-slide, the current and incoming rooms offset
/// by the transition's progress — into the virtual screen.
///
/// **Stub:** each room paints as its flat backdrop colour (rooms are
/// backdrop-only for now). The slide *geometry* is real: the two screen-sized
/// rooms tile the viewport exactly and scroll by one screen over the transition.
#[derive(Debug, Clone)]
pub struct RoomView<'a> {
    current: &'a Room,
    incoming: Option<(&'a Room, Direction)>,
    progress: f32,
}

impl<'a> RoomView<'a> {
    /// A still view of `current` (no slide).
    #[must_use]
    pub fn settled(current: &'a Room) -> Self {
        Self {
            current,
            incoming: None,
            progress: 1.0,
        }
    }

    /// A slide from `current` toward `incoming` (the neighbour in `dir`) at
    /// `progress` in `0.0..=1.0`.
    #[must_use]
    pub fn sliding(current: &'a Room, incoming: &'a Room, dir: Direction, progress: f32) -> Self {
        Self {
            current,
            incoming: Some((incoming, dir)),
            progress,
        }
    }

    #[must_use]
    pub fn current(&self) -> &Room {
        self.current
    }
}

impl PixelLayer for RoomView<'_> {
    fn render(&self, screen: &mut Surface) {
        let size = screen.size();
        let Some((incoming, dir)) = self.incoming else {
            screen.fill(self.current.backdrop());
            return;
        };
        // The world shifts opposite the camera's travel; both rooms share the
        // same shift, so they stay exactly adjacent and always tile the screen.
        let d = dir.delta();
        let shift = Point::new(
            (-(d.x as f32) * self.progress * size.w as f32).round() as i32,
            (-(d.y as f32) * self.progress * size.h as f32).round() as i32,
        );
        let incoming_at = Point::new(d.x * size.w as i32 + shift.x, d.y * size.h as i32 + shift.y);
        screen.fill_rect(Rect::new(shift, size), self.current.backdrop());
        screen.fill_rect(Rect::new(incoming_at, size), incoming.backdrop());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: RoomId = RoomId::new(1);
    const B: RoomId = RoomId::new(2);
    const C: RoomId = RoomId::new(3);

    fn room(id: RoomId, color: Color) -> Room {
        Room::new(id, Size::new(4, 4), color)
    }

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
    fn room_id_roundtrips() {
        assert_eq!(RoomId::new(7).get(), 7);
    }

    #[test]
    fn map_inserts_and_looks_up_rooms() {
        let mut map = RoomMap::new();
        assert!(map.is_empty());
        map.insert(room(A, Color::rgb(1, 2, 3)));
        assert_eq!(map.len(), 1);
        assert!(map.contains(A));
        assert!(!map.contains(B));
        assert_eq!(map.room(A).unwrap().backdrop(), Color::rgb(1, 2, 3));
        assert!(map.room(B).is_none());
    }

    #[test]
    fn link_is_one_way_connect_is_both_ways() {
        let mut map = RoomMap::new();
        map.insert(room(A, Color::rgb(0, 0, 0)));
        map.insert(room(B, Color::rgb(0, 0, 0)));
        map.insert(room(C, Color::rgb(0, 0, 0)));

        map.link(A, Direction::East, B);
        assert_eq!(map.neighbor(A, Direction::East), Some(B));
        assert_eq!(map.neighbor(B, Direction::West), None); // one-way only

        map.connect(A, Direction::South, C);
        assert_eq!(map.neighbor(A, Direction::South), Some(C));
        assert_eq!(map.neighbor(C, Direction::North), Some(A)); // opposite auto-linked
    }

    #[test]
    fn transition_carries_target_and_runs_once() {
        let mut t = Transition::start(Direction::East, 2, B);
        assert_eq!(t.target(), Some(B));
        assert!(!t.is_settled());
        assert!((t.progress() - 0.0).abs() < f32::EPSILON);

        assert!(!t.advance()); // frame 1 of 2
        assert!((t.progress() - 0.5).abs() < f32::EPSILON);

        assert!(t.advance()); // reaches 2 -> settles, returns true exactly once
        assert!(t.is_settled());
        assert_eq!(t.target(), None);
        assert!(!t.advance()); // already settled
    }

    #[test]
    fn start_clamps_zero_frames_to_one() {
        let mut t = Transition::start(Direction::North, 0, A);
        assert!(t.advance()); // a 1-frame slide settles on the first advance
    }

    #[test]
    fn overworld_requires_a_present_start() {
        assert!(Overworld::new(RoomMap::new(), A, 4).is_none());

        let mut map = RoomMap::new();
        map.insert(room(A, Color::rgb(0, 0, 0)));
        assert!(Overworld::new(map, A, 4).is_some());
    }

    fn two_room_world() -> Overworld {
        let mut map = RoomMap::new();
        map.insert(room(A, Color::rgb(10, 0, 0)));
        map.insert(room(B, Color::rgb(0, 20, 0)));
        map.connect(A, Direction::East, B);
        Overworld::new(map, A, 2).unwrap()
    }

    #[test]
    fn go_starts_a_slide_toward_an_existing_neighbor() {
        let mut w = two_room_world();
        assert!(w.go(Direction::East));
        assert_eq!(w.current(), A); // not committed until it settles
        assert_eq!(w.transition().target(), Some(B));
        assert_eq!(w.target_room().unwrap().id(), B);
    }

    #[test]
    fn go_into_a_wall_is_a_noop() {
        let mut w = two_room_world();
        assert!(!w.go(Direction::North)); // no neighbour north
        assert!(w.transition().is_settled());
    }

    #[test]
    fn go_is_ignored_mid_slide() {
        let mut w = two_room_world();
        assert!(w.go(Direction::East));
        assert!(!w.go(Direction::East)); // already sliding
        assert!(!w.go(Direction::West));
    }

    #[test]
    fn go_skips_a_dangling_link() {
        let mut map = RoomMap::new();
        map.insert(room(A, Color::rgb(0, 0, 0)));
        map.link(A, Direction::East, B); // B is never inserted
        let mut w = Overworld::new(map, A, 2).unwrap();
        assert!(!w.go(Direction::East));
        assert!(w.transition().is_settled());
    }

    #[test]
    fn advance_commits_the_move_on_settle() {
        let mut w = two_room_world();
        w.go(Direction::East); // 2-frame slide
        assert!(!w.advance()); // frame 1
        assert_eq!(w.current(), A);
        assert!(w.advance()); // settles
        assert_eq!(w.current(), B);
        assert!(w.transition().is_settled());
        assert!(w.target_room().is_none());
    }

    #[test]
    fn advance_while_settled_is_a_noop() {
        let mut w = two_room_world();
        assert!(!w.advance());
        assert_eq!(w.current(), A);
    }

    // ---- rendering ----

    fn pixel(s: &Surface, x: u32, y: u32) -> u32 {
        s.as_slice()[y as usize * 4 + x as usize]
    }

    #[test]
    fn settled_view_fills_the_current_backdrop() {
        let w = two_room_world();
        let view = w.view();
        assert_eq!(view.current().id(), A);

        let mut screen = Surface::new(Size::new(4, 4), Color::rgb(0, 0, 0));
        view.render(&mut screen);
        assert_eq!(pixel(&screen, 0, 0), Color::rgb(10, 0, 0).packed());
        assert_eq!(pixel(&screen, 3, 3), Color::rgb(10, 0, 0).packed());
    }

    #[test]
    fn sliding_east_splits_the_screen_at_the_halfway_point() {
        let cur = room(A, Color::rgb(10, 0, 0));
        let inc = room(B, Color::rgb(0, 20, 0));
        let view = RoomView::sliding(&cur, &inc, Direction::East, 0.5);

        let mut screen = Surface::new(Size::new(4, 4), Color::rgb(0, 0, 0));
        view.render(&mut screen);
        // Camera panned east halfway: current holds the left half, the incoming
        // east room the right half.
        assert_eq!(pixel(&screen, 0, 0), Color::rgb(10, 0, 0).packed());
        assert_eq!(pixel(&screen, 1, 0), Color::rgb(10, 0, 0).packed());
        assert_eq!(pixel(&screen, 2, 0), Color::rgb(0, 20, 0).packed());
        assert_eq!(pixel(&screen, 3, 0), Color::rgb(0, 20, 0).packed());
    }

    #[test]
    fn sliding_at_zero_progress_shows_only_the_current_room() {
        let cur = room(A, Color::rgb(10, 0, 0));
        let inc = room(B, Color::rgb(0, 20, 0));
        let view = RoomView::sliding(&cur, &inc, Direction::East, 0.0);

        let mut screen = Surface::new(Size::new(4, 4), Color::rgb(0, 0, 0));
        view.render(&mut screen);
        for x in 0..4 {
            assert_eq!(pixel(&screen, x, 0), Color::rgb(10, 0, 0).packed());
        }
    }

    #[test]
    fn sliding_north_brings_the_incoming_room_in_from_the_top() {
        let cur = room(A, Color::rgb(10, 0, 0));
        let inc = room(C, Color::rgb(0, 0, 30));
        let view = RoomView::sliding(&cur, &inc, Direction::North, 0.5);

        let mut screen = Surface::new(Size::new(4, 4), Color::rgb(0, 0, 0));
        view.render(&mut screen);
        // Moving north, the northern room slides in from the top; current sinks.
        assert_eq!(pixel(&screen, 0, 0), Color::rgb(0, 0, 30).packed());
        assert_eq!(pixel(&screen, 0, 1), Color::rgb(0, 0, 30).packed());
        assert_eq!(pixel(&screen, 0, 2), Color::rgb(10, 0, 0).packed());
        assert_eq!(pixel(&screen, 0, 3), Color::rgb(10, 0, 0).packed());
    }
}
