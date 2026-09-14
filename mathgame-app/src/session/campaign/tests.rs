use crate::test_support::*;
use crate::*;
use mathgame_core::GeneratorError;
use ratgames::{
    AnswerMode, AnswerModeError, CampaignError, ContinueRules, LevelSpecError, RunPhase,
    ScoringRules,
};

#[test]
fn compiled_campaign_starts_independent_reproducible_runs() {
    let levels = typed_levels();
    let campaign = MathgameCampaign::from_levels(&levels, 3)
        .unwrap()
        .with_scoring(ScoringRules::default())
        .unwrap()
        .with_continues(ContinueRules {
            allowed: 1,
            keep_score: true,
        });
    let mut first = campaign.start(42);
    let mut second = campaign.start(42);
    let mut legacy = MathgameSession::from_levels(&levels, 3, 42).unwrap();
    for _ in 0..15 {
        assert_eq!(first.current_problem(), second.current_problem());
        assert_eq!(first.current_problem(), legacy.current_problem());
        let answer = first.current_answer();
        first.submit_typed_answer(&answer);
        second.submit_typed_answer(&answer);
        legacy.submit_typed_answer(&answer);
    }
    assert_eq!(first.run().phase(), RunPhase::Won);
    let fresh = campaign.start(42);
    assert_eq!(fresh.run().score().points(), 0);
    assert_eq!(fresh.run().levels().current(), 0);
    assert_eq!(fresh.continues_remaining(), 1);
}

#[test]
fn multiple_choice_needs_at_least_two_options() {
    // An unplayable answer mode is caught as the campaign is built, named with
    // its level index.
    assert!(matches!(
        MathgameSession::from_levels(
            &[level(
                OperatorConfig::Add,
                AnswerMode::MultipleChoice { options: 1 }
            )],
            3,
            1,
        ),
        Err(MathgameSessionError::Campaign(CampaignError::Level {
            index: 0,
            source: LevelSpecError::AnswerMode(AnswerModeError::TooFewOptions),
        }))
    ));
}

#[test]
fn from_levels_rejects_an_empty_gauntlet() {
    assert!(matches!(
        MathgameSession::from_levels(&[], 3, 1),
        Err(MathgameSessionError::Campaign(CampaignError::NoLevels))
    ));
}

#[test]
fn from_levels_rejects_a_bad_operand_range() {
    let bad = MathLevel {
        content: Arithmetic {
            problems: vec![ProblemSpec {
                min: 5,
                max: 3, // empty range
                ..spec(OperatorConfig::Add)
            }],
        },
        ..level(OperatorConfig::Add, AnswerMode::Typed)
    };
    assert!(matches!(
        MathgameSession::from_levels(&[bad], 3, 1),
        Err(MathgameSessionError::Generator(_))
    ));
}

#[test]
fn from_levels_rejects_a_level_with_no_problem_entries() {
    let empty = MathLevel {
        content: Arithmetic { problems: vec![] },
        ..level(OperatorConfig::Add, AnswerMode::Typed)
    };
    assert!(matches!(
        MathgameSession::from_levels(&[empty], 3, 1),
        Err(MathgameSessionError::Generator(GeneratorError::EmptyMix))
    ));
}

#[test]
fn from_levels_rejects_a_max_distance_on_division() {
    let bad = MathLevel {
        content: Arithmetic {
            problems: vec![ProblemSpec {
                max_distance: Some(5),
                min: 1,
                ..spec(OperatorConfig::Divide)
            }],
        },
        ..level(OperatorConfig::Divide, AnswerMode::Typed)
    };
    assert!(matches!(
        MathgameSession::from_levels(&[bad], 3, 1),
        Err(MathgameSessionError::Generator(
            GeneratorError::DistanceUnsupported
        ))
    ));
}

#[test]
fn a_fraction_entry_rejects_a_max_distance() {
    let bad = MathLevel {
        content: Arithmetic {
            problems: vec![ProblemSpec {
                min: 1,
                max_distance: Some(3),
                ..spec(OperatorConfig::Simplify)
            }],
        },
        ..level(OperatorConfig::Simplify, AnswerMode::Typed)
    };
    assert!(matches!(
        MathgameSession::from_levels(&[bad], 3, 1),
        Err(MathgameSessionError::Generator(
            GeneratorError::DistanceUnsupported
        ))
    ));
}
