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
use crate::math_core::{ExactValue, Operator, Representation, ValueError};
use crate::rng::Rng;

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
        }
    }
}

/// Produces [`Problem`]s, drawing randomness from a seeded [`Rng`].
pub trait Generator {
    fn generate(&self, rng: &mut Rng) -> Problem;
}

/// Why a generator could not be constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratorError {
    /// The operand range is empty (`start > end`).
    EmptyRange,
    /// The operand range admits values whose arithmetic overflows `i64`.
    RangeOverflows,
}

/// The `expect` message for arithmetic that construction-time validation
/// ([`validate_operands`]) has already proven cannot overflow.
const INVARIANT: &str = "operand range validated at construction; arithmetic cannot overflow";

/// Check that every equation [`build_equation`] can draw for `operator` over
/// `operands` fits in `i64`, so generation stays panic-free. Bounds are the
/// worst case over the range endpoints.
fn validate_operands(
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

/// Build a whole-number equation for `operator` over `operands`, with `unknown`
/// as the hidden slot. Differences are kept non-negative (negatives are deferred
/// until whole-number mastery) and divisions are exact by construction.
///
/// Panics only on a bug: generator constructors validate `operands` with
/// [`validate_operands`], so the arithmetic here cannot overflow.
fn build_equation(
    operator: Operator,
    operands: &RangeInclusive<i64>,
    unknown: Slot,
    rng: &mut Rng,
) -> Equation {
    let (lhs, rhs) = match operator {
        Operator::Add | Operator::Multiply => (
            rng.int_range(operands.clone()),
            rng.int_range(operands.clone()),
        ),
        Operator::Subtract => {
            let mut a = rng.int_range(operands.clone());
            let mut b = rng.int_range(operands.clone());
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

fn arithmetic_problem(equation: Equation, skills: &[SkillId], band: &BandId) -> Problem {
    Problem::new(
        Prompt::Equation(equation),
        skills.to_vec(),
        band.clone(),
        AnswerContract::FreeForm {
            required_representation: None,
        },
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
    /// A direct-arithmetic generator (`a op b = ?`) over `operands`.
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
        })
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
    /// A missing-term generator (`? op b = r` or `a op ? = r`) over `operands`.
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
        })
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
        let generator =
            DirectArithmetic::new("sums-to-20", "addition", Operator::Add, 0..=20).unwrap();
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
        let generator = DirectArithmetic::new("s", "b", Operator::Add, 0..=99).unwrap();
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
        let generator =
            DirectArithmetic::new("diff", "subtraction", Operator::Subtract, 0..=20).unwrap();
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
            DirectArithmetic::new("facts", "multiplication", Operator::Multiply, 0..=12).unwrap();
        let mut rng = Rng::new(3);
        for _ in 0..200 {
            let e = equation_of(&generator.generate(&mut rng));
            assert_eq!(e.lhs().try_mul(e.rhs()).unwrap(), e.result());
        }
    }

    #[test]
    fn division_is_exact_by_construction() {
        let generator = DirectArithmetic::new("div", "division", Operator::Divide, 1..=12).unwrap();
        let mut rng = Rng::new(5);
        for _ in 0..200 {
            let e = equation_of(&generator.generate(&mut rng));
            assert!(e.rhs().as_integer().unwrap() >= 1); // non-zero divisor
            assert_eq!(e.lhs().try_div(e.rhs()).unwrap(), e.result());
        }
    }

    #[test]
    fn missing_term_hides_an_operand_not_the_result() {
        let generator =
            MissingTerm::new("missing-addend", "addition", Operator::Add, 0..=20).unwrap();
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
        )
        .unwrap();
        assert_eq!(e.answer(), ExactValue::integer(7));
    }

    #[test]
    fn equation_new_rejects_a_false_statement() {
        // 2 + 2 = 5 is not a representable equation.
        let err = Equation::new(
            ExactValue::integer(2),
            Operator::Add,
            ExactValue::integer(2),
            ExactValue::integer(5),
            Slot::Result,
        )
        .unwrap_err();
        assert_eq!(err, EquationError::Inconsistent);
    }

    #[test]
    fn equation_new_reports_arithmetic_failure() {
        // Dividing by zero cannot yield any stated result.
        let err = Equation::new(
            ExactValue::integer(1),
            Operator::Divide,
            ExactValue::ZERO,
            ExactValue::ZERO,
            Slot::Result,
        )
        .unwrap_err();
        assert_eq!(err, EquationError::Arithmetic(ValueError::DivideByZero));
    }

    #[test]
    fn equation_solve_computes_a_true_result() {
        let e = Equation::solve(
            ExactValue::integer(2),
            Operator::Add,
            ExactValue::integer(3),
            Slot::Result,
        )
        .unwrap();
        assert_eq!(e.result(), ExactValue::integer(5));
        assert_eq!(e.answer(), ExactValue::integer(5));
    }

    #[test]
    fn problem_canonical_solution_tracks_the_prompt() {
        // The canonical answer is derived from the prompt, so it always equals
        // the equation's hidden slot — there is no way to assert a divergent one.
        let equation = Equation::solve(
            ExactValue::integer(6),
            Operator::Multiply,
            ExactValue::integer(7),
            Slot::Result,
        )
        .unwrap();
        let problem = arithmetic_problem(equation, &[SkillId::from("s")], &BandId::from("b"));
        assert_eq!(problem.canonical_solution(), equation.answer());
        assert_eq!(problem.canonical_solution(), ExactValue::integer(42));
    }

    #[test]
    fn generator_rejects_an_empty_range() {
        // Build the reversed range from values so its emptiness is caught at
        // runtime by validation, not by the compiler's lint.
        let (start, end) = (5_i64, 3_i64);
        let err = DirectArithmetic::new("s", "b", Operator::Add, start..=end).unwrap_err();
        assert_eq!(err, GeneratorError::EmptyRange);
        let err = MissingTerm::new("s", "b", Operator::Add, start..=end).unwrap_err();
        assert_eq!(err, GeneratorError::EmptyRange);
    }

    #[test]
    fn generator_rejects_a_range_that_would_overflow() {
        // A product near i64::MAX must be refused, not panic at generation time.
        let err = DirectArithmetic::new("s", "b", Operator::Multiply, 0..=i64::MAX).unwrap_err();
        assert_eq!(err, GeneratorError::RangeOverflows);
        // Addition overflow is caught too.
        let err = DirectArithmetic::new("s", "b", Operator::Add, 0..=i64::MAX).unwrap_err();
        assert_eq!(err, GeneratorError::RangeOverflows);
    }

    #[test]
    fn generator_accepts_a_safe_range() {
        assert!(DirectArithmetic::new("s", "b", Operator::Multiply, 0..=1_000).is_ok());
    }
}
