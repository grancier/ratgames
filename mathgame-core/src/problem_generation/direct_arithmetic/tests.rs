use super::super::test_support::*;
use super::super::*;
use crate::curriculum::{BandId, SkillId};
use crate::math_core::Operator;
use crate::rng::Rng;

#[test]
fn direct_addition_is_consistent_and_in_range() {
    let generator = DirectArithmetic::new("sums-to-20", "addition", Operator::Add, 0..=20).unwrap();
    let mut rng = Rng::new(42);
    for _ in 0..200 {
        let problem = generator.generate(&mut rng);
        let e = equation_of(&problem);
        assert_eq!(e.unknown(), Slot::Result);
        assert_eq!(e.operator(), Operator::Add);
        let a = e.lhs().as_integer().unwrap();
        let b = e.rhs().as_integer().unwrap();
        assert!((0..=20).contains(&a) && (0..=20).contains(&b));
        assert_eq!(e.lhs().try_add(e.rhs()).unwrap(), e.result());
        assert_eq!(problem.canonical_solution(), e.result());
        assert_eq!(problem.skills(), &[SkillId::from("sums-to-20")]);
        assert_eq!(problem.band(), &BandId::from("addition"));
        assert_eq!(
            problem.answer_contract(),
            &AnswerContract::FreeForm {
                required_representation: None,
                require_reduced: false,
            }
        );
    }
}

#[test]
fn generation_is_deterministic_for_a_seed() {
    let generator = DirectArithmetic::new("s", "b", Operator::Add, 0..=99).unwrap();
    let mut a = Rng::new(7);
    let mut b = Rng::new(7);
    for _ in 0..50 {
        assert_eq!(generator.generate(&mut a), generator.generate(&mut b));
    }
    // A different seed diverges at least once over many draws.
    let mut c = Rng::new(8);
    let mut d = Rng::new(7);
    let differs = (0..50).any(|_| generator.generate(&mut c) != generator.generate(&mut d));
    assert!(differs);
}

#[test]
fn subtraction_never_goes_negative() {
    let generator =
        DirectArithmetic::new("diff", "subtraction", Operator::Subtract, 0..=20).unwrap();
    let mut rng = Rng::new(1);
    for _ in 0..200 {
        let e = equation_of(&generator.generate(&mut rng));
        assert!(e.lhs() >= e.rhs());
        assert!(!e.result().is_negative());
        assert_eq!(e.lhs().try_sub(e.rhs()).unwrap(), e.result());
    }
}

#[test]
fn multiplication_is_the_product() {
    let generator =
        DirectArithmetic::new("facts", "multiplication", Operator::Multiply, 0..=12).unwrap();
    let mut rng = Rng::new(3);
    for _ in 0..200 {
        let e = equation_of(&generator.generate(&mut rng));
        assert_eq!(e.lhs().try_mul(e.rhs()).unwrap(), e.result());
    }
}

#[test]
fn division_is_exact_by_construction() {
    let generator = DirectArithmetic::new("div", "division", Operator::Divide, 1..=12).unwrap();
    let mut rng = Rng::new(5);
    for _ in 0..200 {
        let e = equation_of(&generator.generate(&mut rng));
        assert!(e.rhs().as_integer().unwrap() >= 1); // non-zero divisor
        assert_eq!(e.lhs().try_div(e.rhs()).unwrap(), e.result());
    }
}

#[test]
fn max_distance_bounds_addition_operands() {
    let generator = DirectArithmetic::new("sums", "addition", Operator::Add, 0..=9)
        .unwrap()
        .with_max_distance(3)
        .unwrap();
    let mut rng = Rng::new(11);
    for _ in 0..200 {
        let e = equation_of(&generator.generate(&mut rng));
        let a = e.lhs().as_integer().unwrap();
        let b = e.rhs().as_integer().unwrap();
        assert!((0..=9).contains(&a) && (0..=9).contains(&b));
        assert!((a - b).abs() <= 3, "operands {a} and {b} drift past 3");
        assert_eq!(e.lhs().try_add(e.rhs()).unwrap(), e.result());
    }
}

#[test]
fn max_distance_bounds_subtraction_and_stays_non_negative() {
    let generator = DirectArithmetic::new("diff", "subtraction", Operator::Subtract, 0..=20)
        .unwrap()
        .with_max_distance(8)
        .unwrap();
    let mut rng = Rng::new(2);
    for _ in 0..200 {
        let e = equation_of(&generator.generate(&mut rng));
        let a = e.lhs().as_integer().unwrap();
        let b = e.rhs().as_integer().unwrap();
        assert!((0..=20).contains(&a) && (0..=20).contains(&b));
        assert!(a >= b, "difference must stay non-negative");
        assert!(a - b <= 8, "operands {a} and {b} drift past 8");
        assert_eq!(e.lhs().try_sub(e.rhs()).unwrap(), e.result());
    }
}

#[test]
fn max_distance_bounds_multiplication_operands() {
    let generator = DirectArithmetic::new("facts", "multiplication", Operator::Multiply, 0..=12)
        .unwrap()
        .with_max_distance(8)
        .unwrap();
    let mut rng = Rng::new(4);
    for _ in 0..200 {
        let e = equation_of(&generator.generate(&mut rng));
        let a = e.lhs().as_integer().unwrap();
        let b = e.rhs().as_integer().unwrap();
        assert!((a - b).abs() <= 8, "operands {a} and {b} drift past 8");
        assert_eq!(e.lhs().try_mul(e.rhs()).unwrap(), e.result());
    }
}

#[test]
fn max_distance_zero_forces_equal_operands() {
    let generator = DirectArithmetic::new("doubles", "addition", Operator::Add, 0..=9)
        .unwrap()
        .with_max_distance(0)
        .unwrap();
    let mut rng = Rng::new(6);
    for _ in 0..100 {
        let e = equation_of(&generator.generate(&mut rng));
        assert_eq!(e.lhs(), e.rhs());
    }
}

#[test]
fn max_distance_wider_than_the_range_is_unconstrained() {
    // u64::MAX also exercises the saturating window arithmetic.
    let generator = DirectArithmetic::new("sums", "addition", Operator::Add, 0..=9)
        .unwrap()
        .with_max_distance(u64::MAX)
        .unwrap();
    let mut rng = Rng::new(8);
    let mut widest = 0;
    for _ in 0..200 {
        let e = equation_of(&generator.generate(&mut rng));
        let a = e.lhs().as_integer().unwrap();
        let b = e.rhs().as_integer().unwrap();
        assert!((0..=9).contains(&a) && (0..=9).contains(&b));
        widest = widest.max((a - b).abs());
    }
    assert_eq!(widest, 9, "the full operand spread should still occur");
}

#[test]
fn max_distance_rejects_division() {
    let err = DirectArithmetic::new("div", "division", Operator::Divide, 1..=12)
        .unwrap()
        .with_max_distance(5)
        .unwrap_err();
    assert_eq!(err, GeneratorError::DistanceUnsupported);
    let err = MissingTerm::new("div", "division", Operator::Divide, 1..=12)
        .unwrap()
        .with_max_distance(5)
        .unwrap_err();
    assert_eq!(err, GeneratorError::DistanceUnsupported);
}

#[test]
fn max_distance_generation_is_deterministic_for_a_seed() {
    let build = || {
        DirectArithmetic::new("s", "b", Operator::Add, 0..=99)
            .unwrap()
            .with_max_distance(10)
            .unwrap()
    };
    let (first, second) = (build(), build());
    let mut a = Rng::new(21);
    let mut b = Rng::new(21);
    for _ in 0..50 {
        assert_eq!(first.generate(&mut a), second.generate(&mut b));
    }
}
