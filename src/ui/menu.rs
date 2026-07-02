//! A selection list and the answer semantics layered on it.
//!
//! [`Menu`] is the shared, pure selection model — an ordered list of labels with
//! a highlighted index and wrap-around navigation, driven by [`UiInput`]. A menu
//! *is* the model for both navigation menus and multiple-choice questions;
//! [`MultipleChoice`] wraps a `Menu` with a known correct index rather than
//! duplicating the state machine.

use super::UiInput;

/// An ordered list of choices with one highlighted. Pure: no rendering, no I/O.
/// Feed it [`UiInput`] and read [`selected`](Self::selected).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    items: Vec<String>,
    selected: usize,
}

impl Menu {
    /// A menu over `items`, highlighting the first. Accepts anything string-like
    /// (`Menu::new(["Start", "Quit"])`).
    #[must_use]
    pub fn new<I, S>(items: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            items: items.into_iter().map(Into::into).collect(),
            selected: 0,
        }
    }

    /// The choice labels, in order.
    #[must_use]
    pub fn items(&self) -> &[String] {
        &self.items
    }

    /// Number of choices.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether there are no choices.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The highlighted index (`0` when empty).
    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The highlighted label, or `None` when empty.
    #[must_use]
    pub fn selected_item(&self) -> Option<&str> {
        self.items.get(self.selected).map(String::as_str)
    }

    /// Highlight `index` if it is in range (otherwise a no-op).
    pub fn select(&mut self, index: usize) {
        if index < self.items.len() {
            self.selected = index;
        }
    }

    /// Highlight the next choice, wrapping to the first.
    pub fn next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1) % self.items.len();
        }
    }

    /// Highlight the previous choice, wrapping to the last.
    pub fn prev(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + self.items.len() - 1) % self.items.len();
        }
    }

    /// Apply a command: `Up`/`Left` move back, `Down`/`Right` forward,
    /// `Confirm` commits. Returns the chosen index only on `Confirm` (and only
    /// when non-empty); navigation returns `None`.
    pub fn handle(&mut self, input: UiInput) -> Option<usize> {
        match input {
            UiInput::Up | UiInput::Left => {
                self.prev();
                None
            }
            UiInput::Down | UiInput::Right => {
                self.next();
                None
            }
            UiInput::Confirm => (!self.is_empty()).then_some(self.selected),
            _ => None,
        }
    }
}

/// A multiple-choice question: a [`Menu`] of options plus the index that is
/// correct. Navigation delegates to the menu; committing grades the highlighted
/// option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipleChoice {
    options: Menu,
    answer: usize,
}

impl MultipleChoice {
    /// A question over `options` whose correct choice is at `answer` (clamped to
    /// a valid index).
    #[must_use]
    pub fn new<I, S>(options: I, answer: usize) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let options = Menu::new(options);
        let answer = answer.min(options.len().saturating_sub(1));
        Self { options, answer }
    }

    /// The underlying selection model (for rendering / navigation).
    #[must_use]
    pub fn options(&self) -> &Menu {
        &self.options
    }

    /// The index of the correct option.
    #[must_use]
    pub fn answer(&self) -> usize {
        self.answer
    }

    /// Whether the highlighted option is the correct one.
    #[must_use]
    pub fn is_correct(&self) -> bool {
        self.options.selected() == self.answer
    }

    /// Apply a command. Navigation moves the highlight; `Confirm` grades and
    /// returns whether the highlighted option was correct.
    pub fn handle(&mut self, input: UiInput) -> Option<bool> {
        match input {
            UiInput::Confirm if !self.options.is_empty() => Some(self.is_correct()),
            other => {
                self.options.handle(other);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_wraps_both_ways() {
        let mut m = Menu::new(["a", "b", "c"]);
        assert_eq!(m.selected(), 0);
        m.next();
        assert_eq!(m.selected(), 1);
        m.prev();
        m.prev(); // wrap 0 -> 2
        assert_eq!(m.selected(), 2);
        m.next(); // wrap 2 -> 0
        assert_eq!(m.selected(), 0);
    }

    #[test]
    fn select_ignores_out_of_range() {
        let mut m = Menu::new(["a", "b"]);
        m.select(1);
        assert_eq!(m.selected(), 1);
        m.select(9); // out of range: no-op
        assert_eq!(m.selected(), 1);
        assert_eq!(m.selected_item(), Some("b"));
    }

    #[test]
    fn handle_navigates_and_confirms() {
        let mut m = Menu::new(["a", "b", "c"]);
        assert_eq!(m.handle(UiInput::Down), None);
        assert_eq!(m.selected(), 1);
        assert_eq!(m.handle(UiInput::Up), None);
        assert_eq!(m.selected(), 0);
        assert_eq!(m.handle(UiInput::Confirm), Some(0));
        // unrelated input is ignored
        assert_eq!(m.handle(UiInput::Char('x')), None);
    }

    #[test]
    fn empty_menu_confirms_to_nothing() {
        let mut m = Menu::new(Vec::<String>::new());
        assert!(m.is_empty());
        assert_eq!(m.selected_item(), None);
        assert_eq!(m.handle(UiInput::Confirm), None);
        m.next(); // no panic on empty
        assert_eq!(m.selected(), 0);
    }

    #[test]
    fn multiple_choice_grades_the_highlighted_option() {
        let mut q = MultipleChoice::new(["3", "4", "5"], 1);
        assert_eq!(q.answer(), 1);
        assert!(!q.is_correct()); // starts on index 0
        assert_eq!(q.handle(UiInput::Down), None); // -> index 1
        assert!(q.is_correct());
        assert_eq!(q.handle(UiInput::Confirm), Some(true));
        // move off the answer and it grades wrong
        q.handle(UiInput::Down);
        assert_eq!(q.handle(UiInput::Confirm), Some(false));
    }

    #[test]
    fn multiple_choice_clamps_answer_to_range() {
        let q = MultipleChoice::new(["a", "b"], 9);
        assert_eq!(q.answer(), 1);
    }
}
