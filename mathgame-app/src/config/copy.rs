//! Product copy as data.
/// All user-facing copy — every on-screen string, sourced from JSON like the rest
/// of the app's look, never a Rust literal. Format strings hold `{}` placeholders
/// filled left-to-right by `ratgames::fill_placeholders`. The [`Default`] is
/// deliberately blank so the product copy lives only in `copy.json`; the bundled
/// config supplies it all.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct CopyConfig {
    /// Title-screen banner, e.g. `"MATH GAME"`.
    pub title: String,
    /// The name-entry input prompt, e.g. `"NAME: "`.
    pub name_prompt: String,
    /// The answer input prompt entering play, e.g. `"ANSWER: "`.
    pub answer_prompt: String,
    /// Fallback player name when name entry is left blank, e.g. `"PLAYER"`.
    pub default_player: String,
    /// Score / lives / level HUD template — three `{}` (score, lives, level).
    pub hud: String,
    /// Difficulty-select screen title, e.g. `"SELECT DIFFICULTY"`.
    pub select_difficulty: String,
    /// The attract-loop how-to card.
    pub howto: HowToCopy,
    /// Per-answer verdict text.
    pub verdict: VerdictCopy,
    /// Level-intro card lines.
    pub level_intro: LevelIntroCopy,
    /// Level-clear card lines.
    pub level_clear: LevelClearCopy,
    /// Game-over continue prompt.
    pub continue_prompt: ContinueCopy,
    /// End-of-run result screen.
    pub result: ResultCopy,
    /// High-score board header / footer.
    pub board: BoardCopy,
}

/// The attract-loop how-to card: a title over a list of instruction lines.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct HowToCopy {
    /// Card title, e.g. `"HOW TO PLAY"`.
    pub title: String,
    /// The instruction lines, top to bottom.
    pub lines: Vec<String>,
}

/// Per-answer verdict text: a hit reads `correct`; a miss states the answer
/// (`answer_is`, one `{}`) or falls back to `wrong`; a timeout reads `time_up`.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct VerdictCopy {
    /// Correct-answer verdict, e.g. `"CORRECT"`.
    pub correct: String,
    /// Wrong-answer verdict stating the answer — one `{}`, e.g. `"ANSWER IS {}"`.
    pub answer_is: String,
    /// Wrong-answer fallback when no evaluation is present, e.g. `"WRONG"`.
    pub wrong: String,
    /// Timeout verdict, e.g. `"TIME UP"`.
    pub time_up: String,
}

/// Level-intro card: a `"ROUND {} OF {}"` line (round, total) and a `"{}  GET {}
/// RIGHT"` goal line (difficulty, required successes).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct LevelIntroCopy {
    /// Round header — two `{}` (this round, total rounds).
    pub round: String,
    /// Goal line — two `{}` (difficulty, successes needed).
    pub goal: String,
}

/// Level-clear card: a title, a `"SCORE {}"` line, and an `"ACCURACY {}%"` line.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct LevelClearCopy {
    /// Card title, e.g. `"LEVEL CLEAR"`.
    pub title: String,
    /// Running-score line — one `{}`.
    pub score: String,
    /// Accuracy line — one `{}` (a whole-number percent), e.g. `"ACCURACY {}%"`.
    pub accuracy: String,
}

/// Game-over continue prompt: a title and a `"... {} LEFT"` line (continues left).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct ContinueCopy {
    /// Prompt title, e.g. `"CONTINUE?"`.
    pub title: String,
    /// Prompt line — one `{}` (continues remaining), e.g. `"ENTER TO CONTINUE  {} LEFT"`.
    pub prompt: String,
}

/// End-of-run result screen: the win / game-over title (a configured rank shows
/// over these) and a `"SCORE {}   ENTER"` line.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct ResultCopy {
    /// Plain win title (no rank earned), e.g. `"YOU WIN"`.
    pub win: String,
    /// Plain game-over title, e.g. `"GAME OVER"`.
    pub game_over: String,
    /// Final-score line — one `{}`, e.g. `"SCORE {}   ENTER"`.
    pub score: String,
}

/// High-score board header and footer text.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct BoardCopy {
    /// Board header, e.g. `"HIGH SCORES"`.
    pub header: String,
    /// Board footer hint, e.g. `"PRESS ENTER"`.
    pub footer: String,
}
