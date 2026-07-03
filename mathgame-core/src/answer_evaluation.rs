//! Answer evaluation: turn a learner's answer into assessment evidence.
//!
//! Thin first cut — free-form answers only. The learner's text is parsed into an
//! exact value and checked against the problem's [`AnswerContract`]: exact value
//! equality, plus an optional required representation. The result is the shared
//! [`Evaluation`] shape that the mastery layer consumes.
//!
//! Multiple-choice answers, misconception-encoding distractors, and rich
//! diagnostics arrive with the richer-content step; here `error_kind` distinguishes
//! only unparseable input, a wrong value, and a wrong representation.

use crate::curriculum::SkillId;
use crate::math_core::{ExactValue, Representation};
use crate::problem_generation::{AnswerContract, Problem};

/// Why a submitted answer was not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// The answer could not be parsed as a number.
    Unparseable,
    /// The value was wrong.
    Incorrect,
    /// The value was correct but written in the wrong form (e.g. a fraction when
    /// a percent was required).
    WrongRepresentation {
        required: Representation,
        used: Representation,
    },
}

/// Evidence that one skill was exercised by an attempt — the input the mastery
/// layer accumulates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillEvidence {
    pub skill: SkillId,
    pub correct: bool,
}

/// The outcome of evaluating an answer: the shared shape mastery consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluation {
    correct: bool,
    canonical_answer: ExactValue,
    accepted_equivalent: Option<ExactValue>,
    error_kind: Option<ErrorKind>,
    skill_evidence: Vec<SkillEvidence>,
}

impl Evaluation {
    /// Whether the answer was accepted.
    #[must_use]
    pub fn is_correct(&self) -> bool {
        self.correct
    }

    /// The canonical (expected) exact value.
    #[must_use]
    pub fn canonical_answer(&self) -> ExactValue {
        self.canonical_answer
    }

    /// The learner's value when it was accepted as equivalent (e.g. `2/4` for a
    /// canonical `1/2`); `None` on a wrong or unparseable answer.
    #[must_use]
    pub fn accepted_equivalent(&self) -> Option<ExactValue> {
        self.accepted_equivalent
    }

    /// Why the answer was rejected, if it was.
    #[must_use]
    pub fn error_kind(&self) -> Option<&ErrorKind> {
        self.error_kind.as_ref()
    }

    /// Per-skill correctness evidence for this attempt.
    #[must_use]
    pub fn skill_evidence(&self) -> &[SkillEvidence] {
        &self.skill_evidence
    }

    fn correct(canonical: ExactValue, submitted: ExactValue, skills: &[SkillId]) -> Self {
        Self {
            correct: true,
            canonical_answer: canonical,
            accepted_equivalent: Some(submitted),
            error_kind: None,
            skill_evidence: evidence(skills, true),
        }
    }

    fn incorrect(canonical: ExactValue, error: ErrorKind, skills: &[SkillId]) -> Self {
        Self {
            correct: false,
            canonical_answer: canonical,
            accepted_equivalent: None,
            error_kind: Some(error),
            skill_evidence: evidence(skills, false),
        }
    }
}

fn evidence(skills: &[SkillId], correct: bool) -> Vec<SkillEvidence> {
    skills
        .iter()
        .map(|skill| SkillEvidence {
            skill: skill.clone(),
            correct,
        })
        .collect()
}

/// Evaluate a free-form `answer` against `problem`.
///
/// The answer is parsed into an exact value; it is accepted when it equals the
/// canonical solution and (if the contract requires one) is written in the
/// required representation. Any form equal to the canonical value is accepted
/// otherwise — `2/4`, `0.5`, and `50%` all satisfy a canonical `1/2`.
#[must_use]
pub fn evaluate(problem: &Problem, answer: &str) -> Evaluation {
    let canonical = problem.canonical_solution();
    let skills = problem.skills();
    let AnswerContract::FreeForm {
        required_representation,
    } = problem.answer_contract();

    let (value, representation) = match ExactValue::parse(answer) {
        Ok(parsed) => parsed,
        Err(_) => return Evaluation::incorrect(canonical, ErrorKind::Unparseable, skills),
    };
    if value != canonical {
        return Evaluation::incorrect(canonical, ErrorKind::Incorrect, skills);
    }
    if let Some(required) = required_representation
        && representation != *required
    {
        return Evaluation::incorrect(
            canonical,
            ErrorKind::WrongRepresentation {
                required: *required,
                used: representation,
            },
            skills,
        );
    }
    Evaluation::correct(canonical, value, skills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curriculum::BandId;
    use crate::math_core::Operator;
    use crate::problem_generation::{DirectArithmetic, Equation, Generator, Prompt, Slot};
    use crate::rng::Rng;

    /// A problem whose only relevant fields for evaluation are the canonical
    /// solution, the contract, and the skills (the prompt is a placeholder).
    fn problem_with(canonical: ExactValue, contract: AnswerContract) -> Problem {
        let equation = Equation::new(
            canonical,
            Operator::Add,
            ExactValue::ZERO,
            canonical,
            Slot::Result,
        )
        .unwrap();
        Problem::new(
            Prompt::Equation(equation),
            vec![SkillId::from("s")],
            BandId::from("b"),
            contract,
        )
    }

    fn free_form() -> AnswerContract {
        AnswerContract::FreeForm {
            required_representation: None,
        }
    }

    #[test]
    fn a_correct_generated_answer_is_accepted() {
        let generator =
            DirectArithmetic::new("sums-to-20", "addition", Operator::Add, 0..=20).unwrap();
        let mut rng = Rng::new(1);
        let problem = generator.generate(&mut rng);
        let answer = problem.canonical_solution().to_fraction_string();

        let evaluation = evaluate(&problem, &answer);
        assert!(evaluation.is_correct());
        assert_eq!(evaluation.error_kind(), None);
        assert_eq!(
            evaluation.accepted_equivalent(),
            Some(problem.canonical_solution())
        );
        assert_eq!(
            evaluation.skill_evidence(),
            &[SkillEvidence {
                skill: SkillId::from("sums-to-20"),
                correct: true,
            }]
        );
    }

    #[test]
    fn a_wrong_value_is_incorrect() {
        let generator = DirectArithmetic::new("sums", "addition", Operator::Add, 0..=20).unwrap();
        let mut rng = Rng::new(2);
        let problem = generator.generate(&mut rng);
        let wrong = problem
            .canonical_solution()
            .try_add(ExactValue::ONE)
            .unwrap()
            .to_fraction_string();

        let evaluation = evaluate(&problem, &wrong);
        assert!(!evaluation.is_correct());
        assert_eq!(evaluation.error_kind(), Some(&ErrorKind::Incorrect));
        assert_eq!(evaluation.accepted_equivalent(), None);
        assert!(evaluation.skill_evidence().iter().all(|e| !e.correct));
    }

    #[test]
    fn unparseable_input_is_flagged() {
        let problem = problem_with(ExactValue::integer(5), free_form());
        let evaluation = evaluate(&problem, "not a number");
        assert!(!evaluation.is_correct());
        assert_eq!(evaluation.error_kind(), Some(&ErrorKind::Unparseable));
    }

    #[test]
    fn any_equivalent_form_is_accepted_when_no_representation_is_required() {
        let half = ExactValue::rational(1, 2).unwrap();
        let problem = problem_with(half, free_form());
        for form in ["1/2", "2/4", "0.5", "50%"] {
            assert!(evaluate(&problem, form).is_correct(), "{form} should pass");
        }
    }

    #[test]
    fn a_required_representation_is_enforced() {
        let quarter = ExactValue::rational(1, 4).unwrap();
        let problem = problem_with(
            quarter,
            AnswerContract::FreeForm {
                required_representation: Some(Representation::Percent),
            },
        );
        // Correct value, correct form.
        assert!(evaluate(&problem, "25%").is_correct());
        // Correct value, wrong form.
        let evaluation = evaluate(&problem, "1/4");
        assert!(!evaluation.is_correct());
        assert_eq!(
            evaluation.error_kind(),
            Some(&ErrorKind::WrongRepresentation {
                required: Representation::Percent,
                used: Representation::Fraction,
            })
        );
    }
}
