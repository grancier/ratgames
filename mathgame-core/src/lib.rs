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
//!
//! Planned: `problem_generation`, `answer_evaluation`, `mastery`,
//! `learning_policy`.

pub mod curriculum;
pub mod math_core;

pub use curriculum::{Band, BandId, Curriculum, CurriculumError, Skill, SkillId};
pub use math_core::{
    EvalError, EvaluationRule, ExactValue, Expression, Operator, ParseError, Representation, Token,
    ValueError,
};
