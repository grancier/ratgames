//! Model.
use crate::curriculum::{BandId, SkillId};
use crate::math_core::{ExactValue, Operator, Representation, ValueError};

/// Which slot of an [`Equation`] is the unknown (the answer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Lhs,
    Rhs,
    Result,
}

/// Why an [`Equation`] could not be constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquationError {
    /// The stated `result` does not equal `lhs op rhs`: the equation is not true.
    Inconsistent,
    /// Evaluating `lhs op rhs` overflowed `i64` or divided by zero.
    Arithmetic(ValueError),
}

/// An equation with exactly one unknown slot: `347 + 286 = ?` (unknown
/// [`Result`](Slot::Result)) or `? + 8 = 15` (unknown [`Lhs`](Slot::Lhs)).
///
/// An `Equation` is always **true** by construction: every constructor checks
/// that `lhs op rhs == result`, so a false statement like `2 + 2 = 5` cannot be
/// represented. This is the structural guarantee the rest of the domain relies
/// on — a [`Problem`]'s canonical answer is *derived* from its equation, never
/// asserted alongside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Equation {
    lhs: ExactValue,
    operator: Operator,
    rhs: ExactValue,
    result: ExactValue,
    unknown: Slot,
}

impl Equation {
    /// An equation with an explicitly stated `result`, checked for truth.
    ///
    /// Errors with [`EquationError::Inconsistent`] when `lhs op rhs != result`,
    /// or [`EquationError::Arithmetic`] when the operation overflows or divides
    /// by zero. Use [`solve`](Equation::solve) when you want the result computed
    /// for you.
    pub fn new(
        lhs: ExactValue,
        operator: Operator,
        rhs: ExactValue,
        result: ExactValue,
        unknown: Slot,
    ) -> Result<Self, EquationError> {
        let computed = operator
            .apply(lhs, rhs)
            .map_err(EquationError::Arithmetic)?;
        if computed != result {
            return Err(EquationError::Inconsistent);
        }
        Ok(Self {
            lhs,
            operator,
            rhs,
            result,
            unknown,
        })
    }

    /// An equation whose `result` is computed as `lhs op rhs`, so it is true by
    /// construction. Errors ([`EquationError::Arithmetic`]) only when the
    /// operation overflows `i64` or divides by zero.
    pub fn solve(
        lhs: ExactValue,
        operator: Operator,
        rhs: ExactValue,
        unknown: Slot,
    ) -> Result<Self, EquationError> {
        let result = operator
            .apply(lhs, rhs)
            .map_err(EquationError::Arithmetic)?;
        Ok(Self {
            lhs,
            operator,
            rhs,
            result,
            unknown,
        })
    }

    #[must_use]
    pub fn lhs(&self) -> ExactValue {
        self.lhs
    }

    #[must_use]
    pub fn operator(&self) -> Operator {
        self.operator
    }

    #[must_use]
    pub fn rhs(&self) -> ExactValue {
        self.rhs
    }

    #[must_use]
    pub fn result(&self) -> ExactValue {
        self.result
    }

    #[must_use]
    pub fn unknown(&self) -> Slot {
        self.unknown
    }

    /// The value hidden behind the unknown slot — the answer.
    #[must_use]
    pub fn answer(&self) -> ExactValue {
        match self.unknown {
            Slot::Lhs => self.lhs,
            Slot::Rhs => self.rhs,
            Slot::Result => self.result,
        }
    }
}

/// A fraction as written — possibly not in lowest terms. [`ExactValue`] cannot
/// carry this: it normalizes on construction, so `125/500` and `1/4` are the
/// same value; this keeps the written numerator and denominator for prompts
/// about the written form itself ([`Prompt::Simplify`]). True by construction:
/// the denominator is nonzero, so [`value`](UnreducedFraction::value) cannot
/// fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnreducedFraction {
    numerator: i64,
    denominator: i64,
}

impl UnreducedFraction {
    /// A fraction with an explicit written form. Errors exactly where
    /// [`ExactValue::rational`] does (a zero denominator).
    pub fn new(numerator: i64, denominator: i64) -> Result<Self, ValueError> {
        // Validate exactly what `value` will compute, then keep the raw parts.
        ExactValue::rational(numerator, denominator)?;
        Ok(Self {
            numerator,
            denominator,
        })
    }

    /// The numerator as written.
    #[must_use]
    pub const fn numerator(self) -> i64 {
        self.numerator
    }

    /// The denominator as written.
    #[must_use]
    pub const fn denominator(self) -> i64 {
        self.denominator
    }

    /// The reduced value — the canonical answer to "simplify this".
    #[must_use]
    pub fn value(self) -> ExactValue {
        ExactValue::rational(self.numerator, self.denominator)
            .expect("denominator validated nonzero at construction")
    }
}

/// The kind of prompt a [`Problem`] poses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    /// An equation with one unknown slot. More prompt kinds (comparison,
    /// conversion, …) arrive with their generators.
    Equation(Equation),
    /// Reduce the shown fraction to lowest terms: `125/500 = ?`. The canonical
    /// answer is the shown fraction's reduced value.
    Simplify(UnreducedFraction),
}

/// How a [`Problem`]'s answer is supplied and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerContract {
    /// A typed answer, checked by exact value equality. `required_representation`,
    /// when set, additionally demands the answer be written in that form (e.g.
    /// "answer as a percent").
    FreeForm {
        required_representation: Option<Representation>,
        /// For fraction-form answers: the written numerator and denominator
        /// must be the canonical (lowest-terms) pair, not merely an equal value
        /// — `2/8` is rejected for a canonical `1/4`. What "simplify" means;
        /// plain arithmetic leaves it off.
        require_reduced: bool,
    },
    /// A pick from a fixed set of `options` in display order, exactly one of
    /// which equals the prompt's canonical answer (the rest are plausible
    /// distractors). Built by [`super::into_multiple_choice`]; correctness is derived
    /// from the prompt at evaluation time, never trusted from a stored flag.
    MultipleChoice { options: Vec<ExactValue> },
}

/// A generated problem: a prompt, the skills it exercises, its band, and the
/// answer contract.
///
/// The canonical exact solution is **derived from the prompt**
/// ([`canonical_solution`](Problem::canonical_solution)), not stored alongside
/// it, so a problem can never advertise an answer that disagrees with its own
/// prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    prompt: Prompt,
    skills: Vec<SkillId>,
    band: BandId,
    answer_contract: AnswerContract,
}

impl Problem {
    #[must_use]
    pub fn new(
        prompt: Prompt,
        skills: Vec<SkillId>,
        band: BandId,
        answer_contract: AnswerContract,
    ) -> Self {
        Self {
            prompt,
            skills,
            band,
            answer_contract,
        }
    }

    #[must_use]
    pub fn prompt(&self) -> &Prompt {
        &self.prompt
    }

    #[must_use]
    pub fn skills(&self) -> &[SkillId] {
        &self.skills
    }

    #[must_use]
    pub fn band(&self) -> &BandId {
        &self.band
    }

    #[must_use]
    pub fn answer_contract(&self) -> &AnswerContract {
        &self.answer_contract
    }

    /// The canonical exact answer, derived from the prompt (for an equation, the
    /// value hidden behind its unknown slot).
    #[must_use]
    pub fn canonical_solution(&self) -> ExactValue {
        match &self.prompt {
            Prompt::Equation(equation) => equation.answer(),
            Prompt::Simplify(fraction) => fraction.value(),
        }
    }

    /// Replace the answer contract, keeping the prompt, skills, and band. Used by
    /// [`super::into_multiple_choice`] to turn a free-form problem into a
    /// multiple-choice one.
    #[must_use]
    pub fn with_contract(mut self, answer_contract: AnswerContract) -> Self {
        self.answer_contract = answer_contract;
        self
    }
}

#[cfg(test)]
mod tests;
