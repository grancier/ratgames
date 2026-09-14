use super::super::test_support::*;
use super::super::*;
use crate::math_core::Representation;
use crate::rng::Rng;

#[test]
fn simplify_generator_poses_reducible_proper_fractions() {
    let generator = SimplifyFraction::new("lowest-terms", "fractions", 1..=9, 2..=125).unwrap();
    let mut rng = Rng::new(31);
    for _ in 0..200 {
        let problem = generator.generate(&mut rng);
        let Prompt::Simplify(shown) = *problem.prompt() else {
            panic!("expected a simplify prompt");
        };
        let canonical = problem.canonical_solution();
        // The shown fraction is genuinely reducible and proper...
        assert!(gcd(shown.numerator(), shown.denominator()) >= 2);
        assert!(shown.numerator() < shown.denominator());
        // ...and its reduced value is the canonical answer, still a fraction.
        assert_eq!(shown.value(), canonical);
        assert!(canonical.denominator() > 1, "the answer stays a fraction");
        // The contract demands the answer written as the reduced fraction.
        assert_eq!(
            problem.answer_contract(),
            &AnswerContract::FreeForm {
                required_representation: Some(Representation::Fraction),
                require_reduced: true,
            }
        );
    }
}

#[test]
fn simplify_generator_clamps_and_validates_its_ranges() {
    // Base values below 1 and multipliers below 2 are clamped, not errors.
    assert!(SimplifyFraction::new("s", "b", 0..=5, 1..=3).is_ok());
    // A base without two distinct values cannot build a proper fraction.
    assert_eq!(
        SimplifyFraction::new("s", "b", 4..=4, 2..=3).unwrap_err(),
        GeneratorError::EmptyRange
    );
    // A multiplier range entirely below 2 clamps to empty.
    assert_eq!(
        SimplifyFraction::new("s", "b", 1..=9, 1..=1).unwrap_err(),
        GeneratorError::EmptyRange
    );
    // A scaled part that could overflow i64 is refused up front.
    assert_eq!(
        SimplifyFraction::new("s", "b", 1..=i64::MAX, 2..=i64::MAX).unwrap_err(),
        GeneratorError::RangeOverflows
    );
    // The clamped generator only poses clamped values: base 1..=3 under a
    // multiplier of exactly 2.
    let generator = SimplifyFraction::new("s", "b", 0..=3, 1..=2).unwrap();
    let mut rng = Rng::new(3);
    for _ in 0..100 {
        let problem = generator.generate(&mut rng);
        let Prompt::Simplify(shown) = *problem.prompt() else {
            panic!("expected a simplify prompt");
        };
        assert!(shown.numerator() >= 2);
        assert!(shown.denominator() <= 6);
    }
}

#[test]
fn simplify_generation_is_deterministic_for_a_seed() {
    let build = || SimplifyFraction::new("s", "b", 1..=9, 2..=50).unwrap();
    let (first, second) = (build(), build());
    let mut a = Rng::new(37);
    let mut b = Rng::new(37);
    for _ in 0..50 {
        assert_eq!(first.generate(&mut a), second.generate(&mut b));
    }
}
