//! Shared fixtures for mathgame behavior tests.
use crate::*;
use mathgame_core::{Operator, Prompt};
use ratgames::{AnswerMode, LevelSpec};

/// A one-entry mix over 0..=9 — the plain single-operator spec the
/// behaviour tests drill.
pub(crate) fn spec(operator: OperatorConfig) -> ProblemSpec {
    ProblemSpec {
        operator,
        min: 0,
        max: 9,
        max_distance: None,
        weight: 1,
        multiplier_min: None,
        multiplier_max: None,
        denominator_min: None,
        denominator_max: None,
    }
}

/// One single-operator level over 0..=9, worth 100 a success, five to clear,
/// two misses tolerated — the values these behaviour assertions assume
/// (mirroring the shipped gauntlet's shape).
pub(crate) fn level(operator: OperatorConfig, answer_mode: AnswerMode) -> MathLevel {
    MathLevel {
        name: "LEVEL".to_string(),
        difficulty: "EASY".to_string(),
        rules: LevelSpec {
            required_successes: 5,
            max_failures: 2,
            points_per_success: 100,
            time_limit_frames: 0,
            answer_mode,
        },
        content: Arithmetic {
            problems: vec![spec(operator)],
        },
    }
}

/// A three-level typed-addition gauntlet — the uniform run the earlier
/// single-generator session used to hardcode.
pub(crate) fn typed_levels() -> Vec<MathLevel> {
    vec![level(OperatorConfig::Add, AnswerMode::Typed); 3]
}

pub(crate) fn new_session(seed: u64) -> MathgameSession {
    MathgameSession::from_levels(&typed_levels(), 3, seed).unwrap()
}

pub(crate) fn mc_session(seed: u64, options: usize) -> MathgameSession {
    let levels = vec![level(OperatorConfig::Add, AnswerMode::MultipleChoice { options }); 3];
    MathgameSession::from_levels(&levels, 3, seed).unwrap()
}

/// Assert the session's current problem uses `operator`.
pub(crate) fn assert_operator(session: &MathgameSession, operator: Operator) {
    let Prompt::Equation(equation) = session.current_problem().prompt() else {
        panic!("expected an equation prompt");
    };
    assert_eq!(equation.operator(), operator);
}

/// The display index of the correct choice in the current problem.
pub(crate) fn correct_index(session: &MathgameSession) -> usize {
    let answer = session.current_answer();
    session
        .current_choices()
        .expect("multiple-choice session")
        .iter()
        .position(|choice| *choice == answer)
        .expect("the correct answer is always among the choices")
}

pub(crate) fn answer(session: &MathgameSession) -> String {
    session.current_answer()
}
