use super::super::test_support::*;
use super::*;
use mathgame_core::{DirectArithmetic, Generator, Operator, Response, Rng, evaluate};
use ratgames::Bitmap8x8;

#[test]
fn a_correct_answer_reads_correct_and_advances() {
    assert_eq!(
        verdict_line(&report(true, RunPhase::Playing, None), &copy().verdict),
        "CORRECT"
    );
    assert_eq!(
        pending_for(&report(true, RunPhase::Playing, None)),
        Pending::Advance
    );
}

#[test]
fn a_wrong_answer_states_the_correct_answer_without_ambiguity() {
    // A real evaluation of a wrong typed answer carries the canonical answer;
    // the verdict must state it as THE ANSWER, not beside WRONG (which reads as
    // if that number were the wrong answer) — the whole point of this rework.
    let generator = DirectArithmetic::new("t", "addition", Operator::Add, 0..=9).unwrap();
    let mut rng = Rng::new(1);
    let problem = generator.generate(&mut rng);
    let expected = problem.canonical_solution().to_fraction_string();
    let evaluation = evaluate(&problem, &Response::Typed("999".into()));
    assert!(!evaluation.is_correct());

    let line = verdict_line(
        &report(false, RunPhase::Playing, Some(evaluation)),
        &copy().verdict,
    );
    assert_eq!(line, format!("ANSWER IS {expected}"));
}

#[test]
fn a_missing_evaluation_falls_back_to_a_bare_wrong() {
    assert_eq!(
        verdict_line(&report(false, RunPhase::Playing, None), &copy().verdict),
        "WRONG"
    );
}

#[test]
fn a_finished_run_hands_off_to_the_result_screen() {
    // A won run ends on a correct answer, a game-over on a wrong one; either
    // way the beat hands off to the result screen instead of advancing.
    for phase in [RunPhase::Won, RunPhase::GameOver] {
        let correct = phase == RunPhase::Won;
        assert_eq!(
            pending_for(&report(correct, phase, None)),
            Pending::Finish(phase)
        );
    }
}

#[test]
fn the_reject_cross_blinks_the_configured_number_of_times() {
    let cfg = cfg();
    // blinks × (on + off) frames per cycle.
    let total = cfg.cross_blink.blinks * (cfg.cross_blink.on_frames + cfg.cross_blink.off_frames);
    let mut cross = reject_cross(&cfg, &Bitmap8x8, Size::new(256, 256));
    for _ in 0..total - 1 {
        cross.advance();
        assert!(!cross.is_done());
    }
    cross.advance();
    assert!(cross.is_done());
}

#[test]
fn pending_routes_a_cleared_level_and_a_finished_run() {
    // A clear while the run plays on shows the Level Clear tally.
    let clear_playing = AttemptReport {
        correct: true,
        level_outcome: LevelOutcome::Cleared,
        run_phase: RunPhase::Playing,
        evaluation: None,
    };
    assert_eq!(pending_for(&clear_playing), Pending::LevelCleared);

    // A clear that also won the run goes to the result, not the tally.
    let clear_won = AttemptReport {
        correct: true,
        level_outcome: LevelOutcome::Cleared,
        run_phase: RunPhase::Won,
        evaluation: None,
    };
    assert_eq!(pending_for(&clear_won), Pending::Finish(RunPhase::Won));

    // An in-progress answer (or a retry after a lost life) stays on the level.
    assert_eq!(
        pending_for(&report(true, RunPhase::Playing, None)),
        Pending::Advance
    );
}
