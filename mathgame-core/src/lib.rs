//! `mathgame-core` — the learning domain for the math game.
//!
//! This crate answers one question: *what does the learner need to understand,
//! and what evidence proves they are ready to advance?* It knows nothing about
//! sprites, rooms, menus, framebuffers, storage, or config formats — and that
//! separation is enforced **structurally**: the crate has no dependencies, so it
//! *cannot* reach the presentation, persistence, or platform layers. Game layers
//! depend on the domain, never the reverse.
//!
//! The design keeps four things deliberately separate (the critical boundary):
//! mathematical **truth** (exact value + equivalence), the learning
//! **objective** (which representation/method is being practised), assessment
//! **evidence** (correctness + misconception), and **game outcome** (points,
//! rooms, animations). Only the first lives in [`math_core`]; the rest arrive in
//! later modules.
//!
//! Modules (built incrementally):
//! - [`math_core`] — exact arithmetic and expression evaluation: the domain's
//!   notion of mathematical truth.
//! - [`curriculum`] — the skill graph: skills, bands, prerequisites, objectives.
//! - [`problem_generation`] — the `Problem` model and seeded generators.
//! - [`answer_evaluation`] — parse and check answers into a shared `Evaluation`.
//! - [`mastery`] — accumulate evidence into a per-skill readiness signal.
//! - [`rng`] — a deterministic PRNG for reproducible generation.
//!
//! Planned: `learning_policy`.

pub mod answer_evaluation;
pub mod curriculum;
pub mod mastery;
pub mod math_core;
pub mod problem_generation;
pub mod rng;

pub use answer_evaluation::{ErrorKind, Evaluation, SkillEvidence, evaluate};
pub use curriculum::{Band, BandId, Curriculum, CurriculumError, Skill, SkillId};
pub use mastery::{Mastery, MasteryPolicy, MasteryPolicyError, SkillMastery, SkillState};
pub use math_core::{
    EvalError, EvaluationRule, ExactValue, Expression, Operator, ParseError, Representation, Token,
    ValueError,
};
pub use problem_generation::{
    AnswerContract, DirectArithmetic, Equation, EquationError, Generator, GeneratorError,
    MissingTerm, Problem, Prompt, Slot,
};
pub use rng::Rng;
