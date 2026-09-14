//! Multiple choice.
use super::{AnswerContract, Equation, Problem, Prompt};
use crate::math_core::{ExactValue, Operator};
use crate::rng::Rng;

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

#[cfg(test)]
mod tests;
