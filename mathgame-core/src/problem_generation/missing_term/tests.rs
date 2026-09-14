use super::super::test_support::*;
use super::super::*;
use crate::math_core::Operator;
use crate::rng::Rng;

#[test]
fn missing_term_hides_an_operand_not_the_result() {
    let generator = MissingTerm::new("missing-addend", "addition", Operator::Add, 0..=20).unwrap();
    let mut rng = Rng::new(9);
    let mut saw_lhs = false;
    let mut saw_rhs = false;
    for _ in 0..200 {
        let problem = generator.generate(&mut rng);
        let e = equation_of(&problem);
        assert_ne!(e.unknown(), Slot::Result);
        match e.unknown() {
            Slot::Lhs => saw_lhs = true,
            Slot::Rhs => saw_rhs = true,
            Slot::Result => unreachable!(),
        }
        assert_eq!(e.lhs().try_add(e.rhs()).unwrap(), e.result());
        assert_eq!(problem.canonical_solution(), e.answer());
    }
    assert!(saw_lhs && saw_rhs, "both operand positions should occur");
}

#[test]
fn missing_term_honours_max_distance() {
    let generator = MissingTerm::new("missing-addend", "addition", Operator::Add, 0..=20)
        .unwrap()
        .with_max_distance(4)
        .unwrap();
    let mut rng = Rng::new(13);
    for _ in 0..200 {
        let problem = generator.generate(&mut rng);
        let e = equation_of(&problem);
        let a = e.lhs().as_integer().unwrap();
        let b = e.rhs().as_integer().unwrap();
        assert!((a - b).abs() <= 4, "operands {a} and {b} drift past 4");
        assert_ne!(e.unknown(), Slot::Result);
    }
}
