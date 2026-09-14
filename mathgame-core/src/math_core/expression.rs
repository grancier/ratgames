//! Arithmetic operators and expression evaluation rules.
use super::{ExactValue, ValueError};

/// A binary arithmetic operator in a constructed expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl Operator {
    /// Apply the operator to two exact values, exact. Errors only on `i64`
    /// overflow or division by zero.
    pub fn apply(self, lhs: ExactValue, rhs: ExactValue) -> Result<ExactValue, ValueError> {
        match self {
            Operator::Add => lhs.try_add(rhs),
            Operator::Subtract => lhs.try_sub(rhs),
            Operator::Multiply => lhs.try_mul(rhs),
            Operator::Divide => lhs.try_div(rhs),
        }
    }
}

/// One token of a constructed expression: an operand (a number, fraction, or
/// percent — all exact values) or an operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Value(ExactValue),
    Operator(Operator),
}

/// How a token sequence is combined into a single exact value.
///
/// Ordered by teaching sequence: [`UnorderedSum`](Self::UnorderedSum) first (the
/// maze's initial mode — collect numbers to reach a target, order-independent),
/// then [`OrderedLeftToRight`](Self::OrderedLeftToRight) once operators are
/// introduced. Standard precedence is deliberately absent until a lesson
/// explicitly teaches order of operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationRule {
    /// Sum every value token; operators are not permitted.
    UnorderedSum,
    /// Evaluate strictly left to right with no precedence: `2 + 3 * 4 == 20`.
    OrderedLeftToRight,
}

/// A sequence of tokens the learner constructs (for example by collecting maze
/// tokens), evaluated exactly under an [`EvaluationRule`].
///
/// The evaluator computes mathematical *truth* only; problem constraints (such as
/// "non-negative only") belong to higher layers, not here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expression {
    tokens: Vec<Token>,
}

/// Failure evaluating an [`Expression`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalError {
    /// The expression had no tokens.
    Empty,
    /// The token sequence was not valid for the rule (an operator under
    /// `UnorderedSum`, adjacent values, or a leading/trailing operator).
    Malformed,
    /// An intermediate or final value overflowed `i64`.
    Overflow,
    /// Division by zero.
    DivideByZero,
}

impl From<ValueError> for EvalError {
    fn from(e: ValueError) -> Self {
        match e {
            ValueError::Overflow => EvalError::Overflow,
            ValueError::DivideByZero => EvalError::DivideByZero,
        }
    }
}

impl Expression {
    /// An empty expression; build it with [`push`](Self::push).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An expression from an existing token sequence.
    #[must_use]
    pub fn from_tokens(tokens: Vec<Token>) -> Self {
        Self { tokens }
    }

    /// Append a token.
    pub fn push(&mut self, token: Token) {
        self.tokens.push(token);
    }

    #[must_use]
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Evaluate to a single exact value under `rule`.
    pub fn evaluate(&self, rule: EvaluationRule) -> Result<ExactValue, EvalError> {
        match rule {
            EvaluationRule::UnorderedSum => self.eval_unordered_sum(),
            EvaluationRule::OrderedLeftToRight => self.eval_left_to_right(),
        }
    }

    fn eval_unordered_sum(&self) -> Result<ExactValue, EvalError> {
        if self.tokens.is_empty() {
            return Err(EvalError::Empty);
        }
        let mut acc = ExactValue::ZERO;
        for token in &self.tokens {
            match token {
                Token::Value(v) => acc = acc.try_add(*v)?,
                Token::Operator(_) => return Err(EvalError::Malformed),
            }
        }
        Ok(acc)
    }

    fn eval_left_to_right(&self) -> Result<ExactValue, EvalError> {
        let toks = &self.tokens;
        if toks.is_empty() {
            return Err(EvalError::Empty);
        }
        // A valid sequence is `value (operator value)*`: odd length, values at
        // even indices. Reject the shape up front so the stepping is panic-free.
        if toks.len().is_multiple_of(2) {
            return Err(EvalError::Malformed);
        }
        let mut acc = match toks[0] {
            Token::Value(v) => v,
            Token::Operator(_) => return Err(EvalError::Malformed),
        };
        let mut i = 1;
        while i < toks.len() {
            let op = match toks[i] {
                Token::Operator(op) => op,
                Token::Value(_) => return Err(EvalError::Malformed),
            };
            let rhs = match toks[i + 1] {
                Token::Value(v) => v,
                Token::Operator(_) => return Err(EvalError::Malformed),
            };
            acc = op.apply(acc, rhs)?;
            i += 2;
        }
        Ok(acc)
    }
}

#[cfg(test)]
mod tests;
