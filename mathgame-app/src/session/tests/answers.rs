use crate::test_support::*;
use crate::*;
use mathgame_core::Operator;
use ratgames::{AnswerMode, LevelOutcome};

#[test]
fn typed_mode_offers_no_choices() {
    // The default typed session has no multiple-choice options.
    assert!(new_session(1).current_choices().is_none());
}

#[test]
fn multiple_choice_offers_the_configured_options_including_the_answer() {
    let session = mc_session(1, 4);
    let choices = session.current_choices().expect("choices in mc mode");
    assert_eq!(choices.len(), 4);
    assert!(
        choices.contains(&session.current_answer()),
        "the correct answer must be among the choices"
    );
}

#[test]
fn selecting_the_correct_choice_clears_toward_the_level() {
    let mut session = mc_session(1, 4);
    for _ in 0..4 {
        let report = session.submit_choice(correct_index(&session));
        assert!(report.correct);
        assert_eq!(report.level_outcome, LevelOutcome::InProgress);
    }
    let report = session.submit_choice(correct_index(&session));
    assert!(report.correct);
    assert_eq!(report.level_outcome, LevelOutcome::Cleared);
    assert_eq!(session.run().score().points(), 500);
}

#[test]
fn selecting_a_distractor_is_wrong() {
    let mut session = mc_session(1, 4);
    // Any index other than the correct one is a distractor.
    let wrong = (correct_index(&session) + 1) % 4;
    let report = session.submit_choice(wrong);
    assert!(!report.correct);
}

#[test]
fn each_level_drills_its_own_operator() {
    // Clear level 0 (addition), then confirm level 1 poses subtraction — the
    // session swapped generators as the run advanced.
    let levels = vec![
        level(OperatorConfig::Add, AnswerMode::Typed),
        level(OperatorConfig::Subtract, AnswerMode::Typed),
    ];
    let mut session = MathgameSession::from_levels(&levels, 3, 7).unwrap();
    assert_operator(&session, Operator::Add);
    for _ in 0..5 {
        let answer = session.current_answer();
        session.submit_typed_answer(answer);
    }
    assert_eq!(session.run().levels().current(), 1);
    assert_operator(&session, Operator::Subtract);
}

#[test]
fn current_level_name_and_difficulty_track_the_run() {
    let levels = vec![
        MathLevel {
            name: "NUMBER YARD".to_string(),
            difficulty: "EASY".to_string(),
            ..level(OperatorConfig::Add, AnswerMode::Typed)
        },
        MathLevel {
            name: "MINUS MINE".to_string(),
            difficulty: "HARD".to_string(),
            ..level(OperatorConfig::Subtract, AnswerMode::Typed)
        },
    ];
    let mut session = MathgameSession::from_levels(&levels, 3, 1).unwrap();
    assert_eq!(session.current_level_name(), "NUMBER YARD");
    assert_eq!(session.current_difficulty(), "EASY");
    for _ in 0..5 {
        let answer = session.current_answer();
        session.submit_typed_answer(answer);
    }
    assert_eq!(session.current_level_name(), "MINUS MINE");
    assert_eq!(session.current_difficulty(), "HARD");
}

#[test]
fn a_simplify_level_rejects_the_unreduced_echo() {
    // The prompt shows an unreduced fraction; echoing it back has the right
    // value but the wrong (unsimplified) form, and must not pass.
    let simplify = MathLevel {
        content: Arithmetic {
            problems: vec![ProblemSpec {
                min: 1,
                max: 9,
                multiplier_min: Some(2),
                multiplier_max: Some(125),
                ..spec(OperatorConfig::Simplify)
            }],
        },
        ..level(OperatorConfig::Add, AnswerMode::Typed)
    };
    let mut session = MathgameSession::from_levels(&[simplify], 3, 11).unwrap();

    let prompt = session.current_prompt();
    let shown = prompt
        .strip_suffix(" = ?")
        .expect("a simplify prompt ends in ' = ?'");
    assert!(shown.contains('/'), "shows a fraction: {prompt}");
    let echoed = session.submit_typed_answer(shown);
    assert!(!echoed.correct, "echoing {shown} back must not pass");

    // The reduced canonical answer clears the (fresh) question.
    let answer = session.current_answer();
    assert!(
        answer.contains('/'),
        "the answer stays a fraction: {answer}"
    );
    assert!(session.submit_typed_answer(answer).correct);
}

#[test]
fn a_fraction_addition_level_grades_the_exact_sum() {
    let addition = MathLevel {
        content: Arithmetic {
            problems: vec![ProblemSpec {
                min: 100,
                max: 299,
                denominator_min: Some(150),
                denominator_max: Some(350),
                ..spec(OperatorConfig::FractionAdd)
            }],
        },
        ..level(OperatorConfig::Add, AnswerMode::Typed)
    };
    let mut session = MathgameSession::from_levels(&[addition], 3, 13).unwrap();
    let prompt = session.current_prompt();
    assert!(
        prompt.contains(" + ") && prompt.contains('/'),
        "poses fraction addition: {prompt}"
    );
    // The exact sum, in any equal form, is accepted.
    let answer = session.current_answer();
    assert!(session.submit_typed_answer(answer).correct);
}
