//! Fraction arithmetic.
use super::generator::FRACTION_INVARIANT;
use super::whole_number::arithmetic_problem;
use super::{Equation, Generator, GeneratorError, Problem, Slot};
use crate::curriculum::{BandId, SkillId};
use crate::math_core::{ExactValue, Operator};
use crate::rng::Rng;
use std::ops::RangeInclusive;

/// Fraction arithmetic over proper fractions: `a/b + c/d = ?` or
/// `a/b × c/d = ?` (`212/325 + 128/225` at the summit band). Denominators come
/// from `denominators`; each numerator is drawn from `numerators` capped below
/// its own denominator, so every operand is proper. Operands are [`ExactValue`]s
/// and so display in reduced form; the answer is checked by value, any equal
/// form accepted.
#[derive(Debug, Clone)]
pub struct FractionArithmetic {
    skills: Vec<SkillId>,
    band: BandId,
    operator: Operator,
    numerators: RangeInclusive<i64>,
    denominators: RangeInclusive<i64>,
}

impl FractionArithmetic {
    /// A fraction-arithmetic generator for `operator` over proper fractions.
    /// The numerators' start is clamped to at least 1 and the denominators' to
    /// at least 2.
    ///
    /// Errors with [`GeneratorError::OperatorUnsupported`] for subtraction and
    /// division (differences could go negative; quotients are a different
    /// drill), [`GeneratorError::EmptyRange`] when a clamped range is empty or
    /// the numerators do not start below the denominators (the proper window
    /// under a minimal denominator would be empty), and
    /// [`GeneratorError::RangeOverflows`] when a sum's cross-multiplication or
    /// a product's parts could overflow `i64`.
    pub fn new(
        skill: impl Into<SkillId>,
        band: impl Into<BandId>,
        operator: Operator,
        numerators: RangeInclusive<i64>,
        denominators: RangeInclusive<i64>,
    ) -> Result<Self, GeneratorError> {
        if !matches!(operator, Operator::Add | Operator::Multiply) {
            return Err(GeneratorError::OperatorUnsupported);
        }
        let numerators = (*numerators.start()).max(1)..=*numerators.end();
        let denominators = (*denominators.start()).max(2)..=*denominators.end();
        if numerators.is_empty()
            || denominators.is_empty()
            || numerators.start() >= denominators.start()
        {
            return Err(GeneratorError::EmptyRange);
        }
        let max_numerator = *numerators.end();
        let max_denominator = *denominators.end();
        // try_add computes n1·d2 + n2·d1 over d1·d2; try_mul, n1·n2 over d1·d2.
        let fits = max_denominator.checked_mul(max_denominator).is_some()
            && match operator {
                Operator::Add => max_numerator
                    .checked_mul(max_denominator)
                    .and_then(|cross| cross.checked_add(cross))
                    .is_some(),
                Operator::Multiply => max_numerator.checked_mul(max_numerator).is_some(),
                Operator::Subtract | Operator::Divide => false, // rejected above
            };
        if !fits {
            return Err(GeneratorError::RangeOverflows);
        }
        Ok(Self {
            skills: vec![skill.into()],
            band: band.into(),
            operator,
            numerators,
            denominators,
        })
    }

    /// One proper fraction: a denominator from the range, a numerator from the
    /// numerator range capped below it.
    fn proper_fraction(&self, rng: &mut Rng) -> ExactValue {
        let denominator = rng.int_range(self.denominators.clone());
        let cap = (*self.numerators.end()).min(denominator - 1);
        let numerator = rng.int_range(*self.numerators.start()..=cap);
        ExactValue::rational(numerator, denominator).expect(FRACTION_INVARIANT)
    }
}

impl Generator for FractionArithmetic {
    fn generate(&self, rng: &mut Rng) -> Problem {
        let lhs = self.proper_fraction(rng);
        let rhs = self.proper_fraction(rng);
        let equation =
            Equation::solve(lhs, self.operator, rhs, Slot::Result).expect(FRACTION_INVARIANT);
        arithmetic_problem(equation, &self.skills, &self.band)
    }
}

#[cfg(test)]
mod tests;
