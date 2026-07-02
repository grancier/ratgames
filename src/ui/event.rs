//! Backend-agnostic input commands.
//!
//! A windowing backend (minifb in the examples) translates its raw keys into a
//! [`UiInput`], and widgets consume `UiInput` — so the same menu or text field
//! works under any backend and the library keeps no windowing dependency. This
//! is the one seam every interactive widget shares.

/// A single semantic input event a widget can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiInput {
    /// A printable character was typed.
    Char(char),
    /// Delete the character before the caret.
    Backspace,
    /// Delete the character at the caret (forward delete).
    Delete,
    /// Move left — caret back one, or the previous item in a list.
    Left,
    /// Move right — caret forward one, or the next item in a list.
    Right,
    /// Move to the previous item (vertical lists).
    Up,
    /// Move to the next item (vertical lists).
    Down,
    /// Jump to the start.
    Home,
    /// Jump to the end.
    End,
    /// Commit — Enter / confirm the current selection or entry.
    Confirm,
    /// Dismiss — Esc / cancel.
    Cancel,
}
