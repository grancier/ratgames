use crate::test_support::*;
use crate::*;
use ratgames::{
    AnswerMode, ContinueRules, LevelOutcome, LevelSpec, OneUpRules, RankRule, RankRules, RunPhase,
    ScoringRules, StreakRules,
};

#[test]
fn five_correct_answers_clear_the_first_level_and_award_points() {
    let mut session = new_session(1);

    for _ in 0..4 {
        let report = session.submit_typed_answer(answer(&session));
        assert!(report.correct);
        assert_eq!(report.level_outcome, LevelOutcome::InProgress);
        assert_eq!(report.run_phase, RunPhase::Playing);
    }

    let report = session.submit_typed_answer(answer(&session));

    assert!(report.correct);
    assert_eq!(report.level_outcome, LevelOutcome::Cleared);
    assert_eq!(report.run_phase, RunPhase::Playing);
    assert_eq!(session.run().levels().current(), 1);
    assert_eq!(session.run().score().points(), 500);
    assert_eq!(session.goal().successes(), 0);
    assert_eq!(session.goal().failures(), 0);
}

#[test]
fn with_scoring_applies_the_combo_bonus_and_grants_a_one_up() {
    // A run-wide scoring policy layered over the 100-point base: +10 per combo
    // step, an extra life the first time the score reaches 250.
    let mut session = MathgameSession::from_levels(&typed_levels(), 3, 1)
        .unwrap()
        .with_scoring(ScoringRules {
            streak: StreakRules { bonus_per_step: 10 },
            one_up: OneUpRules {
                max_lives: 5,
                thresholds: vec![250],
            },
            ..Default::default()
        })
        .unwrap();

    session.submit_typed_answer(answer(&session)); // 100 (combo 0)
    session.submit_typed_answer(answer(&session)); // +110 → 210 (combo 10)
    session.submit_typed_answer(answer(&session)); // +120 → 330 (combo 20), crosses 250

    assert_eq!(session.run().score().points(), 330); // base 300 + combo 30
    assert_eq!(session.run().lives().count(), 4); // 3 starting + 1UP
}

#[test]
fn with_scoring_rejects_a_lives_cap_below_the_starting_lives() {
    let bad = MathgameSession::from_levels(&typed_levels(), 3, 1)
        .unwrap()
        .with_scoring(ScoringRules {
            one_up: OneUpRules {
                max_lives: 2, // below the run's 3 starting lives
                thresholds: vec![],
            },
            ..Default::default()
        });
    assert!(matches!(bad, Err(MathgameSessionError::Scoring(_))));
}

#[test]
fn rank_reflects_the_finished_runs_facts() {
    let rules = RankRules {
        rules: vec![RankRule {
            title: "MATH MASTER".to_string(),
            requires_won: true,
            ..Default::default()
        }],
    };
    let mut session = new_session(1);
    assert_eq!(
        session.rank(&rules),
        None,
        "a run still playing has not won"
    );

    for _ in 0..15 {
        let answer = answer(&session);
        session.submit_typed_answer(answer);
    }
    assert_eq!(session.run().phase(), RunPhase::Won);
    assert_eq!(session.rank(&rules), Some("MATH MASTER"));
    assert_eq!(session.tally().successes, 15);
    assert_eq!(session.tally().failures, 0);
}

#[test]
fn a_continue_resumes_the_run_with_a_fresh_problem() {
    let mut session = MathgameSession::from_levels(&typed_levels(), 3, 1)
        .unwrap()
        .with_continues(ContinueRules {
            allowed: 1,
            keep_score: true,
        });
    assert!(!session.can_continue(), "nothing to continue while playing");

    while session.run().phase() == RunPhase::Playing {
        session.submit_typed_answer("9999");
    }
    assert_eq!(session.run().phase(), RunPhase::GameOver);
    assert!(session.can_continue());
    assert_eq!(session.continues_remaining(), 1);

    assert!(session.continue_run());
    assert_eq!(session.run().phase(), RunPhase::Playing);
    assert_eq!(session.run().lives().count(), 3);
    assert!(session.last_result().is_none(), "the old grading is gone");
    // The resumed run poses a live problem: answering it counts.
    let answer = answer(&session);
    assert!(session.submit_typed_answer(answer).correct);
    assert_eq!(session.continues_remaining(), 0);
}

#[test]
fn timeouts_are_misses_that_can_fail_a_level_and_cost_a_life() {
    // A timeout sequences the run exactly like a wrong answer, but grades no
    // answer — the two-miss tolerance then fails the level on the third.
    let mut session = new_session(1);
    for _ in 0..2 {
        let report = session.time_out();
        assert!(!report.correct);
        assert!(report.evaluation.is_none(), "a timeout grades no answer");
        assert_eq!(report.level_outcome, LevelOutcome::InProgress);
        assert_eq!(session.run().lives().count(), 3);
    }
    let failed = session.time_out();
    assert_eq!(failed.level_outcome, LevelOutcome::Failed);
    assert_eq!(session.run().lives().count(), 2);
}

#[test]
fn award_bonus_adds_points_and_time_limit_reads_the_level() {
    // The bundled typed gauntlet is untimed; a bonus still adds to the score.
    let mut session = new_session(1);
    assert_eq!(session.current_time_limit_frames(), 0);
    session.award_bonus(70);
    assert_eq!(session.run().score().points(), 70);

    // A level that sets a time limit reports its per-question budget.
    let timed = MathLevel {
        rules: LevelSpec {
            time_limit_frames: 480,
            ..LevelSpec::default()
        },
        ..level(OperatorConfig::Add, AnswerMode::Typed)
    };
    let timed = MathgameSession::from_levels(&[timed], 3, 1).unwrap();
    assert_eq!(timed.current_time_limit_frames(), 480);
}

#[test]
fn exceeding_the_level_failure_limit_costs_one_life_and_resets_the_goal() {
    let mut session = new_session(1);

    for _ in 0..2 {
        let report = session.submit_typed_answer("9999");
        assert!(!report.correct);
        assert_eq!(report.level_outcome, LevelOutcome::InProgress);
        assert_eq!(session.run().lives().count(), 3);
    }

    let report = session.submit_typed_answer("9999");

    assert!(!report.correct);
    assert_eq!(report.level_outcome, LevelOutcome::Failed);
    assert_eq!(report.run_phase, RunPhase::Playing);
    assert_eq!(session.run().lives().count(), 2);
    assert_eq!(session.goal().successes(), 0);
    assert_eq!(session.goal().failures(), 0);
}

#[test]
fn clearing_three_levels_wins_the_run() {
    let mut session = new_session(1);
    let mut last = None;

    for _ in 0..15 {
        last = Some(session.submit_typed_answer(answer(&session)));
    }

    let report = last.unwrap();
    assert_eq!(report.level_outcome, LevelOutcome::Cleared);
    assert_eq!(report.run_phase, RunPhase::Won);
    assert_eq!(session.run().levels().current(), 3);
    assert_eq!(session.run().score().points(), 1500);
}

#[test]
fn three_failed_levels_end_the_run() {
    let mut session = new_session(1);
    let mut last = None;

    for _ in 0..9 {
        last = Some(session.submit_typed_answer("9999"));
    }

    let report = last.unwrap();
    assert_eq!(report.level_outcome, LevelOutcome::Failed);
    assert_eq!(report.run_phase, RunPhase::GameOver);
    assert_eq!(session.run().lives().count(), 0);
}

#[test]
fn reset_restores_a_full_playable_run() {
    let mut session = new_session(1);
    for _ in 0..9 {
        session.submit_typed_answer("9999");
    }
    assert_eq!(session.run().phase(), RunPhase::GameOver);

    session.reset();
    assert_eq!(session.run().phase(), RunPhase::Playing);
    assert_eq!(session.run().lives().count(), 3);
    assert_eq!(session.run().score().points(), 0);
    assert_eq!(session.run().levels().current(), 0);
}
