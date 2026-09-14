//! Whole number.
use super::{AnswerContract, Equation, GeneratorError, Problem, Prompt, Slot};
use crate::curriculum::{BandId, SkillId};
use crate::math_core::{ExactValue, Operator};
use crate::rng::Rng;
use std::ops::RangeInclusive;

/// The `expect` message for arithmetic that construction-time validation
/// ([`validate_operands`]) has already proven cannot overflow.
const INVARIANT: &str = "operand range validated at construction; arithmetic cannot overflow";

/// Check that every equation [`build_equation`] can draw for `operator` over
/// `operands` fits in `i64`, so generation stays panic-free. Bounds are the
/// worst case over the range endpoints.
pub(super) fn validate_operands(
    operator: Operator,
    operands: &RangeInclusive<i64>,
) -> Result<(), GeneratorError> {
    let (lo, hi) = (*operands.start(), *operands.end());
    if lo > hi {
        return Err(GeneratorError::EmptyRange);
    }
    let fits = match operator {
        // Sums span 2*lo ..= 2*hi.
        Operator::Add => lo.checked_add(lo).is_some() && hi.checked_add(hi).is_some(),
        // Differences are non-negative, at most hi - lo.
        Operator::Subtract => hi.checked_sub(lo).is_some(),
        // Products range over {lo*lo, lo*hi, hi*hi}.
        Operator::Multiply => {
            lo.checked_mul(lo).is_some()
                && lo.checked_mul(hi).is_some()
                && hi.checked_mul(hi).is_some()
        }
        // Dividend = divisor * quotient; divisor in 1..=max(hi,1), quotient in lo..=hi.
        Operator::Divide => {
            let max_divisor = hi.max(1);
            max_divisor.checked_mul(lo).is_some() && max_divisor.checked_mul(hi).is_some()
        }
    };
    if fits {
        Ok(())
    } else {
        Err(GeneratorError::RangeOverflows)
    }
}

/// The sub-range of `operands` a companion operand may be drawn from once the
/// first operand is `anchor`: the values within `max_distance` of it, clipped
/// to the range (`None` leaves the whole range). The window always contains
/// `anchor`, which came from the range, so it is never empty; the arithmetic
/// saturates, so a distance wider than the range simply leaves it unclipped.
fn companion_range(
    operands: &RangeInclusive<i64>,
    anchor: i64,
    max_distance: Option<u64>,
) -> RangeInclusive<i64> {
    let Some(distance) = max_distance else {
        return operands.clone();
    };
    let lo = (*operands.start()).max(anchor.saturating_sub_unsigned(distance));
    let hi = (*operands.end()).min(anchor.saturating_add_unsigned(distance));
    lo..=hi
}

/// Build a whole-number equation for `operator` over `operands`, with `unknown`
/// as the hidden slot. When `max_distance` is set the two operands are at most
/// that far apart: the first is drawn from the whole range, the second from the
/// window around it ([`companion_range`]) — uniform per operand, not over
/// pairs. Differences are kept non-negative (negatives are deferred until
/// whole-number mastery) and divisions are exact by construction; division
/// ignores `max_distance`, which its constructors reject.
///
/// Panics only on a bug: generator constructors validate `operands` with
/// [`validate_operands`], so the arithmetic here cannot overflow.
pub(super) fn build_equation(
    operator: Operator,
    operands: &RangeInclusive<i64>,
    max_distance: Option<u64>,
    unknown: Slot,
    rng: &mut Rng,
) -> Equation {
    let (lhs, rhs) = match operator {
        Operator::Add | Operator::Multiply => {
            let a = rng.int_range(operands.clone());
            let b = rng.int_range(companion_range(operands, a, max_distance));
            (a, b)
        }
        Operator::Subtract => {
            let mut a = rng.int_range(operands.clone());
            let mut b = rng.int_range(companion_range(operands, a, max_distance));
            if a < b {
                std::mem::swap(&mut a, &mut b);
            }
            (a, b)
        }
        Operator::Divide => {
            // Exact by construction: dividend = divisor * quotient.
            let divisor = rng.int_range(1..=(*operands.end()).max(1));
            let quotient = rng.int_range(operands.clone());
            let dividend = divisor.checked_mul(quotient).expect(INVARIANT);
            (dividend, divisor)
        }
    };
    Equation::solve(
        ExactValue::integer(lhs),
        operator,
        ExactValue::integer(rhs),
        unknown,
    )
    .expect(INVARIANT)
}

pub(super) fn arithmetic_problem(equation: Equation, skills: &[SkillId], band: &BandId) -> Problem {
    Problem::new(
        Prompt::Equation(equation),
        skills.to_vec(),
        band.clone(),
        AnswerContract::FreeForm {
            required_representation: None,
            require_reduced: false,
        },
    )
}

#[cfg(test)]
mod tests;
