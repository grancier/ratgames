//! Problem generation: named generators that produce [`Problem`]s with
//! constrained, reproducible parameters.
//!
//! A [`Problem`] pairs a [`Prompt`] with the skills it exercises, its band, an
//! [`AnswerContract`], and the canonical exact solution. Generators draw from a
//! seeded [`Rng`], so a drill replays identically.
//!
//! This module owns the problem *model*, the whole-number arithmetic
//! generators ([`DirectArithmetic`], [`MissingTerm`]), the fraction generators
//! ([`SimplifyFraction`], [`FractionArithmetic`]), their weighted composition
//! ([`Mix`]), and multiple-choice distractor generation
//! ([`into_multiple_choice`]) — the options are shown before the learner
//! answers, so they are problem-time state. Answer parsing and diagnostics live
//! in the later `answer_evaluation` module: [`AnswerContract`] here is the
//! model; evaluating against it is behaviour there.

use std::fmt;
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
    /// distractors). Built by [`into_multiple_choice`]; correctness is derived
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
    /// [`into_multiple_choice`] to turn a free-form problem into a
    /// multiple-choice one.
    #[must_use]
    pub fn with_contract(mut self, answer_contract: AnswerContract) -> Self {
        self.answer_contract = answer_contract;
        self
    }
}

/// Why a [`Problem`] could not be turned into multiple choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultipleChoiceError {
    /// Fewer than two options were requested; multiple choice needs the correct
    /// answer plus at least one distractor.
    TooFewOptions,
}

/// Turn `problem` into multiple choice with `options` total choices — the
/// correct answer plus `options - 1` plausible distractors — drawing from the
/// seeded `rng` so the choice order is reproducible.
///
/// Distractors are believable wrong numbers (near-misses, the other operations
/// on the operands, and numbers visible in the equation), not labelled
/// misconceptions. Errors with [`MultipleChoiceError::TooFewOptions`] when
/// `options < 2`.
pub fn into_multiple_choice(
    problem: Problem,
    rng: &mut Rng,
    options: usize,
) -> Result<Problem, MultipleChoiceError> {
    if options < 2 {
        return Err(MultipleChoiceError::TooFewOptions);
    }
    let canonical = problem.canonical_solution();
    let mut choices = match *problem.prompt() {
        Prompt::Equation(equation) => equation_distractors(equation, canonical, options - 1, rng),
        Prompt::Simplify(_) => simplify_distractors(canonical, options - 1, rng),
    };
    choices.push(canonical);
    shuffle(&mut choices, rng);
    Ok(problem.with_contract(AnswerContract::MultipleChoice { options: choices }))
}

/// Up to `count` plausible, distinct, non-negative distractors for an
/// equation's `canonical` answer.
fn equation_distractors(
    equation: Equation,
    canonical: ExactValue,
    count: usize,
    rng: &mut Rng,
) -> Vec<ExactValue> {
    let mut pool: Vec<ExactValue> = Vec::new();

    // Near-misses: the answer nudged by a small amount (off-by-one, place value).
    for offset in [1, -1, 2, -2, 10, -10] {
        consider(&mut pool, canonical, offset_by(canonical, offset));
    }
    // Numbers visible in the equation — a common slip is echoing an operand.
    for shown in [equation.lhs(), equation.rhs(), equation.result()] {
        consider(&mut pool, canonical, Some(shown));
    }
    // The other operations on the same operands — "added instead of multiplied".
    for op in [
        Operator::Add,
        Operator::Subtract,
        Operator::Multiply,
        Operator::Divide,
    ] {
        if op != equation.operator() {
            consider(
                &mut pool,
                canonical,
                op.apply(equation.lhs(), equation.rhs()).ok(),
            );
        }
    }

    finish_pool(pool, canonical, count, rng)
}

/// Up to `count` distractors for "simplify to `canonical`": the shapes a
/// mis-reduction actually takes — a numerator or denominator off by one, the
/// flipped fraction, and one part reduced by a factor the other kept. (Every
/// *partially* reduced form of the shown fraction equals the canonical value,
/// so it can never be a distractor — options differ by value.)
fn simplify_distractors(canonical: ExactValue, count: usize, rng: &mut Rng) -> Vec<ExactValue> {
    let (p, q) = (canonical.numerator(), canonical.denominator());
    let mut pool: Vec<ExactValue> = Vec::new();
    let candidates = [
        ExactValue::rational(p + 1, q).ok(),
        ExactValue::rational(p - 1, q).ok(),
        ExactValue::rational(p, q + 1).ok(),
        ExactValue::rational(p, q - 1).ok(),
        (p != 0).then(|| ExactValue::rational(q, p).ok()).flatten(),
        p.checked_mul(2)
            .and_then(|doubled| ExactValue::rational(doubled, q).ok()),
        q.checked_mul(2)
            .and_then(|doubled| ExactValue::rational(p, doubled).ok()),
    ];
    for candidate in candidates {
        consider(&mut pool, canonical, candidate);
    }
    finish_pool(pool, canonical, count, rng)
}

/// Shuffle and cap a distractor pool at `count`, then guarantee enough by
/// widening an integer offset up and down. Downward stays non-negative for a
/// large canonical, upward avoids overflow for a small one, so one direction
/// always yields a fresh value and the loop terminates.
fn finish_pool(
    mut pool: Vec<ExactValue>,
    canonical: ExactValue,
    count: usize,
    rng: &mut Rng,
) -> Vec<ExactValue> {
    shuffle(&mut pool, rng);
    pool.truncate(count);

    let mut step = 3;
    while pool.len() < count {
        consider(&mut pool, canonical, offset_by(canonical, step));
        if pool.len() < count {
            consider(&mut pool, canonical, offset_by(canonical, -step));
        }
        step += 1;
    }
    pool
}

/// `canonical + offset`, or `None` on `i64` overflow.
fn offset_by(canonical: ExactValue, offset: i64) -> Option<ExactValue> {
    canonical.try_add(ExactValue::integer(offset)).ok()
}

/// Push `candidate` into `pool` when it is a usable distractor: present, not the
/// canonical answer, non-negative, and not already there.
fn consider(pool: &mut Vec<ExactValue>, canonical: ExactValue, candidate: Option<ExactValue>) {
    if let Some(value) = candidate
        && value != canonical
        && !value.is_negative()
        && !pool.contains(&value)
    {
        pool.push(value);
    }
}

/// Fisher–Yates shuffle driven by the seeded `rng`.
fn shuffle(items: &mut [ExactValue], rng: &mut Rng) {
    for i in (1..items.len()).rev() {
        let j = rng.int_range(0..=i as i64) as usize;
        items.swap(i, j);
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
    /// The operand range — or a mix's summed weights — admits values whose
    /// arithmetic overflows `i64`.
    RangeOverflows,
    /// A maximum operand distance was set for division, where it has no
    /// meaning: the shown numbers are dividend and divisor, and the dividend is
    /// the divisor times the quotient — not an independent draw that can be
    /// held near its partner.
    DistanceUnsupported,
    /// A mix was built with no generators to draw from.
    EmptyMix,
    /// A mix entry's weight was zero — it could never be drawn; remove the
    /// entry instead of weighting it out.
    ZeroMixWeight,
    /// The generator does not pose this operator (fraction arithmetic covers
    /// addition and multiplication; differences could go negative and
    /// quotients are a different drill).
    OperatorUnsupported,
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
fn build_equation(
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

fn arithmetic_problem(equation: Equation, skills: &[SkillId], band: &BandId) -> Problem {
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

/// A direct-arithmetic generator: `a op b = ?` (the result is unknown).
#[derive(Debug, Clone)]
pub struct DirectArithmetic {
    skills: Vec<SkillId>,
    band: BandId,
    operator: Operator,
    operands: RangeInclusive<i64>,
    max_distance: Option<u64>,
}

impl DirectArithmetic {
    /// A direct-arithmetic generator (`a op b = ?`) over `operands`, with no
    /// constraint between the operands beyond the range (see
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
    /// (`|lhs − rhs| ≤ max_distance`) — the "numbers at most 8 apart" knob of a
    /// graduated difficulty ladder. `0` forces equal operands (doubles); a
    /// distance wider than the range changes nothing. For subtraction the
    /// distance also bounds the result, since the difference *is* the distance.
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

impl Generator for DirectArithmetic {
    fn generate(&self, rng: &mut Rng) -> Problem {
        let equation = build_equation(
            self.operator,
            &self.operands,
            self.max_distance,
            Slot::Result,
            rng,
        );
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

/// The `expect` message for fraction arithmetic that construction-time
/// validation has already proven safe: parts cannot overflow and denominators
/// are nonzero.
const FRACTION_INVARIANT: &str =
    "fraction ranges validated at construction; parts cannot overflow and denominators are nonzero";

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

/// A weighted mix of generators: every [`generate`](Generator::generate) call
/// picks one entry by weight from the seeded [`Rng`], then delegates to it —
/// how a single level drills several operators at once (e.g. addition and
/// subtraction at 40 each with multiplication at 20, an 80/20 split).
///
/// Weights are relative shares, not percentages. One draw decides the entry and
/// the chosen generator then draws as usual, so a mixed drill replays
/// identically for a seed. Composes with anything implementing [`Generator`],
/// including another `Mix`.
pub struct Mix {
    entries: Vec<(u32, Box<dyn Generator>)>,
    /// Cached sum of the weights; validated non-zero and within `i64`.
    total_weight: i64,
}

impl fmt::Debug for Mix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let weights: Vec<u32> = self.entries.iter().map(|(weight, _)| *weight).collect();
        f.debug_struct("Mix")
            .field("weights", &weights)
            .finish_non_exhaustive()
    }
}

impl Mix {
    /// A weighted mix over `entries` (`(weight, generator)` pairs).
    ///
    /// Errors with [`GeneratorError::EmptyMix`] when `entries` is empty,
    /// [`GeneratorError::ZeroMixWeight`] when any entry's weight is zero (it
    /// could never be drawn — remove the entry instead), and
    /// [`GeneratorError::RangeOverflows`] when the summed weights exceed `i64`
    /// (unreachable with realistic mixes).
    pub fn new(entries: Vec<(u32, Box<dyn Generator>)>) -> Result<Self, GeneratorError> {
        if entries.is_empty() {
            return Err(GeneratorError::EmptyMix);
        }
        if entries.iter().any(|(weight, _)| *weight == 0) {
            return Err(GeneratorError::ZeroMixWeight);
        }
        let total_weight = entries
            .iter()
            .try_fold(0_i64, |sum, (weight, _)| {
                sum.checked_add(i64::from(*weight))
            })
            .ok_or(GeneratorError::RangeOverflows)?;
        Ok(Self {
            entries,
            total_weight,
        })
    }
}

/// The `expect` message for the mix draw, whose construction-time validation
/// ([`Mix::new`]) has already proven the entries non-empty.
const MIX_INVARIANT: &str = "mix validated non-empty at construction";

impl Generator for Mix {
    fn generate(&self, rng: &mut Rng) -> Problem {
        // A ticket below the total weight lands in exactly one entry's band;
        // walking the bands keeps the pick a single rng draw.
        let mut ticket = rng.int_range(0..=self.total_weight - 1);
        let (last, rest) = self.entries.split_last().expect(MIX_INVARIANT);
        for (weight, generator) in rest {
            let weight = i64::from(*weight);
            if ticket < weight {
                return generator.generate(rng);
            }
            ticket -= weight;
        }
        last.1.generate(rng)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn equation_of(problem: &Problem) -> Equation {
        let Prompt::Equation(equation) = problem.prompt() else {
            panic!("expected an equation prompt");
        };
        *equation
    }

    fn boxed(generator: impl Generator + 'static) -> Box<dyn Generator> {
        Box::new(generator)
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
                    required_representation: None,
                    require_reduced: false,
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

    #[test]
    fn max_distance_bounds_addition_operands() {
        let generator = DirectArithmetic::new("sums", "addition", Operator::Add, 0..=9)
            .unwrap()
            .with_max_distance(3)
            .unwrap();
        let mut rng = Rng::new(11);
        for _ in 0..200 {
            let e = equation_of(&generator.generate(&mut rng));
            let a = e.lhs().as_integer().unwrap();
            let b = e.rhs().as_integer().unwrap();
            assert!((0..=9).contains(&a) && (0..=9).contains(&b));
            assert!((a - b).abs() <= 3, "operands {a} and {b} drift past 3");
            assert_eq!(e.lhs().try_add(e.rhs()).unwrap(), e.result());
        }
    }

    #[test]
    fn max_distance_bounds_subtraction_and_stays_non_negative() {
        let generator = DirectArithmetic::new("diff", "subtraction", Operator::Subtract, 0..=20)
            .unwrap()
            .with_max_distance(8)
            .unwrap();
        let mut rng = Rng::new(2);
        for _ in 0..200 {
            let e = equation_of(&generator.generate(&mut rng));
            let a = e.lhs().as_integer().unwrap();
            let b = e.rhs().as_integer().unwrap();
            assert!((0..=20).contains(&a) && (0..=20).contains(&b));
            assert!(a >= b, "difference must stay non-negative");
            assert!(a - b <= 8, "operands {a} and {b} drift past 8");
            assert_eq!(e.lhs().try_sub(e.rhs()).unwrap(), e.result());
        }
    }

    #[test]
    fn max_distance_bounds_multiplication_operands() {
        let generator =
            DirectArithmetic::new("facts", "multiplication", Operator::Multiply, 0..=12)
                .unwrap()
                .with_max_distance(8)
                .unwrap();
        let mut rng = Rng::new(4);
        for _ in 0..200 {
            let e = equation_of(&generator.generate(&mut rng));
            let a = e.lhs().as_integer().unwrap();
            let b = e.rhs().as_integer().unwrap();
            assert!((a - b).abs() <= 8, "operands {a} and {b} drift past 8");
            assert_eq!(e.lhs().try_mul(e.rhs()).unwrap(), e.result());
        }
    }

    #[test]
    fn max_distance_zero_forces_equal_operands() {
        let generator = DirectArithmetic::new("doubles", "addition", Operator::Add, 0..=9)
            .unwrap()
            .with_max_distance(0)
            .unwrap();
        let mut rng = Rng::new(6);
        for _ in 0..100 {
            let e = equation_of(&generator.generate(&mut rng));
            assert_eq!(e.lhs(), e.rhs());
        }
    }

    #[test]
    fn max_distance_wider_than_the_range_is_unconstrained() {
        // u64::MAX also exercises the saturating window arithmetic.
        let generator = DirectArithmetic::new("sums", "addition", Operator::Add, 0..=9)
            .unwrap()
            .with_max_distance(u64::MAX)
            .unwrap();
        let mut rng = Rng::new(8);
        let mut widest = 0;
        for _ in 0..200 {
            let e = equation_of(&generator.generate(&mut rng));
            let a = e.lhs().as_integer().unwrap();
            let b = e.rhs().as_integer().unwrap();
            assert!((0..=9).contains(&a) && (0..=9).contains(&b));
            widest = widest.max((a - b).abs());
        }
        assert_eq!(widest, 9, "the full operand spread should still occur");
    }

    #[test]
    fn max_distance_rejects_division() {
        let err = DirectArithmetic::new("div", "division", Operator::Divide, 1..=12)
            .unwrap()
            .with_max_distance(5)
            .unwrap_err();
        assert_eq!(err, GeneratorError::DistanceUnsupported);
        let err = MissingTerm::new("div", "division", Operator::Divide, 1..=12)
            .unwrap()
            .with_max_distance(5)
            .unwrap_err();
        assert_eq!(err, GeneratorError::DistanceUnsupported);
    }

    #[test]
    fn missing_term_honours_max_distance() {
        let generator = MissingTerm::new("missing-addend", "addition", Operator::Add, 0..=20)
            .unwrap()
            .with_max_distance(4)
            .unwrap();
        let mut rng = Rng::new(13);
        for _ in 0..200 {
            let problem = generator.generate(&mut rng);
            let e = equation_of(&problem);
            let a = e.lhs().as_integer().unwrap();
            let b = e.rhs().as_integer().unwrap();
            assert!((a - b).abs() <= 4, "operands {a} and {b} drift past 4");
            assert_ne!(e.unknown(), Slot::Result);
        }
    }

    #[test]
    fn max_distance_generation_is_deterministic_for_a_seed() {
        let build = || {
            DirectArithmetic::new("s", "b", Operator::Add, 0..=99)
                .unwrap()
                .with_max_distance(10)
                .unwrap()
        };
        let (first, second) = (build(), build());
        let mut a = Rng::new(21);
        let mut b = Rng::new(21);
        for _ in 0..50 {
            assert_eq!(first.generate(&mut a), second.generate(&mut b));
        }
    }

    fn gcd(a: i64, b: i64) -> i64 {
        if b == 0 { a } else { gcd(b, a % b) }
    }

    #[test]
    fn unreduced_fraction_keeps_its_written_form_and_reduces_its_value() {
        let shown = UnreducedFraction::new(125, 500).unwrap();
        assert_eq!((shown.numerator(), shown.denominator()), (125, 500));
        assert_eq!(shown.value(), ExactValue::rational(1, 4).unwrap());
        assert_eq!(
            UnreducedFraction::new(1, 0).unwrap_err(),
            ValueError::DivideByZero
        );
    }

    #[test]
    fn simplify_generator_poses_reducible_proper_fractions() {
        let generator = SimplifyFraction::new("lowest-terms", "fractions", 1..=9, 2..=125).unwrap();
        let mut rng = Rng::new(31);
        for _ in 0..200 {
            let problem = generator.generate(&mut rng);
            let Prompt::Simplify(shown) = *problem.prompt() else {
                panic!("expected a simplify prompt");
            };
            let canonical = problem.canonical_solution();
            // The shown fraction is genuinely reducible and proper...
            assert!(gcd(shown.numerator(), shown.denominator()) >= 2);
            assert!(shown.numerator() < shown.denominator());
            // ...and its reduced value is the canonical answer, still a fraction.
            assert_eq!(shown.value(), canonical);
            assert!(canonical.denominator() > 1, "the answer stays a fraction");
            // The contract demands the answer written as the reduced fraction.
            assert_eq!(
                problem.answer_contract(),
                &AnswerContract::FreeForm {
                    required_representation: Some(Representation::Fraction),
                    require_reduced: true,
                }
            );
        }
    }

    #[test]
    fn simplify_generator_clamps_and_validates_its_ranges() {
        // Base values below 1 and multipliers below 2 are clamped, not errors.
        assert!(SimplifyFraction::new("s", "b", 0..=5, 1..=3).is_ok());
        // A base without two distinct values cannot build a proper fraction.
        assert_eq!(
            SimplifyFraction::new("s", "b", 4..=4, 2..=3).unwrap_err(),
            GeneratorError::EmptyRange
        );
        // A multiplier range entirely below 2 clamps to empty.
        assert_eq!(
            SimplifyFraction::new("s", "b", 1..=9, 1..=1).unwrap_err(),
            GeneratorError::EmptyRange
        );
        // A scaled part that could overflow i64 is refused up front.
        assert_eq!(
            SimplifyFraction::new("s", "b", 1..=i64::MAX, 2..=i64::MAX).unwrap_err(),
            GeneratorError::RangeOverflows
        );
        // The clamped generator only poses clamped values: base 1..=3 under a
        // multiplier of exactly 2.
        let generator = SimplifyFraction::new("s", "b", 0..=3, 1..=2).unwrap();
        let mut rng = Rng::new(3);
        for _ in 0..100 {
            let problem = generator.generate(&mut rng);
            let Prompt::Simplify(shown) = *problem.prompt() else {
                panic!("expected a simplify prompt");
            };
            assert!(shown.numerator() >= 2);
            assert!(shown.denominator() <= 6);
        }
    }

    #[test]
    fn simplify_generation_is_deterministic_for_a_seed() {
        let build = || SimplifyFraction::new("s", "b", 1..=9, 2..=50).unwrap();
        let (first, second) = (build(), build());
        let mut a = Rng::new(37);
        let mut b = Rng::new(37);
        for _ in 0..50 {
            assert_eq!(first.generate(&mut a), second.generate(&mut b));
        }
    }

    #[test]
    fn fraction_arithmetic_adds_proper_fractions_exactly() {
        // The summit-band shape: three-digit numerators under three-digit
        // denominators, e.g. 212/325 + 128/225.
        let generator =
            FractionArithmetic::new("f-add", "fractions", Operator::Add, 100..=299, 150..=350)
                .unwrap();
        let mut rng = Rng::new(41);
        for _ in 0..200 {
            let e = equation_of(&generator.generate(&mut rng));
            assert_eq!(e.operator(), Operator::Add);
            assert_eq!(e.unknown(), Slot::Result);
            for operand in [e.lhs(), e.rhs()] {
                assert!(operand > ExactValue::ZERO && operand < ExactValue::ONE);
            }
            assert_eq!(e.lhs().try_add(e.rhs()).unwrap(), e.result());
        }
    }

    #[test]
    fn fraction_arithmetic_multiplies_proper_fractions_exactly() {
        let generator =
            FractionArithmetic::new("f-mul", "fractions", Operator::Multiply, 1..=9, 2..=12)
                .unwrap();
        let mut rng = Rng::new(43);
        for _ in 0..200 {
            let e = equation_of(&generator.generate(&mut rng));
            assert_eq!(e.operator(), Operator::Multiply);
            for operand in [e.lhs(), e.rhs()] {
                assert!(operand > ExactValue::ZERO && operand < ExactValue::ONE);
            }
            assert_eq!(e.lhs().try_mul(e.rhs()).unwrap(), e.result());
        }
    }

    #[test]
    fn fraction_arithmetic_rejects_unsupported_operators_and_bad_ranges() {
        for operator in [Operator::Subtract, Operator::Divide] {
            assert_eq!(
                FractionArithmetic::new("f", "b", operator, 1..=9, 2..=12).unwrap_err(),
                GeneratorError::OperatorUnsupported
            );
        }
        // Numerators must start below the denominators, or the proper window
        // under a minimal denominator would be empty.
        assert_eq!(
            FractionArithmetic::new("f", "b", Operator::Add, 5..=9, 5..=12).unwrap_err(),
            GeneratorError::EmptyRange
        );
        // Parts that could overflow a sum's cross-multiplication are refused.
        assert_eq!(
            FractionArithmetic::new("f", "b", Operator::Add, 1..=i64::MAX / 2, 2..=i64::MAX / 2)
                .unwrap_err(),
            GeneratorError::RangeOverflows
        );
    }

    #[test]
    fn fraction_arithmetic_is_deterministic_for_a_seed() {
        let build =
            || FractionArithmetic::new("f", "b", Operator::Add, 100..=299, 150..=350).unwrap();
        let (first, second) = (build(), build());
        let mut a = Rng::new(47);
        let mut b = Rng::new(47);
        for _ in 0..50 {
            assert_eq!(first.generate(&mut a), second.generate(&mut b));
        }
    }

    #[test]
    fn simplify_problems_convert_to_multiple_choice() {
        let generator = SimplifyFraction::new("s", "b", 1..=9, 2..=50).unwrap();
        let mut rng = Rng::new(53);
        for _ in 0..50 {
            let problem = generator.generate(&mut rng);
            let canonical = problem.canonical_solution();
            let mc = into_multiple_choice(problem, &mut rng, 4).unwrap();
            let AnswerContract::MultipleChoice { options } = mc.answer_contract() else {
                panic!("expected a multiple-choice contract");
            };
            assert_eq!(options.len(), 4);
            assert_eq!(options.iter().filter(|&&o| o == canonical).count(), 1);
            for (i, option) in options.iter().enumerate() {
                assert!(!option.is_negative());
                assert!(
                    !options[i + 1..].contains(option),
                    "options must be distinct"
                );
            }
        }
    }

    #[test]
    fn mix_needs_at_least_one_entry() {
        assert_eq!(Mix::new(vec![]).unwrap_err(), GeneratorError::EmptyMix);
    }

    #[test]
    fn mix_rejects_a_zero_weight_entry() {
        let add = DirectArithmetic::new("sums", "addition", Operator::Add, 0..=9).unwrap();
        let sub = DirectArithmetic::new("diff", "subtraction", Operator::Subtract, 0..=9).unwrap();
        let err = Mix::new(vec![(40, boxed(add)), (0, boxed(sub))]).unwrap_err();
        assert_eq!(err, GeneratorError::ZeroMixWeight);
    }

    #[test]
    fn mix_draws_operators_roughly_by_weight() {
        // The shape of a mid-gauntlet level: double-digit add/sub at 40 each,
        // single-digit multiplication at 20 — an 80/20 split.
        let add = DirectArithmetic::new("sums", "addition", Operator::Add, 10..=99)
            .unwrap()
            .with_max_distance(11)
            .unwrap();
        let sub = DirectArithmetic::new("diff", "subtraction", Operator::Subtract, 10..=99)
            .unwrap()
            .with_max_distance(11)
            .unwrap();
        let mul = DirectArithmetic::new("facts", "multiplication", Operator::Multiply, 2..=9)
            .unwrap()
            .with_max_distance(8)
            .unwrap();
        let mix = Mix::new(vec![(40, boxed(add)), (40, boxed(sub)), (20, boxed(mul))]).unwrap();

        let mut rng = Rng::new(17);
        let (mut adds, mut subs, mut muls) = (0, 0, 0);
        for _ in 0..1000 {
            let e = equation_of(&mix.generate(&mut rng));
            let a = e.lhs().as_integer().unwrap();
            let b = e.rhs().as_integer().unwrap();
            match e.operator() {
                Operator::Add => {
                    assert!((a - b).abs() <= 11, "operands {a} and {b} drift past 11");
                    adds += 1;
                }
                Operator::Subtract => {
                    assert!((a - b).abs() <= 11, "operands {a} and {b} drift past 11");
                    subs += 1;
                }
                Operator::Multiply => {
                    assert!((a - b).abs() <= 8, "operands {a} and {b} drift past 8");
                    assert!((2..=9).contains(&a) && (2..=9).contains(&b));
                    muls += 1;
                }
                Operator::Divide => panic!("no division in the mix"),
            }
        }
        assert_eq!(adds + subs + muls, 1000);
        // Deterministic for the seed; the bands are generous so the assertion
        // documents proportion, not the exact draw.
        assert!((300..=500).contains(&adds), "adds: {adds}");
        assert!((300..=500).contains(&subs), "subs: {subs}");
        assert!((100..=300).contains(&muls), "muls: {muls}");
    }

    #[test]
    fn mix_with_one_entry_always_uses_it() {
        let mul =
            DirectArithmetic::new("facts", "multiplication", Operator::Multiply, 0..=5).unwrap();
        let mix = Mix::new(vec![(1, boxed(mul))]).unwrap();
        let mut rng = Rng::new(23);
        for _ in 0..100 {
            assert_eq!(
                equation_of(&mix.generate(&mut rng)).operator(),
                Operator::Multiply
            );
        }
    }

    #[test]
    fn mix_is_deterministic_for_a_seed() {
        let build = || {
            let direct = DirectArithmetic::new("s", "addition", Operator::Add, 0..=20).unwrap();
            let term = MissingTerm::new("m", "addition", Operator::Add, 0..=20).unwrap();
            Mix::new(vec![(3, boxed(direct)), (1, boxed(term))]).unwrap()
        };
        let (first, second) = (build(), build());
        let mut a = Rng::new(29);
        let mut b = Rng::new(29);
        for _ in 0..100 {
            assert_eq!(first.generate(&mut a), second.generate(&mut b));
        }
    }

    #[test]
    fn into_multiple_choice_builds_distinct_options_with_one_correct() {
        let generator = DirectArithmetic::new("sums", "addition", Operator::Add, 0..=20).unwrap();
        let mut rng = Rng::new(7);
        for _ in 0..100 {
            let problem = generator.generate(&mut rng);
            let canonical = problem.canonical_solution();
            let mc = into_multiple_choice(problem, &mut rng, 4).unwrap();

            let AnswerContract::MultipleChoice { options } = mc.answer_contract() else {
                panic!("expected a multiple-choice contract");
            };
            assert_eq!(options.len(), 4);
            // Exactly one option is the correct answer.
            assert_eq!(options.iter().filter(|&&o| o == canonical).count(), 1);
            // Options are distinct and non-negative.
            for (i, a) in options.iter().enumerate() {
                assert!(!a.is_negative());
                assert!(!options[i + 1..].contains(a), "options must be distinct");
            }
            // The transform preserves the prompt's canonical answer.
            assert_eq!(mc.canonical_solution(), canonical);
        }
    }

    #[test]
    fn into_multiple_choice_is_deterministic_for_a_seed() {
        let generator = DirectArithmetic::new("s", "b", Operator::Add, 0..=50).unwrap();
        let build = |seed| {
            let mut rng = Rng::new(seed);
            let problem = generator.generate(&mut rng);
            into_multiple_choice(problem, &mut rng, 4).unwrap()
        };
        assert_eq!(build(3), build(3));
    }

    #[test]
    fn into_multiple_choice_needs_at_least_two_options() {
        let generator = DirectArithmetic::new("s", "b", Operator::Add, 0..=9).unwrap();
        let mut rng = Rng::new(1);
        let problem = generator.generate(&mut rng);
        assert_eq!(
            into_multiple_choice(problem, &mut rng, 1),
            Err(MultipleChoiceError::TooFewOptions)
        );
    }
}
