use crate::config::CopyConfig;
use mathgame_app::AttemptReport;
use ratgames::{FeedbackBeatConfig, LevelOutcome, RunPhase};

/// The bundled product feedback config (from `style.json`), so the
/// reject-cross test reads the shipped blink pattern rather than a duplicated
/// Rust literal.
pub(super) fn cfg() -> FeedbackBeatConfig {
    crate::config::AppConfig::resolve(None)
        .expect("bundled config")
        .feedback
}

/// The bundled product copy (from `copy.json`), so string assertions read the
/// real shipped text rather than the neutral `Default`.
pub(super) fn copy() -> CopyConfig {
    crate::config::AppConfig::resolve(None)
        .expect("bundled config")
        .copy
}

pub(super) fn report(
    correct: bool,
    run_phase: RunPhase,
    evaluation: Option<mathgame_core::Evaluation>,
) -> AttemptReport {
    AttemptReport {
        correct,
        level_outcome: LevelOutcome::InProgress,
        run_phase,
        evaluation,
    }
}
