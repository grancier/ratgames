//! Problem generation: named generators that produce [`Problem`]s with
//! constrained, reproducible parameters.
//!
//! A [`Problem`] pairs a [`Prompt`] with the skills it exercises, its band, an
//! [`AnswerContract`], and the canonical exact solution. Generators draw from a
//! seeded [`Rng`], so a drill replays identically.
//!
//! This module owns the problem *model* and the arithmetic generators
//! ([`DirectArithmetic`], [`MissingTerm`]). Answer parsing, distractor
//! generation, and diagnostics live in the later `answer_evaluation` module:
//! [`AnswerContract`] here is the model; evaluating against it is behaviour there.

use std::ops::RangeInclusive;

use crate::curriculum::{BandId, SkillId};
use crate::math_core::{ExactValue, Operator, Representation};
use crate::rng::Rng;

/// Which slot of an [`Equation`] is the unknown (the answer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Lhs,
    Rhs,
    Result,
}

/// An equation with exactly one unknown slot: `347 + 286 = ?` (unknown
/// [`Result`](Slot::Result)) or `? + 8 = 15` (unknown [`Lhs`](Slot::Lhs)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Equation {
    lhs: ExactValue,
    operator: Operator,
    rhs: ExactValue,
    result: ExactValue,
    unknown: Slot,
}

impl Equation {
    #[must_use]
    pub fn new(
        lhs: ExactValue,
        operator: Operator,
        rhs: ExactValue,
        result: ExactValue,
        unknown: Slot,
    ) -> Self {
        Self {
            lhs,
            operator,
            rhs,
            result,
            unknown,
        }
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

/// The kind of prompt a [`Problem`] poses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    /// An equation with one unknown slot. More prompt kinds (comparison,
    /// conversion, …) arrive with their generators.
    Equation(Equation),
}

/// How a [`Problem`]'s answer is supplied and checked.
///
/// Only free-form entry exists so far; multiple choice arrives with
/// `answer_evaluation`, which owns distractor generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerContract {
    /// A typed answer, checked by exact value equality. `required_representation`,
    /// when set, additionally demands the answer be written in that form (e.g.
    /// "answer as a percent").
    FreeForm {
        required_representation: Option<Representation>,
    },
}

/// A generated problem: a prompt, the skills it exercises, its band, the answer
/// contract, and the canonical exact solution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    prompt: Prompt,
    skills: Vec<SkillId>,
    band: BandId,
    answer_contract: AnswerContract,
    canonical_solution: ExactValue,
}

impl Problem {
    #[must_use]
    pub fn new(
        prompt: Prompt,
        skills: Vec<SkillId>,
        band: BandId,
        answer_contract: AnswerContract,
        canonical_solution: ExactValue,
    ) -> Self {
        Self {
            prompt,
            skills,
            band,
            answer_contract,
            canonical_solution,
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

    #[must_use]
    pub fn canonical_solution(&self) -> ExactValue {
        self.canonical_solution
    }
}

/// Produces [`Problem`]s, drawing randomness from a seeded [`Rng`].
pub trait Generator {
    fn generate(&self, rng: &mut Rng) -> Problem;
}

const OVERFLOW: &str = "generator operand range overflows i64";

/// Build a whole-number equation for `operator` over `operands`, with `unknown`
/// as the hidden slot. Differences are kept non-negative (negatives are deferred
/// until whole-number mastery) and divisions are exact by construction.
fn build_equation(
    operator: Operator,
    operands: &RangeInclusive<i64>,
    unknown: Slot,
    rng: &mut Rng,
) -> Equation {
    let (lhs, rhs, result) = match operator {
        Operator::Add => {
            let a = rng.int_range(operands.clone());
            let b = rng.int_range(operands.clone());
            (a, b, a.checked_add(b).expect(OVERFLOW))
        }
        Operator::Subtract => {
            let mut a = rng.int_range(operands.clone());
            let mut b = rng.int_range(operands.clone());
            if a < b {
                std::mem::swap(&mut a, &mut b);
            }
            (a, b, a - b)
        }
        Operator::Multiply => {
            let a = rng.int_range(operands.clone());
            let b = rng.int_range(operands.clone());
            (a, b, a.checked_mul(b).expect(OVERFLOW))
        }
        Operator::Divide => {
            // Exact by construction: dividend = divisor * quotient.
            let divisor = rng.int_range(1..=(*operands.end()).max(1));
            let quotient = rng.int_range(operands.clone());
            let dividend = divisor.checked_mul(quotient).expect(OVERFLOW);
            (dividend, divisor, quotient)
        }
    };
    Equation::new(
        ExactValue::integer(lhs),
        operator,
        ExactValue::integer(rhs),
        ExactValue::integer(result),
        unknown,
    )
}

fn arithmetic_problem(equation: Equation, skills: &[SkillId], band: &BandId) -> Problem {
    Problem::new(
        Prompt::Equation(equation),
        skills.to_vec(),
        band.clone(),
        AnswerContract::FreeForm {
            required_representation: None,
        },
        equation.answer(),
    )
}

/// A direct-arithmetic generator: `a op b = ?` (the result is unknown).
#[derive(Debug, Clone)]
pub struct DirectArithmetic {
    skills: Vec<SkillId>,
    band: BandId,
    operator: Operator,
    operands: RangeInclusive<i64>,
}

impl DirectArithmetic {
    #[must_use]
    pub fn new(
        skill: impl Into<SkillId>,
        band: impl Into<BandId>,
        operator: Operator,
        operands: RangeInclusive<i64>,
    ) -> Self {
        Self {
            skills: vec![skill.into()],
            band: band.into(),
            operator,
            operands,
        }
    }
}

impl Generator for DirectArithmetic {
    fn generate(&self, rng: &mut Rng) -> Problem {
        let equation = build_equation(self.operator, &self.operands, Slot::Result, rng);
        arithmetic_problem(equation, &self.skills, &self.band)
    }
}

/// A missing-term generator: `? op b = r` or `a op ? = r` (an operand is unknown).
#[derive(Debug, Clone)]
pub struct MissingTerm {
    skills: Vec<SkillId>,
    band: BandId,
    operator: Operator,
    operands: RangeInclusive<i64>,
}

impl MissingTerm {
    #[must_use]
    pub fn new(
        skill: impl Into<SkillId>,
        band: impl Into<BandId>,
        operator: Operator,
        operands: RangeInclusive<i64>,
    ) -> Self {
        Self {
            skills: vec![skill.into()],
            band: band.into(),
            operator,
            operands,
        }
    }
}

impl Generator for MissingTerm {
    fn generate(&self, rng: &mut Rng) -> Problem {
        let unknown = if rng.coin() { Slot::Lhs } else { Slot::Rhs };
        let equation = build_equation(self.operator, &self.operands, unknown, rng);
        arithmetic_problem(equation, &self.skills, &self.band)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn equation_of(problem: &Problem) -> Equation {
        let Prompt::Equation(equation) = problem.prompt();
        *equation
    }

    #[test]
    fn direct_addition_is_consistent_and_in_range() {
        let generator = DirectArithmetic::new("sums-to-20", "addition", Operator::Add, 0..=20);
        let mut rng = Rng::new(42);
        for _ in 0..200 {
            let problem = generator.generate(&mut rng);
            let e = equation_of(&problem);
            assert_eq!(e.unknown(), Slot::Result);
            assert_eq!(e.operator(), Operator::Add);
            let a = e.lhs().as_integer().unwrap();
            let b = e.rhs().as_integer().unwrap();
            assert!((0..=20).contains(&a) && (0..=20).contains(&b));
            assert_eq!(e.lhs().try_add(e.rhs()).unwrap(), e.result());
            assert_eq!(problem.canonical_solution(), e.result());
            assert_eq!(problem.skills(), &[SkillId::from("sums-to-20")]);
            assert_eq!(problem.band(), &BandId::from("addition"));
            assert_eq!(
                problem.answer_contract(),
                &AnswerContract::FreeForm {
                    required_representation: None
                }
            );
        }
    }

    #[test]
    fn generation_is_deterministic_for_a_seed() {
        let generator = DirectArithmetic::new("s", "b", Operator::Add, 0..=99);
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..50 {
            assert_eq!(generator.generate(&mut a), generator.generate(&mut b));
        }
        // A different seed diverges at least once over many draws.
        let mut c = Rng::new(8);
        let mut d = Rng::new(7);
        let differs = (0..50).any(|_| generator.generate(&mut c) != generator.generate(&mut d));
        assert!(differs);
    }

    #[test]
    fn subtraction_never_goes_negative() {
        let generator = DirectArithmetic::new("diff", "subtraction", Operator::Subtract, 0..=20);
        let mut rng = Rng::new(1);
        for _ in 0..200 {
            let e = equation_of(&generator.generate(&mut rng));
            assert!(e.lhs() >= e.rhs());
            assert!(!e.result().is_negative());
            assert_eq!(e.lhs().try_sub(e.rhs()).unwrap(), e.result());
        }
    }

    #[test]
    fn multiplication_is_the_product() {
        let generator =
            DirectArithmetic::new("facts", "multiplication", Operator::Multiply, 0..=12);
        let mut rng = Rng::new(3);
        for _ in 0..200 {
            let e = equation_of(&generator.generate(&mut rng));
            assert_eq!(e.lhs().try_mul(e.rhs()).unwrap(), e.result());
        }
    }

    #[test]
    fn division_is_exact_by_construction() {
        let generator = DirectArithmetic::new("div", "division", Operator::Divide, 1..=12);
        let mut rng = Rng::new(5);
        for _ in 0..200 {
            let e = equation_of(&generator.generate(&mut rng));
            assert!(e.rhs().as_integer().unwrap() >= 1); // non-zero divisor
            assert_eq!(e.lhs().try_div(e.rhs()).unwrap(), e.result());
        }
    }

    #[test]
    fn missing_term_hides_an_operand_not_the_result() {
        let generator = MissingTerm::new("missing-addend", "addition", Operator::Add, 0..=20);
        let mut rng = Rng::new(9);
        let mut saw_lhs = false;
        let mut saw_rhs = false;
        for _ in 0..200 {
            let problem = generator.generate(&mut rng);
            let e = equation_of(&problem);
            assert_ne!(e.unknown(), Slot::Result);
            match e.unknown() {
                Slot::Lhs => saw_lhs = true,
                Slot::Rhs => saw_rhs = true,
                Slot::Result => unreachable!(),
            }
            assert_eq!(e.lhs().try_add(e.rhs()).unwrap(), e.result());
            assert_eq!(problem.canonical_solution(), e.answer());
        }
        assert!(saw_lhs && saw_rhs, "both operand positions should occur");
    }

    #[test]
    fn equation_answer_selects_the_unknown_slot() {
        let e = Equation::new(
            ExactValue::integer(7),
            Operator::Add,
            ExactValue::integer(8),
            ExactValue::integer(15),
            Slot::Lhs,
        );
        assert_eq!(e.answer(), ExactValue::integer(7));
    }
}
