//! Exact arithmetic: the domain's notion of mathematical **truth**.
//!
//! Every value is a normalized rational over `i64` — integers, fractions,
//! terminating decimals, and percentages are all the *same* underlying value, so
//! equality and comparison are exact and representation-independent:
//! `25% == 1/4 == 0.25`. There is no floating point anywhere; correctness, not
//! approximation, is the contract (foundational invariant: "no floating-point
//! equality").
//!
//! A value's *truth* is separate from its *representation*. [`ExactValue`] is the
//! truth; [`Representation`] records how a value was written (integer, fraction,
//! decimal, percent), which is what conversion problems and answer contracts
//! reason about later.
//!
//! [`Expression`]s combine value and operator [`Token`]s into a single exact
//! value under an [`EvaluationRule`] — the model behind expression-construction
//! (maze) tasks.

// Public paths remain stable while implementations live in private modules.
mod expression;
mod value;

pub use expression::{EvalError, EvaluationRule, Expression, Operator, Token};
pub use value::{ExactValue, ParseError, Representation, ValueError};
