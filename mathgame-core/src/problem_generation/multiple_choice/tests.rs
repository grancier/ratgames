use super::super::*;
use crate::math_core::Operator;
use crate::rng::Rng;

#[test]
fn simplify_problems_convert_to_multiple_choice() {
    let generator = SimplifyFraction::new("s", "b", 1..=9, 2..=50).unwrap();
    let mut rng = Rng::new(53);
    for _ in 0..50 {
        let problem = generator.generate(&mut rng);
        let canonical = problem.canonical_solution();
        let mc = into_multiple_choice(problem, &mut rng, 4).unwrap();
        let AnswerContract::MultipleChoice { options } = mc.answer_contract() else {
            panic!("expected a multiple-choice contract");
        };
        assert_eq!(options.len(), 4);
        assert_eq!(options.iter().filter(|&&o| o == canonical).count(), 1);
        for (i, option) in options.iter().enumerate() {
            assert!(!option.is_negative());
            assert!(
                !options[i + 1..].contains(option),
                "options must be distinct"
            );
        }
    }
}

#[test]
fn into_multiple_choice_builds_distinct_options_with_one_correct() {
    let generator = DirectArithmetic::new("sums", "addition", Operator::Add, 0..=20).unwrap();
    let mut rng = Rng::new(7);
    for _ in 0..100 {
        let problem = generator.generate(&mut rng);
        let canonical = problem.canonical_solution();
        let mc = into_multiple_choice(problem, &mut rng, 4).unwrap();

        let AnswerContract::MultipleChoice { options } = mc.answer_contract() else {
            panic!("expected a multiple-choice contract");
        };
        assert_eq!(options.len(), 4);
        // Exactly one option is the correct answer.
        assert_eq!(options.iter().filter(|&&o| o == canonical).count(), 1);
        // Options are distinct and non-negative.
        for (i, a) in options.iter().enumerate() {
            assert!(!a.is_negative());
            assert!(!options[i + 1..].contains(a), "options must be distinct");
        }
        // The transform preserves the prompt's canonical answer.
        assert_eq!(mc.canonical_solution(), canonical);
    }
}

#[test]
fn into_multiple_choice_is_deterministic_for_a_seed() {
    let generator = DirectArithmetic::new("s", "b", Operator::Add, 0..=50).unwrap();
    let build = |seed| {
        let mut rng = Rng::new(seed);
        let problem = generator.generate(&mut rng);
        into_multiple_choice(problem, &mut rng, 4).unwrap()
    };
    assert_eq!(build(3), build(3));
}

#[test]
fn into_multiple_choice_needs_at_least_two_options() {
    let generator = DirectArithmetic::new("s", "b", Operator::Add, 0..=9).unwrap();
    let mut rng = Rng::new(1);
    let problem = generator.generate(&mut rng);
    assert_eq!(
        into_multiple_choice(problem, &mut rng, 1),
        Err(MultipleChoiceError::TooFewOptions)
    );
}
