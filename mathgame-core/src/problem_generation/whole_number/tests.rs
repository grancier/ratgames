use super::super::*;
use crate::math_core::Operator;

#[test]
fn generator_rejects_an_empty_range() {
    // Build the reversed range from values so its emptiness is caught at
    // runtime by validation, not by the compiler's lint.
    let (start, end) = (5_i64, 3_i64);
    let err = DirectArithmetic::new("s", "b", Operator::Add, start..=end).unwrap_err();
    assert_eq!(err, GeneratorError::EmptyRange);
    let err = MissingTerm::new("s", "b", Operator::Add, start..=end).unwrap_err();
    assert_eq!(err, GeneratorError::EmptyRange);
}

#[test]
fn generator_rejects_a_range_that_would_overflow() {
    // A product near i64::MAX must be refused, not panic at generation time.
    let err = DirectArithmetic::new("s", "b", Operator::Multiply, 0..=i64::MAX).unwrap_err();
    assert_eq!(err, GeneratorError::RangeOverflows);
    // Addition overflow is caught too.
    let err = DirectArithmetic::new("s", "b", Operator::Add, 0..=i64::MAX).unwrap_err();
    assert_eq!(err, GeneratorError::RangeOverflows);
}

#[test]
fn generator_accepts_a_safe_range() {
    assert!(DirectArithmetic::new("s", "b", Operator::Multiply, 0..=1_000).is_ok());
}
