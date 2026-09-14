use crate::test_support::*;
use crate::*;
use mathgame_core::Generator;
use mathgame_core::{Operator, Prompt, Rng};
use ratgames::{AnswerMode, LevelSpec};

#[test]
fn a_level_mixes_its_problem_entries_by_weight() {
    // A four-to-one add/multiply mix: both operators appear, addition
    // dominates, and the constrained entry honours its distance cap.
    let content = Arithmetic {
        problems: vec![
            ProblemSpec {
                weight: 4,
                max_distance: Some(3),
                ..spec(OperatorConfig::Add)
            },
            ProblemSpec {
                weight: 1,
                min: 0,
                max: 5,
                ..spec(OperatorConfig::Multiply)
            },
        ],
    };
    let mix = content.generator("MIXED").expect("a valid mix");
    let mut rng = Rng::new(5);
    let (mut adds, mut muls) = (0, 0);
    for _ in 0..300 {
        let problem = mix.generate(&mut rng);
        let Prompt::Equation(equation) = problem.prompt() else {
            panic!("expected an equation prompt");
        };
        let a = equation.lhs().as_integer().unwrap();
        let b = equation.rhs().as_integer().unwrap();
        match equation.operator() {
            Operator::Add => {
                assert!((a - b).abs() <= 3, "operands {a} and {b} drift past 3");
                adds += 1;
            }
            Operator::Multiply => {
                assert!((0..=5).contains(&a) && (0..=5).contains(&b));
                muls += 1;
            }
            other => panic!("unexpected operator {other:?} in the mix"),
        }
    }
    assert_eq!(adds + muls, 300);
    assert!(muls > 0, "the minority entry still poses questions");
    assert!(adds > muls, "the heavier entry dominates: {adds} vs {muls}");
}

#[test]
fn level_config_parses_a_flat_file_with_defaulted_rules() {
    // The problems list is required; omitted rule fields fall back to
    // LevelSpec defaults, an entry's weight to an even share, and its
    // max_distance to unconstrained.
    let config: MathLevel = serde_json::from_str(
        r#"{"name":"NUMBER YARD","difficulty":"EASY","problems":[{"operator":"add","min":0,"max":9}],"answer_mode":{"kind":"multiple_choice","options":4}}"#,
    )
    .expect("valid level file");
    assert_eq!(config.name, "NUMBER YARD");
    assert_eq!(config.content.problems.len(), 1);
    let entry = config.content.problems[0];
    assert_eq!(entry.operator.operator(), Some(Operator::Add));
    assert_eq!(entry.weight, 1, "an omitted weight is an even share");
    assert_eq!(entry.max_distance, None, "omitted distance: unconstrained");
    assert_eq!(
        config.rules.answer_mode,
        AnswerMode::MultipleChoice { options: 4 }
    );
    // required_successes was omitted, so it takes the LevelSpec default.
    assert_eq!(
        config.rules.required_successes,
        LevelSpec::default().required_successes
    );
    assert!(config.content.generator(&config.name).is_ok());
}

#[test]
fn level_config_parses_a_weighted_multi_entry_mix() {
    // The mid-gauntlet shape: double-digit add/sub at 40 each, single-digit
    // multiplication at 20, each with its own distance cap.
    let config: MathLevel = serde_json::from_str(
        r#"{"name":"DOUBLE CREEK","difficulty":"MEDIUM","problems":[
            {"operator":"add","min":5,"max":49,"max_distance":11,"weight":40},
            {"operator":"subtract","min":5,"max":49,"max_distance":11,"weight":40},
            {"operator":"multiply","min":1,"max":9,"max_distance":8,"weight":20}
        ]}"#,
    )
    .expect("valid level file");
    let entries = &config.content.problems;
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].max_distance, Some(11));
    assert_eq!(entries[2].operator.operator(), Some(Operator::Multiply));
    assert_eq!(entries[2].weight, 20);
    assert!(config.content.generator(&config.name).is_ok());
}

#[test]
fn level_config_parses_fraction_entries_with_their_own_ranges() {
    // The summit-band shape: a simplify drill scaled up to 125/500-style
    // prompts, and three-digit fraction addition.
    let config: MathLevel = serde_json::from_str(
        r#"{"name":"THE SUMMIT","difficulty":"HARD","problems":[
            {"operator":"simplify","min":1,"max":9,"multiplier_min":2,"multiplier_max":125,"weight":10},
            {"operator":"fraction_add","min":100,"max":299,"denominator_min":150,"denominator_max":350,"weight":10},
            {"operator":"fraction_multiply","min":1,"max":9,"weight":10}
        ]}"#,
    )
    .expect("valid level file");
    let entries = &config.content.problems;
    assert_eq!(entries[0].operator, OperatorConfig::Simplify);
    assert_eq!(entries[0].operator.operator(), None);
    assert_eq!(entries[0].multiplier_max, Some(125));
    assert_eq!(entries[1].operator, OperatorConfig::FractionAdd);
    assert_eq!(entries[1].denominator_min, Some(150));
    // The multiply entry leans on the denominator defaults (2..=9).
    assert_eq!(entries[2].denominator_max, None);
    assert!(config.content.generator(&config.name).is_ok());
}
