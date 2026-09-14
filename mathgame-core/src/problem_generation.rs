//! Problem generation: named generators that produce [`Problem`]s with
//! constrained, reproducible parameters.
//!
//! A [`Problem`] pairs a [`Prompt`] with the skills it exercises, its band, an
//! [`AnswerContract`], and the canonical exact solution. Generators draw from a
//! seeded [`crate::rng::Rng`], so a drill replays identically.
//!
//! This module owns the problem *model*, the whole-number arithmetic
//! generators ([`DirectArithmetic`], [`MissingTerm`]), the fraction generators
//! ([`SimplifyFraction`], [`FractionArithmetic`]), their weighted composition
//! ([`Mix`]), and multiple-choice distractor generation
//! ([`into_multiple_choice`]) — the options are shown before the learner
//! answers, so they are problem-time state. Answer parsing and diagnostics live
//! in the later `answer_evaluation` module: [`AnswerContract`] here is the
//! model; evaluating against it is behaviour there.

// Keep the existing public module and root re-exports as the consumer facade.
mod direct_arithmetic;
mod fraction_arithmetic;
mod generator;
mod missing_term;
mod mix;
mod model;
mod multiple_choice;
mod simplify_fraction;
mod whole_number;

pub use direct_arithmetic::DirectArithmetic;
pub use fraction_arithmetic::FractionArithmetic;
pub use generator::{Generator, GeneratorError};
pub use missing_term::MissingTerm;
pub use mix::Mix;
pub use model::{
    AnswerContract, Equation, EquationError, Problem, Prompt, Slot, UnreducedFraction,
};
pub use multiple_choice::{MultipleChoiceError, into_multiple_choice};
pub use simplify_fraction::SimplifyFraction;

#[cfg(test)]
mod test_support;
