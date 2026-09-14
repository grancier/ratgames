//! Exact rational values, parsing, representation, and formatting.
use std::cmp::Ordering;
use std::fmt;
use std::num::IntErrorKind;

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
        part.parse::<i64>().map_err(|e| match e.kind() {
            // A magnitude too large for i64 is an overflow, not garbage input.
            IntErrorKind::PosOverflow | IntErrorKind::NegOverflow => ParseError::Overflow,
            _ => ParseError::Malformed(whole.to_string()),
        })
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

#[cfg(test)]
mod tests;
