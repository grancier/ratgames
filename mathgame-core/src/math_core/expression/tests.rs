use super::*;

fn v(n: i64) -> Token {
    Token::Value(ExactValue::integer(n))
}

fn frac(n: i64, d: i64) -> Token {
    Token::Value(ExactValue::rational(n, d).unwrap())
}

#[test]
fn unordered_sum_adds_all_values() {
    let expr = Expression::from_tokens(vec![v(3), v(5), v(12)]);
    assert_eq!(
        expr.evaluate(EvaluationRule::UnorderedSum).unwrap(),
        ExactValue::integer(20)
    );
}

#[test]
fn unordered_sum_mixes_fractions_and_integers_exactly() {
    // 1/2 + 1/4 + 1 == 7/4
    let expr = Expression::from_tokens(vec![frac(1, 2), frac(1, 4), v(1)]);
    assert_eq!(
        expr.evaluate(EvaluationRule::UnorderedSum).unwrap(),
        ExactValue::rational(7, 4).unwrap()
    );
}

#[test]
fn unordered_sum_rejects_operators() {
    let expr = Expression::from_tokens(vec![v(3), Token::Operator(Operator::Add), v(5)]);
    assert_eq!(
        expr.evaluate(EvaluationRule::UnorderedSum),
        Err(EvalError::Malformed)
    );
}

#[test]
fn empty_expression_is_an_error() {
    let expr = Expression::new();
    assert_eq!(
        expr.evaluate(EvaluationRule::UnorderedSum),
        Err(EvalError::Empty)
    );
    assert_eq!(
        expr.evaluate(EvaluationRule::OrderedLeftToRight),
        Err(EvalError::Empty)
    );
}

#[test]
fn left_to_right_ignores_precedence() {
    // 2 + 3 * 4 == 20 left to right (not 14).
    let expr = Expression::from_tokens(vec![
        v(2),
        Token::Operator(Operator::Add),
        v(3),
        Token::Operator(Operator::Multiply),
        v(4),
    ]);
    assert_eq!(
        expr.evaluate(EvaluationRule::OrderedLeftToRight).unwrap(),
        ExactValue::integer(20)
    );
}

#[test]
fn left_to_right_uses_every_operator_exactly() {
    // ((3 - 1) * 4) / 2 == 4
    let expr = Expression::from_tokens(vec![
        v(3),
        Token::Operator(Operator::Subtract),
        v(1),
        Token::Operator(Operator::Multiply),
        v(4),
        Token::Operator(Operator::Divide),
        v(2),
    ]);
    assert_eq!(
        expr.evaluate(EvaluationRule::OrderedLeftToRight).unwrap(),
        ExactValue::integer(4)
    );
}

#[test]
fn left_to_right_reports_divide_by_zero() {
    let expr = Expression::from_tokens(vec![v(4), Token::Operator(Operator::Divide), v(0)]);
    assert_eq!(
        expr.evaluate(EvaluationRule::OrderedLeftToRight),
        Err(EvalError::DivideByZero)
    );
}

#[test]
fn left_to_right_rejects_malformed_sequences() {
    let add = Token::Operator(Operator::Add);
    // leading operator
    assert_eq!(
        Expression::from_tokens(vec![add, v(3)]).evaluate(EvaluationRule::OrderedLeftToRight),
        Err(EvalError::Malformed)
    );
    // two values in a row
    assert_eq!(
        Expression::from_tokens(vec![v(2), v(3)]).evaluate(EvaluationRule::OrderedLeftToRight),
        Err(EvalError::Malformed)
    );
    // trailing operator
    assert_eq!(
        Expression::from_tokens(vec![v(2), add]).evaluate(EvaluationRule::OrderedLeftToRight),
        Err(EvalError::Malformed)
    );
}

#[test]
fn single_value_evaluates_to_itself() {
    let expr = Expression::from_tokens(vec![v(7)]);
    assert_eq!(
        expr.evaluate(EvaluationRule::UnorderedSum).unwrap(),
        ExactValue::integer(7)
    );
    assert_eq!(
        expr.evaluate(EvaluationRule::OrderedLeftToRight).unwrap(),
        ExactValue::integer(7)
    );
}

#[test]
fn overflow_is_reported() {
    let expr = Expression::from_tokens(vec![v(i64::MAX), v(1)]);
    assert_eq!(
        expr.evaluate(EvaluationRule::UnorderedSum),
        Err(EvalError::Overflow)
    );
}

#[test]
fn push_builds_an_expression() {
    let mut expr = Expression::new();
    assert!(expr.is_empty());
    expr.push(v(10));
    expr.push(Token::Operator(Operator::Add));
    expr.push(v(5));
    assert_eq!(expr.len(), 3);
    assert_eq!(expr.tokens().len(), 3);
    assert_eq!(
        expr.evaluate(EvaluationRule::OrderedLeftToRight).unwrap(),
        ExactValue::integer(15)
    );
}
