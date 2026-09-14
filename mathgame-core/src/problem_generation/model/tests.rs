use super::super::whole_number::arithmetic_problem;
use super::super::*;
use crate::curriculum::{BandId, SkillId};
use crate::math_core::{ExactValue, Operator, ValueError};

#[test]
fn equation_answer_selects_the_unknown_slot() {
    let e = Equation::new(
        ExactValue::integer(7),
        Operator::Add,
        ExactValue::integer(8),
        ExactValue::integer(15),
        Slot::Lhs,
    )
    .unwrap();
    assert_eq!(e.answer(), ExactValue::integer(7));
}

#[test]
fn equation_new_rejects_a_false_statement() {
    // 2 + 2 = 5 is not a representable equation.
    let err = Equation::new(
        ExactValue::integer(2),
        Operator::Add,
        ExactValue::integer(2),
        ExactValue::integer(5),
        Slot::Result,
    )
    .unwrap_err();
    assert_eq!(err, EquationError::Inconsistent);
}

#[test]
fn equation_new_reports_arithmetic_failure() {
    // Dividing by zero cannot yield any stated result.
    let err = Equation::new(
        ExactValue::integer(1),
        Operator::Divide,
        ExactValue::ZERO,
        ExactValue::ZERO,
        Slot::Result,
    )
    .unwrap_err();
    assert_eq!(err, EquationError::Arithmetic(ValueError::DivideByZero));
}

#[test]
fn equation_solve_computes_a_true_result() {
    let e = Equation::solve(
        ExactValue::integer(2),
        Operator::Add,
        ExactValue::integer(3),
        Slot::Result,
    )
    .unwrap();
    assert_eq!(e.result(), ExactValue::integer(5));
    assert_eq!(e.answer(), ExactValue::integer(5));
}

#[test]
fn problem_canonical_solution_tracks_the_prompt() {
    // The canonical answer is derived from the prompt, so it always equals
    // the equation's hidden slot — there is no way to assert a divergent one.
    let equation = Equation::solve(
        ExactValue::integer(6),
        Operator::Multiply,
        ExactValue::integer(7),
        Slot::Result,
    )
    .unwrap();
    let problem = arithmetic_problem(equation, &[SkillId::from("s")], &BandId::from("b"));
    assert_eq!(problem.canonical_solution(), equation.answer());
    assert_eq!(problem.canonical_solution(), ExactValue::integer(42));
}

#[test]
fn unreduced_fraction_keeps_its_written_form_and_reduces_its_value() {
    let shown = UnreducedFraction::new(125, 500).unwrap();
    assert_eq!((shown.numerator(), shown.denominator()), (125, 500));
    assert_eq!(shown.value(), ExactValue::rational(1, 4).unwrap());
    assert_eq!(
        UnreducedFraction::new(1, 0).unwrap_err(),
        ValueError::DivideByZero
    );
}
