//! Simplify fraction.
use super::generator::FRACTION_INVARIANT;
use super::{AnswerContract, Generator, GeneratorError, Problem, Prompt, UnreducedFraction};
use crate::curriculum::{BandId, SkillId};
use crate::math_core::Representation;
use crate::rng::Rng;
use std::ops::RangeInclusive;

/// Poses "reduce this fraction to lowest terms": a proper base `p/q` is drawn
/// from `base` and scaled by a `multiplier` `m ≥ 2`, showing `(p·m)/(q·m)` —
/// reducible by construction (`125/500` is `1/4` under `m = 125`). The
/// canonical answer is the reduced value, and the contract demands it written
/// as the lowest-terms fraction ([`AnswerContract::FreeForm`] with
/// `require_reduced`), so echoing the prompt back is wrong even though the
/// value is equal.
#[derive(Debug, Clone)]
pub struct SimplifyFraction {
    skills: Vec<SkillId>,
    band: BandId,
    base: RangeInclusive<i64>,
    multiplier: RangeInclusive<i64>,
}

impl SimplifyFraction {
    /// A simplify generator over proper base fractions from `base`, scaled by
    /// `multiplier`. The base's start is clamped to at least 1 (fraction parts
    /// are positive) and the multiplier's to at least 2 (an unscaled fraction
    /// might already be reduced).
    ///
    /// Errors with [`GeneratorError::EmptyRange`] when the clamped base cannot
    /// supply two distinct values (a proper fraction needs `p < q`) or the
    /// clamped multiplier is empty, and [`GeneratorError::RangeOverflows`] when
    /// a scaled part could overflow `i64`.
    pub fn new(
        skill: impl Into<SkillId>,
        band: impl Into<BandId>,
        base: RangeInclusive<i64>,
        multiplier: RangeInclusive<i64>,
    ) -> Result<Self, GeneratorError> {
        let base = (*base.start()).max(1)..=*base.end();
        let multiplier = (*multiplier.start()).max(2)..=*multiplier.end();
        if base.start() >= base.end() || multiplier.is_empty() {
            return Err(GeneratorError::EmptyRange);
        }
        if base.end().checked_mul(*multiplier.end()).is_none() {
            return Err(GeneratorError::RangeOverflows);
        }
        Ok(Self {
            skills: vec![skill.into()],
            band: band.into(),
            base,
            multiplier,
        })
    }
}

impl Generator for SimplifyFraction {
    fn generate(&self, rng: &mut Rng) -> Problem {
        let (lo, hi) = (*self.base.start(), *self.base.end());
        // A strictly proper base: the denominator leaves room below itself.
        let denominator = rng.int_range(lo + 1..=hi);
        let numerator = rng.int_range(lo..=denominator - 1);
        let multiplier = rng.int_range(self.multiplier.clone());
        let shown = UnreducedFraction::new(
            numerator.checked_mul(multiplier).expect(FRACTION_INVARIANT),
            denominator
                .checked_mul(multiplier)
                .expect(FRACTION_INVARIANT),
        )
        .expect(FRACTION_INVARIANT);
        Problem::new(
            Prompt::Simplify(shown),
            self.skills.clone(),
            self.band.clone(),
            AnswerContract::FreeForm {
                required_representation: Some(Representation::Fraction),
                require_reduced: true,
            },
        )
    }
}

#[cfg(test)]
mod tests;
