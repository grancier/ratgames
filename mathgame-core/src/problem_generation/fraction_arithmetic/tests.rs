use super::super::test_support::*;
use super::super::*;
use crate::math_core::{ExactValue, Operator};
use crate::rng::Rng;

#[test]
fn fraction_arithmetic_adds_proper_fractions_exactly() {
    // The summit-band shape: three-digit numerators under three-digit
    // denominators, e.g. 212/325 + 128/225.
    let generator =
        FractionArithmetic::new("f-add", "fractions", Operator::Add, 100..=299, 150..=350).unwrap();
    let mut rng = Rng::new(41);
    for _ in 0..200 {
        let e = equation_of(&generator.generate(&mut rng));
        assert_eq!(e.operator(), Operator::Add);
        assert_eq!(e.unknown(), Slot::Result);
        for operand in [e.lhs(), e.rhs()] {
            assert!(operand > ExactValue::ZERO && operand < ExactValue::ONE);
        }
        assert_eq!(e.lhs().try_add(e.rhs()).unwrap(), e.result());
    }
}

#[test]
fn fraction_arithmetic_multiplies_proper_fractions_exactly() {
    let generator =
        FractionArithmetic::new("f-mul", "fractions", Operator::Multiply, 1..=9, 2..=12).unwrap();
    let mut rng = Rng::new(43);
    for _ in 0..200 {
        let e = equation_of(&generator.generate(&mut rng));
        assert_eq!(e.operator(), Operator::Multiply);
        for operand in [e.lhs(), e.rhs()] {
            assert!(operand > ExactValue::ZERO && operand < ExactValue::ONE);
        }
        assert_eq!(e.lhs().try_mul(e.rhs()).unwrap(), e.result());
    }
}

#[test]
fn fraction_arithmetic_rejects_unsupported_operators_and_bad_ranges() {
    for operator in [Operator::Subtract, Operator::Divide] {
        assert_eq!(
            FractionArithmetic::new("f", "b", operator, 1..=9, 2..=12).unwrap_err(),
            GeneratorError::OperatorUnsupported
        );
    }
    // Numerators must start below the denominators, or the proper window
    // under a minimal denominator would be empty.
    assert_eq!(
        FractionArithmetic::new("f", "b", Operator::Add, 5..=9, 5..=12).unwrap_err(),
        GeneratorError::EmptyRange
    );
    // Parts that could overflow a sum's cross-multiplication are refused.
    assert_eq!(
        FractionArithmetic::new("f", "b", Operator::Add, 1..=i64::MAX / 2, 2..=i64::MAX / 2)
            .unwrap_err(),
        GeneratorError::RangeOverflows
    );
}

#[test]
fn fraction_arithmetic_is_deterministic_for_a_seed() {
    let build = || FractionArithmetic::new("f", "b", Operator::Add, 100..=299, 150..=350).unwrap();
    let (first, second) = (build(), build());
    let mut a = Rng::new(47);
    let mut b = Rng::new(47);
    for _ in 0..50 {
        assert_eq!(first.generate(&mut a), second.generate(&mut b));
    }
}
