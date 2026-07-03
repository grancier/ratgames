//! Exact arithmetic: the domain's notion of mathematical **truth**.
//!
//! Every value is a normalized rational over `i64` — integers, fractions,
//! terminating decimals, and percentages are all the *same* underlying value, so
//! equality and comparison are exact and representation-independent:
//! `25% == 1/4 == 0.25`. There is no floating point anywhere; correctness, not
//! approximation, is the contract (foundational invariant: "no floating-point
//! equality").
//!
//! A value's *truth* is separate from its *representation*. [`ExactValue`] is the
//! truth; [`Representation`] records how a value was written (integer, fraction,
//! decimal, percent), which is what conversion problems and answer contracts
//! reason about later.
//!
//! [`Expression`]s combine value and operator [`Token`]s into a single exact
//! value under an [`EvaluationRule`] — the model behind expression-construction
//! (maze) tasks.

use std::cmp::Ordering;
use std::fmt;

/// A normalized rational number: the domain's exact value type.
///
/// Always stored in lowest terms with a positive denominator, so two equal
/// values have identical fields — structural equality *is* value equality, and
/// `Ord`/`Hash` agree with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExactValue {
    num: i64,
    den: i64, // invariant: den > 0, gcd(|num|, den) == 1
}

/// How an [`ExactValue`] was written. The value is the same regardless; the
/// representation is the learning-objective concern (e.g. "answer as a percent").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Representation {
    Integer,
    Fraction,
    Decimal,
    Percent,
}

/// Failure constructing or combining [`ExactValue`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueError {
    /// A zero denominator (or division by a zero value).
    DivideByZero,
    /// The result did not fit in `i64`.
    Overflow,
}

/// Failure parsing a written value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The input was empty (after trimming).
    Empty,
    /// The input was not a recognizable integer, fraction, decimal, or percent.
    Malformed(String),
    /// A fraction with a zero denominator.
    DivideByZero,
    /// A magnitude too large for `i64`.
    Overflow,
}

impl ExactValue {
    /// The value `0`.
    pub const ZERO: ExactValue = ExactValue { num: 0, den: 1 };
    /// The value `1`.
    pub const ONE: ExactValue = ExactValue { num: 1, den: 1 };

    /// The whole number `n`.
    #[must_use]
    pub const fn integer(n: i64) -> Self {
        // den == 1 is already reduced with a positive denominator.
        Self { num: n, den: 1 }
    }

    /// The rational `num / den`, reduced to lowest terms with a positive
    /// denominator. Errors on a zero denominator (or an `i64::MIN` sign flip).
    pub fn rational(num: i64, den: i64) -> Result<Self, ValueError> {
        Self::normalize(num, den)
    }

    #[must_use]
    pub const fn numerator(self) -> i64 {
        self.num
    }

    #[must_use]
    pub const fn denominator(self) -> i64 {
        self.den
    }

    #[must_use]
    pub const fn is_integer(self) -> bool {
        self.den == 1
    }

    /// The whole-number value, when this is an integer.
    #[must_use]
    pub const fn as_integer(self) -> Option<i64> {
        if self.den == 1 { Some(self.num) } else { None }
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.num == 0
    }

    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.num < 0
    }

    /// `self + rhs`, exact. Errors only on `i64` overflow.
    pub fn try_add(self, rhs: Self) -> Result<Self, ValueError> {
        let ad = self.num.checked_mul(rhs.den).ok_or(ValueError::Overflow)?;
        let cb = rhs.num.checked_mul(self.den).ok_or(ValueError::Overflow)?;
        let num = ad.checked_add(cb).ok_or(ValueError::Overflow)?;
        let den = self.den.checked_mul(rhs.den).ok_or(ValueError::Overflow)?;
        Self::normalize(num, den)
    }

    /// `self - rhs`, exact. Errors only on `i64` overflow.
    pub fn try_sub(self, rhs: Self) -> Result<Self, ValueError> {
        let ad = self.num.checked_mul(rhs.den).ok_or(ValueError::Overflow)?;
        let cb = rhs.num.checked_mul(self.den).ok_or(ValueError::Overflow)?;
        let num = ad.checked_sub(cb).ok_or(ValueError::Overflow)?;
        let den = self.den.checked_mul(rhs.den).ok_or(ValueError::Overflow)?;
        Self::normalize(num, den)
    }

    /// `self * rhs`, exact. Errors only on `i64` overflow.
    pub fn try_mul(self, rhs: Self) -> Result<Self, ValueError> {
        let num = self.num.checked_mul(rhs.num).ok_or(ValueError::Overflow)?;
        let den = self.den.checked_mul(rhs.den).ok_or(ValueError::Overflow)?;
        Self::normalize(num, den)
    }

    /// `self / rhs`, exact. Errors on division by zero or `i64` overflow.
    pub fn try_div(self, rhs: Self) -> Result<Self, ValueError> {
        if rhs.num == 0 {
            return Err(ValueError::DivideByZero);
        }
        let num = self.num.checked_mul(rhs.den).ok_or(ValueError::Overflow)?;
        let den = self.den.checked_mul(rhs.num).ok_or(ValueError::Overflow)?;
        Self::normalize(num, den)
    }

    /// Parse a learner's written value, returning the exact value *and* the form
    /// it was written in. Accepts integers (`"347"`, `"-3"`), fractions
    /// (`"3/4"`), terminating decimals (`"0.25"`, `".5"`), and percentages
    /// (`"25%"`, `"12.5%"`). Exact only — nothing is rounded.
    pub fn parse(input: &str) -> Result<(ExactValue, Representation), ParseError> {
        let s = input.trim();
        if s.is_empty() {
            return Err(ParseError::Empty);
        }
        if let Some(body) = s.strip_suffix('%') {
            let (value, _) = Self::parse_numeric(body.trim())?;
            let per_hundred = value
                .try_div(ExactValue::integer(100))
                .map_err(ParseError::from_value)?;
            return Ok((per_hundred, Representation::Percent));
        }
        Self::parse_numeric(s)
    }

    /// The canonical fraction text: `"3/4"`, or `"3"` for whole numbers.
    #[must_use]
    pub fn to_fraction_string(self) -> String {
        if self.den == 1 {
            self.num.to_string()
        } else {
            format!("{}/{}", self.num, self.den)
        }
    }

    /// The exact decimal text (`"0.25"`), or `None` when the value does not
    /// terminate in base 10 (e.g. `1/3`) — we never round.
    #[must_use]
    pub fn to_decimal_string(self) -> Option<String> {
        // A reduced n/d terminates iff d factors into only 2s and 5s.
        let mut d = self.den;
        let mut twos = 0u32;
        while d % 2 == 0 {
            d /= 2;
            twos += 1;
        }
        let mut fives = 0u32;
        while d % 5 == 0 {
            d /= 5;
            fives += 1;
        }
        if d != 1 {
            return None;
        }
        let k = twos.max(fives);
        if k == 0 {
            return Some(self.num.to_string());
        }
        // Scale the numerator to an integer of value * 10^k, then place the point.
        let factor = 2i64
            .checked_pow(k - twos)?
            .checked_mul(5i64.checked_pow(k - fives)?)?;
        let scaled = self.num.checked_mul(factor)?;
        let negative = scaled < 0;
        let digits = scaled.unsigned_abs().to_string();
        let digits = if digits.len() <= k as usize {
            format!("{digits:0>width$}", width = k as usize + 1) // ensure a leading 0
        } else {
            digits
        };
        let point = digits.len() - k as usize;
        let body = format!("{}.{}", &digits[..point], &digits[point..]);
        Some(if negative { format!("-{body}") } else { body })
    }

    /// The exact percent text (`"25%"`, `"12.5%"`), or `None` when the percent
    /// does not terminate in base 10.
    #[must_use]
    pub fn to_percent_string(self) -> Option<String> {
        let hundredfold = self.try_mul(ExactValue::integer(100)).ok()?;
        hundredfold.to_decimal_string().map(|s| format!("{s}%"))
    }

    // ---- internals ----

    fn parse_numeric(s: &str) -> Result<(ExactValue, Representation), ParseError> {
        if let Some((n, d)) = s.split_once('/') {
            let num = Self::parse_i64(n.trim(), s)?;
            let den = Self::parse_i64(d.trim(), s)?;
            let value = ExactValue::rational(num, den).map_err(ParseError::from_value)?;
            return Ok((value, Representation::Fraction));
        }
        if s.contains('.') {
            return Ok((Self::parse_decimal(s)?, Representation::Decimal));
        }
        let n = Self::parse_i64(s, s)?;
        Ok((ExactValue::integer(n), Representation::Integer))
    }

    fn parse_decimal(s: &str) -> Result<ExactValue, ParseError> {
        let (sign, body) = match s.strip_prefix('-') {
            Some(rest) => (-1i64, rest),
            None => (1i64, s.strip_prefix('+').unwrap_or(s)),
        };
        let (int_str, frac_str) = body
            .split_once('.')
            .ok_or_else(|| ParseError::Malformed(s.to_string()))?;
        let all_digits = |t: &str| t.chars().all(|c| c.is_ascii_digit());
        if frac_str.contains('.')
            || (int_str.is_empty() && frac_str.is_empty())
            || !all_digits(int_str)
            || !all_digits(frac_str)
        {
            return Err(ParseError::Malformed(s.to_string()));
        }
        let k = frac_str.len() as u32;
        let denom = 10i64.checked_pow(k).ok_or(ParseError::Overflow)?;
        let int_val = if int_str.is_empty() {
            0
        } else {
            Self::parse_i64(int_str, s)?
        };
        let frac_val = if frac_str.is_empty() {
            0
        } else {
            Self::parse_i64(frac_str, s)?
        };
        let scaled = int_val
            .checked_mul(denom)
            .and_then(|x| x.checked_add(frac_val))
            .and_then(|x| x.checked_mul(sign))
            .ok_or(ParseError::Overflow)?;
        ExactValue::rational(scaled, denom).map_err(ParseError::from_value)
    }

    fn parse_i64(part: &str, whole: &str) -> Result<i64, ParseError> {
        part.parse::<i64>()
            .map_err(|_| ParseError::Malformed(whole.to_string()))
    }

    /// Reduce `num / den` to lowest terms with a positive denominator.
    fn normalize(mut num: i64, mut den: i64) -> Result<Self, ValueError> {
        if den == 0 {
            return Err(ValueError::DivideByZero);
        }
        if den < 0 {
            num = num.checked_neg().ok_or(ValueError::Overflow)?;
            den = den.checked_neg().ok_or(ValueError::Overflow)?;
        }
        let g = Self::gcd(num, den);
        Ok(Self {
            num: num / g,
            den: den / g,
        })
    }

    /// Positive gcd of `a` and `den`, where `den > 0`. Uses non-negative
    /// remainders so it never takes `abs()` (which would overflow on `i64::MIN`).
    fn gcd(a: i64, den: i64) -> i64 {
        let mut x = a;
        let mut y = den;
        while y != 0 {
            let r = x.rem_euclid(y);
            x = y;
            y = r;
        }
        x // > 0 because it divides the positive denominator
    }
}

impl ParseError {
    fn from_value(e: ValueError) -> Self {
        match e {
            ValueError::DivideByZero => ParseError::DivideByZero,
            ValueError::Overflow => ParseError::Overflow,
        }
    }
}

impl Ord for ExactValue {
    fn cmp(&self, other: &Self) -> Ordering {
        // Both denominators are positive, so cross-multiplication preserves the
        // sign; widen to i128 so the products cannot overflow.
        let lhs = i128::from(self.num) * i128::from(other.den);
        let rhs = i128::from(other.num) * i128::from(self.den);
        lhs.cmp(&rhs)
    }
}

impl PartialOrd for ExactValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for ExactValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_fraction_string())
    }
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValueError::DivideByZero => f.write_str("division by zero"),
            ValueError::Overflow => f.write_str("arithmetic overflow"),
        }
    }
}

impl std::error::Error for ValueError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => f.write_str("empty input"),
            ParseError::Malformed(s) => write!(f, "malformed number: {s}"),
            ParseError::DivideByZero => f.write_str("division by zero"),
            ParseError::Overflow => f.write_str("number too large"),
        }
    }
}

impl std::error::Error for ParseError {}

/// A binary arithmetic operator in a constructed expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl Operator {
    /// Apply the operator to two exact values.
    fn apply(self, lhs: ExactValue, rhs: ExactValue) -> Result<ExactValue, ValueError> {
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
mod expression_tests {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(num: i64, den: i64) -> ExactValue {
        ExactValue::rational(num, den).unwrap()
    }

    fn parse(s: &str) -> ExactValue {
        ExactValue::parse(s).unwrap().0
    }

    #[test]
    fn rationals_reduce_and_sign_normalize() {
        assert_eq!(r(2, 4), r(1, 2));
        assert_eq!(r(6, 3), ExactValue::integer(2));
        // The denominator is always positive; a negative sign lives on the top.
        let half_neg = r(1, -2);
        assert_eq!(half_neg.numerator(), -1);
        assert_eq!(half_neg.denominator(), 2);
        assert_eq!(r(-1, -2), r(1, 2));
        assert_eq!(ExactValue::ZERO, r(0, 5));
    }

    #[test]
    fn zero_denominator_is_rejected() {
        assert_eq!(ExactValue::rational(1, 0), Err(ValueError::DivideByZero));
    }

    #[test]
    fn equivalent_forms_are_one_value() {
        // The whole point: 1/2, 2/4, 0.5, and 50% are the same exact value.
        let half = r(1, 2);
        assert_eq!(parse("1/2"), half);
        assert_eq!(parse("2/4"), half);
        assert_eq!(parse("0.5"), half);
        assert_eq!(parse("50%"), half);
        // 25% == 1/4 == 0.25 (a foundational invariant).
        assert_eq!(parse("25%"), r(1, 4));
        assert_eq!(parse("0.25"), r(1, 4));
    }

    #[test]
    fn parse_reports_the_written_representation() {
        assert_eq!(
            ExactValue::parse("347").unwrap(),
            (ExactValue::integer(347), Representation::Integer)
        );
        assert_eq!(
            ExactValue::parse("3/4").unwrap(),
            (r(3, 4), Representation::Fraction)
        );
        assert_eq!(
            ExactValue::parse("0.25").unwrap(),
            (r(1, 4), Representation::Decimal)
        );
        assert_eq!(
            ExactValue::parse("25%").unwrap(),
            (r(1, 4), Representation::Percent)
        );
    }

    #[test]
    fn parse_handles_signs_bare_decimals_and_whitespace() {
        assert_eq!(parse("-3"), ExactValue::integer(-3));
        assert_eq!(parse("  12/8 "), r(3, 2));
        assert_eq!(parse(".5"), r(1, 2));
        assert_eq!(parse("-.25"), r(-1, 4));
        assert_eq!(parse("12.5%"), r(1, 8));
        assert_eq!(parse("1."), ExactValue::integer(1));
    }

    #[test]
    fn parse_rejects_garbage() {
        assert_eq!(ExactValue::parse(""), Err(ParseError::Empty));
        assert_eq!(ExactValue::parse("   "), Err(ParseError::Empty));
        assert_eq!(ExactValue::parse("1/0"), Err(ParseError::DivideByZero));
        assert!(matches!(
            ExactValue::parse("abc"),
            Err(ParseError::Malformed(_))
        ));
        assert!(matches!(
            ExactValue::parse("1.2.3"),
            Err(ParseError::Malformed(_))
        ));
        assert!(matches!(
            ExactValue::parse("."),
            Err(ParseError::Malformed(_))
        ));
    }

    #[test]
    fn ordering_is_by_value_not_by_field() {
        assert!(r(3, 4) > r(2, 3));
        assert!(r(1, 2) < ExactValue::ONE);
        // 1/2 vs 1/3: equal numerators, but 1/2 is larger — field order would lie.
        assert!(r(1, 2) > r(1, 3));
        let mut xs = [r(3, 4), r(1, 4), r(1, 2), ExactValue::ZERO];
        xs.sort();
        assert_eq!(xs, [ExactValue::ZERO, r(1, 4), r(1, 2), r(3, 4)]);
    }

    #[test]
    fn arithmetic_is_exact() {
        assert_eq!(r(1, 2).try_add(r(1, 3)).unwrap(), r(5, 6));
        assert_eq!(r(3, 4).try_sub(r(1, 4)).unwrap(), r(1, 2));
        assert_eq!(r(2, 3).try_mul(r(3, 4)).unwrap(), r(1, 2));
        assert_eq!(r(1, 2).try_div(r(1, 4)).unwrap(), ExactValue::integer(2));
        assert_eq!(
            r(1, 2).try_div(ExactValue::ZERO),
            Err(ValueError::DivideByZero)
        );
    }

    #[test]
    fn overflow_is_reported_not_panicked() {
        let big = ExactValue::integer(i64::MAX);
        assert_eq!(big.try_add(ExactValue::ONE), Err(ValueError::Overflow));
        // i64::MIN cannot be sign-flipped into a positive denominator.
        assert_eq!(ExactValue::rational(1, i64::MIN), Err(ValueError::Overflow));
    }

    #[test]
    fn fraction_formatting() {
        assert_eq!(r(1, 2).to_fraction_string(), "1/2");
        assert_eq!(ExactValue::integer(3).to_fraction_string(), "3");
        assert_eq!(r(-1, 2).to_fraction_string(), "-1/2");
        assert_eq!(r(6, 3).to_fraction_string(), "2");
    }

    #[test]
    fn decimal_formatting_is_exact_or_none() {
        assert_eq!(r(1, 4).to_decimal_string().as_deref(), Some("0.25"));
        assert_eq!(r(1, 2).to_decimal_string().as_deref(), Some("0.5"));
        assert_eq!(r(1, 8).to_decimal_string().as_deref(), Some("0.125"));
        assert_eq!(r(1, 5).to_decimal_string().as_deref(), Some("0.2"));
        assert_eq!(
            ExactValue::integer(5).to_decimal_string().as_deref(),
            Some("5")
        );
        assert_eq!(r(-1, 4).to_decimal_string().as_deref(), Some("-0.25"));
        // 1/3 does not terminate in base 10 — we refuse to round.
        assert_eq!(r(1, 3).to_decimal_string(), None);
    }

    #[test]
    fn percent_formatting_is_exact_or_none() {
        assert_eq!(r(1, 4).to_percent_string().as_deref(), Some("25%"));
        assert_eq!(r(1, 8).to_percent_string().as_deref(), Some("12.5%"));
        assert_eq!(r(1, 2).to_percent_string().as_deref(), Some("50%"));
        assert_eq!(ExactValue::ONE.to_percent_string().as_deref(), Some("100%"));
        assert_eq!(r(1, 3).to_percent_string(), None);
    }

    #[test]
    fn fraction_text_round_trips_through_parse() {
        // Property-style: every small rational survives format -> parse unchanged.
        for num in -20i64..=20 {
            for den in 1i64..=20 {
                let value = r(num, den);
                let reparsed = parse(&value.to_fraction_string());
                assert_eq!(value, reparsed, "round trip failed for {num}/{den}");
            }
        }
    }

    #[test]
    fn addition_matches_an_independent_reference() {
        // Cross-check add() against a straight i128 computation for many pairs.
        for a in -8i64..=8 {
            for b in 1i64..=8 {
                for c in -8i64..=8 {
                    for d in 1i64..=8 {
                        let got = r(a, b).try_add(r(c, d)).unwrap();
                        let ref_num = i128::from(a) * i128::from(d) + i128::from(c) * i128::from(b);
                        let ref_den = i128::from(b) * i128::from(d);
                        // got == ref_num/ref_den  <=>  got.num*ref_den == ref_num*got.den
                        let lhs = i128::from(got.numerator()) * ref_den;
                        let rhs = ref_num * i128::from(got.denominator());
                        assert_eq!(lhs, rhs, "{a}/{b} + {c}/{d}");
                    }
                }
            }
        }
    }
}
