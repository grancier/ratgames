use super::super::*;
use mathgame_app::{MathLevel, MathgameSession, OperatorConfig};
use ratgames::AnswerMode;

#[test]
fn bundled_levels_form_the_graduated_gauntlet() {
    // The gauntlet is shipped game design: a twelve-level ladder that adds
    // mechanics band by band. Pin its structure — the band boundaries, when
    // each operator enters, the shrinking clock — and leave the tunable
    // ranges/weights/points/labels free to change in the level files.
    let levels =
        profiles::inline_levels(&resolve_levels(None).expect("bundled levels must be valid"));
    assert_eq!(levels.len(), 12);
    assert_eq!(levels[0].name, "NUMBER YARD");

    let ops = |level: &MathLevel| -> Vec<OperatorConfig> {
        level.content.problems.iter().map(|p| p.operator).collect()
    };
    // The opening level drills addition alone; the singles band stays
    // add/sub only.
    assert_eq!(ops(&levels[0]), vec![OperatorConfig::Add]);
    for level in &levels[1..5] {
        assert!(
            ops(level)
                .iter()
                .all(|op| matches!(op, OperatorConfig::Add | OperatorConfig::Subtract)),
            "{}: the singles band mixes only add/sub",
            level.name
        );
    }
    // Multiplication enters mid-ladder as the minority share of an add/sub
    // level; division stays out until the summit band.
    for level in &levels[5..10] {
        let entries = &level.content.problems;
        assert!(
            entries
                .iter()
                .any(|p| p.operator == OperatorConfig::Multiply),
            "{}: the doubles band mixes multiplication in",
            level.name
        );
        assert!(
            entries.iter().all(|p| p.operator != OperatorConfig::Divide),
            "{}: no division before the summit band",
            level.name
        );
        let heaviest_mul = entries
            .iter()
            .filter(|p| p.operator == OperatorConfig::Multiply)
            .map(|p| p.weight)
            .max()
            .expect("a multiply entry exists");
        for entry in entries
            .iter()
            .filter(|p| matches!(p.operator, OperatorConfig::Add | OperatorConfig::Subtract))
        {
            assert!(
                heaviest_mul < entry.weight,
                "{}: multiplication stays the minority share",
                level.name
            );
        }
    }
    // The summit band adds division, triple-digit operands, and fractions.
    for level in &levels[10..] {
        assert!(
            ops(level).contains(&OperatorConfig::Divide),
            "{}: the summit band divides",
            level.name
        );
        assert!(
            level.content.problems.iter().any(|p| p.max >= 100),
            "{}: the summit band reaches triple digits",
            level.name
        );
        assert!(
            ops(level).iter().any(|op| matches!(
                op,
                OperatorConfig::Simplify
                    | OperatorConfig::FractionAdd
                    | OperatorConfig::FractionMultiply
            )),
            "{}: the summit band poses fraction questions",
            level.name
        );
    }
    // Every question is on the clock, and the clock only shrinks as the
    // ladder climbs.
    assert!(levels.iter().all(|l| l.rules.time_limit_frames > 0));
    assert!(
        levels
            .windows(2)
            .all(|w| w[0].rules.time_limit_frames >= w[1].rules.time_limit_frames),
        "time never grows as the ladder climbs"
    );
    // Both play modes ship: the opening level grades typed answers, and the
    // ladder mixes typed with four-option multiple choice.
    assert_eq!(levels[0].rules.answer_mode, AnswerMode::Typed);
    assert!(
        levels
            .iter()
            .any(|l| l.rules.answer_mode == AnswerMode::MultipleChoice { options: 4 })
    );
    // The whole gauntlet builds a playable session.
    assert!(MathgameSession::from_levels(&levels, config_starting_lives(), 1).is_ok());
}

#[test]
fn every_bundled_level_generates_problems() {
    // Building a session validates every level's mix but only poses the
    // first level's questions; this drives each level's generator directly —
    // a hundred problems apiece, none panicking, all formattable.
    use mathgame_core::{Generator, Rng};
    let levels =
        profiles::inline_levels(&resolve_levels(None).expect("bundled levels must be valid"));
    let mut rng = Rng::new(99);
    for level in &levels {
        let mix = level
            .content
            .generator(&level.name)
            .expect("every bundled level builds its mix");
        for _ in 0..100 {
            let problem = mix.generate(&mut rng);
            assert!(!mathgame_app::format_problem(&problem).is_empty());
        }
    }
}

#[test]
fn bundled_scoring_is_valid_and_applies_to_the_shipped_run() {
    // The shipped scoring is game design, but its values are a first cut to be
    // play-tuned — so pin only that it is present, well-formed, and applies
    // cleanly to the shipped run (its lives cap is not below the starting
    // lives). `resolve` already validates the intra-scoring invariants.
    let config = AppConfig::resolve(None).expect("bundled config");
    assert!(
        !config.scoring.one_up.thresholds.is_empty(),
        "the shipped gauntlet configures 1UP thresholds"
    );
    let levels = profiles::inline_levels(&resolve_levels(None).expect("bundled levels"));
    assert!(
        MathgameSession::from_levels(&levels, config.starting_lives, 1)
            .and_then(|session| session.with_scoring(config.scoring.clone()))
            .is_ok(),
        "bundled scoring must apply cleanly to the shipped run"
    );
}

/// The bundled run-wide starting lives, for tests that build a session.
fn config_starting_lives() -> u32 {
    AppConfig::resolve(None)
        .expect("bundled config")
        .starting_lives
}
