//! Missing term.
use super::whole_number::{arithmetic_problem, build_equation, validate_operands};
use super::{Generator, GeneratorError, Problem, Slot};
use crate::curriculum::{BandId, SkillId};
use crate::math_core::Operator;
use crate::rng::Rng;
use std::ops::RangeInclusive;

/// A missing-term generator: `? op b = r` or `a op ? = r` (an operand is unknown).
#[derive(Debug, Clone)]
pub struct MissingTerm {
    skills: Vec<SkillId>,
    band: BandId,
    operator: Operator,
    operands: RangeInclusive<i64>,
    max_distance: Option<u64>,
}

impl MissingTerm {
    /// A missing-term generator (`? op b = r` or `a op ? = r`) over `operands`,
    /// with no constraint between the operands beyond the range (see
    /// [`with_max_distance`](Self::with_max_distance)).
    ///
    /// Errors with [`GeneratorError`] when the range is empty or admits an
    /// operand combination that would overflow `i64`.
    pub fn new(
        skill: impl Into<SkillId>,
        band: impl Into<BandId>,
        operator: Operator,
        operands: RangeInclusive<i64>,
    ) -> Result<Self, GeneratorError> {
        validate_operands(operator, &operands)?;
        Ok(Self {
            skills: vec![skill.into()],
            band: band.into(),
            operator,
            operands,
            max_distance: None,
        })
    }

    /// Constrain the two operands to lie at most `max_distance` apart
    /// (`|lhs − rhs| ≤ max_distance`); the distance is between the equation's
    /// operands, whichever of them is hidden. `0` forces equal operands; a
    /// distance wider than the range changes nothing.
    ///
    /// Errors with [`GeneratorError::DistanceUnsupported`] for a division
    /// generator, where operand distance has no meaning.
    pub fn with_max_distance(mut self, max_distance: u64) -> Result<Self, GeneratorError> {
        if self.operator == Operator::Divide {
            return Err(GeneratorError::DistanceUnsupported);
        }
        self.max_distance = Some(max_distance);
        Ok(self)
    }
}

impl Generator for MissingTerm {
    fn generate(&self, rng: &mut Rng) -> Problem {
        let unknown = if rng.coin() { Slot::Lhs } else { Slot::Rhs };
        let equation = build_equation(
            self.operator,
            &self.operands,
            self.max_distance,
            unknown,
            rng,
        );
        arithmetic_problem(equation, &self.skills, &self.band)
    }
}

#[cfg(test)]
mod tests;
