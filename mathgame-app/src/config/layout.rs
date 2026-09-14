//! Product layout as data in virtual-screen pixels.
use ratgames::{HighScoreLayout, Point, Rect, Size};

/// Where every screen element sits, in virtual-screen pixels — sourced from JSON
/// like the copy, reusing the `ratgames` geometry primitives ([`Point`], [`Rect`],
/// [`HighScoreLayout`]). The [`Default`] is neutral (origin / zero) so the product
/// positions live only in `layout.json`; the bundled config supplies them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    /// Top-left anchor of the score / lives / level HUD line.
    pub hud_at: Point,
    /// Shared left margin (x) for the interstitial / attract / continue / menu text.
    pub screen_x: i32,
    /// Y of the how-to and difficulty-select screen titles.
    pub title_y: i32,
    /// The how-to card's instruction-line Ys, top to bottom.
    pub howto_line_ys: Vec<i32>,
    /// Difficulty menu: first-row Y (at `screen_x`) and the row pitch.
    pub menu_y: i32,
    /// Vertical spacing between difficulty menu rows.
    pub menu_row_pitch: i32,
    /// Play-screen multiple-choice list origin.
    pub choices_at: Point,
    /// Vertical spacing between multiple-choice rows.
    pub choices_row_pitch: i32,
    /// Equation banner anchor in multiple-choice mode (typed mode centres it).
    pub equation_mc_at: Point,
    /// The per-question timer bar's rectangle.
    pub timer_bar: Rect,
    /// The question clock's digital seconds-readout anchor, or `None` (the
    /// neutral default) to show the draining bar alone.
    pub timer_seconds_at: Option<Point>,
    /// Level-intro card line Ys (round, level name, goal).
    pub level_intro_ys: Vec<i32>,
    /// Level-clear card line Ys (title, level name, score, accuracy).
    pub level_clear_ys: Vec<i32>,
    /// Continue-prompt subtitle Y (at `screen_x`).
    pub continue_prompt_y: i32,
    /// The continue prompt's live seconds-remaining digit anchor.
    pub continue_seconds_at: Point,
    /// Result-screen score line anchor.
    pub result_score_at: Point,
    /// The high-score board grid layout.
    pub board: HighScoreLayout,
    /// The board header anchor.
    pub board_header_at: Point,
    /// Gap below the board rows to the footer.
    pub board_footer_gap: i32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        // Neutral: everything at the origin / zero, so an unconfigured run has no
        // product positions baked into Rust — they come from `layout.json`.
        Self {
            hud_at: Point::ORIGIN,
            screen_x: 0,
            title_y: 0,
            howto_line_ys: Vec::new(),
            menu_y: 0,
            menu_row_pitch: 0,
            choices_at: Point::ORIGIN,
            choices_row_pitch: 0,
            equation_mc_at: Point::ORIGIN,
            timer_bar: Rect::new(Point::ORIGIN, Size::new(0, 0)),
            timer_seconds_at: None,
            level_intro_ys: Vec::new(),
            level_clear_ys: Vec::new(),
            continue_prompt_y: 0,
            continue_seconds_at: Point::ORIGIN,
            result_score_at: Point::ORIGIN,
            board: HighScoreLayout {
                origin: Point::ORIGIN,
                row_pitch: 0,
                column_width: 0,
                rows_per_column: 0,
                name_width: 0,
            },
            board_header_at: Point::ORIGIN,
            board_footer_gap: 0,
        }
    }
}
