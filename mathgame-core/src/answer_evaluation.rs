//! Answer evaluation: turn a learner's [`Response`] into assessment evidence.
//!
//! A response is matched against the problem's [`AnswerContract`]: a free-form
//! typed answer is parsed to an exact value and checked for value equality (plus
//! an optional required representation); a multiple-choice selection is checked
//! against the option set. The result is the shared [`Evaluation`] the mastery
//! layer consumes, and correctness is derived from the prompt's canonical answer
//! — evaluation never trusts a "correct option" flag.
//!
//! This is an arcade game, not courseware: evidence is correctness-only, with no
//! misconception or remediation tracking.

use crate::curriculum::SkillId;
use crate::math_core::{ExactValue, Representation};
use crate::problem_generation::{AnswerContract, Problem};

/// A learner's response to a [`Problem`], matched against its answer contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// A typed free-form answer.
    Typed(String),
    /// The index of a selected option (multiple choice), into the contract's
    /// options in display order.
    Selected(usize),
}

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
    /// The selected option index was outside the available choices.
    NoSuchChoice,
    /// The response did not match the problem's contract — a typed answer to a
    /// multiple-choice problem, or a selection to a free-form one.
    WrongResponseKind,
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

/// Evaluate a learner's `response` against `problem`.
///
/// The response is matched to the problem's contract. A [`Response::Typed`]
/// answer is parsed and accepted when it equals the canonical solution and (if
/// the contract requires one) is written in the required representation — any
/// equal form otherwise satisfies it (`2/4`, `0.5`, and `50%` all match a
/// canonical `1/2`). A [`Response::Selected`] option is accepted when it equals
/// the canonical answer. A response whose kind does not fit the contract is
/// rejected.
#[must_use]
pub fn evaluate(problem: &Problem, response: &Response) -> Evaluation {
    let canonical = problem.canonical_solution();
    let skills = problem.skills();
    match (problem.answer_contract(), response) {
        (
            AnswerContract::FreeForm {
                required_representation,
            },
            Response::Typed(answer),
        ) => evaluate_typed(canonical, skills, answer, required_representation.as_ref()),
        (AnswerContract::MultipleChoice { options }, Response::Selected(index)) => {
            evaluate_selection(canonical, skills, options, *index)
        }
        (AnswerContract::FreeForm { .. }, Response::Selected(_))
        | (AnswerContract::MultipleChoice { .. }, Response::Typed(_)) => {
            Evaluation::incorrect(canonical, ErrorKind::WrongResponseKind, skills)
        }
    }
}

fn evaluate_typed(
    canonical: ExactValue,
    skills: &[SkillId],
    answer: &str,
    required: Option<&Representation>,
) -> Evaluation {
    let (value, representation) = match ExactValue::parse(answer) {
        Ok(parsed) => parsed,
        Err(_) => return Evaluation::incorrect(canonical, ErrorKind::Unparseable, skills),
    };
    if value != canonical {
        return Evaluation::incorrect(canonical, ErrorKind::Incorrect, skills);
    }
    if let Some(required) = required
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

fn evaluate_selection(
    canonical: ExactValue,
    skills: &[SkillId],
    options: &[ExactValue],
    index: usize,
) -> Evaluation {
    match options.get(index) {
        None => Evaluation::incorrect(canonical, ErrorKind::NoSuchChoice, skills),
        Some(&picked) if picked == canonical => Evaluation::correct(canonical, picked, skills),
        Some(_) => Evaluation::incorrect(canonical, ErrorKind::Incorrect, skills),
    }
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

        let evaluation = evaluate(&problem, &Response::Typed(answer));
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

        let evaluation = evaluate(&problem, &Response::Typed(wrong));
        assert!(!evaluation.is_correct());
        assert_eq!(evaluation.error_kind(), Some(&ErrorKind::Incorrect));
        assert_eq!(evaluation.accepted_equivalent(), None);
        assert!(evaluation.skill_evidence().iter().all(|e| !e.correct));
    }

    #[test]
    fn unparseable_input_is_flagged() {
        let problem = problem_with(ExactValue::integer(5), free_form());
        let evaluation = evaluate(&problem, &Response::Typed("not a number".into()));
        assert!(!evaluation.is_correct());
        assert_eq!(evaluation.error_kind(), Some(&ErrorKind::Unparseable));
    }

    #[test]
    fn any_equivalent_form_is_accepted_when_no_representation_is_required() {
        let half = ExactValue::rational(1, 2).unwrap();
        let problem = problem_with(half, free_form());
        for form in ["1/2", "2/4", "0.5", "50%"] {
            assert!(
                evaluate(&problem, &Response::Typed(form.into())).is_correct(),
                "{form} should pass"
            );
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
        assert!(evaluate(&problem, &Response::Typed("25%".into())).is_correct());
        // Correct value, wrong form.
        let evaluation = evaluate(&problem, &Response::Typed("1/4".into()));
        assert!(!evaluation.is_correct());
        assert_eq!(
            evaluation.error_kind(),
            Some(&ErrorKind::WrongRepresentation {
                required: Representation::Percent,
                used: Representation::Fraction,
            })
        );
    }

    /// A multiple-choice problem with `canonical` as its answer and an explicit
    /// option set (so tests know which index is correct).
    fn multiple_choice(canonical: ExactValue, options: Vec<ExactValue>) -> Problem {
        problem_with(canonical, AnswerContract::MultipleChoice { options })
    }

    fn ints(values: [i64; 3]) -> Vec<ExactValue> {
        values.into_iter().map(ExactValue::integer).collect()
    }

    #[test]
    fn selecting_the_correct_option_is_accepted() {
        // canonical 7, options [4, 7, 9]; the correct index is 1.
        let problem = multiple_choice(ExactValue::integer(7), ints([4, 7, 9]));
        let evaluation = evaluate(&problem, &Response::Selected(1));
        assert!(evaluation.is_correct());
        assert_eq!(evaluation.error_kind(), None);
        assert_eq!(
            evaluation.accepted_equivalent(),
            Some(ExactValue::integer(7))
        );
        assert!(evaluation.skill_evidence().iter().all(|e| e.correct));
    }

    #[test]
    fn selecting_a_distractor_is_incorrect() {
        let problem = multiple_choice(ExactValue::integer(7), ints([4, 7, 9]));
        let evaluation = evaluate(&problem, &Response::Selected(0));
        assert!(!evaluation.is_correct());
        assert_eq!(evaluation.error_kind(), Some(&ErrorKind::Incorrect));
        assert!(evaluation.skill_evidence().iter().all(|e| !e.correct));
    }

    #[test]
    fn an_out_of_range_selection_is_rejected() {
        let problem = multiple_choice(ExactValue::integer(7), ints([4, 7, 9]));
        let evaluation = evaluate(&problem, &Response::Selected(9));
        assert!(!evaluation.is_correct());
        assert_eq!(evaluation.error_kind(), Some(&ErrorKind::NoSuchChoice));
    }

    #[test]
    fn a_response_of_the_wrong_kind_is_rejected() {
        // A typed answer to a multiple-choice problem.
        let mc = multiple_choice(ExactValue::integer(7), ints([4, 7, 9]));
        assert_eq!(
            evaluate(&mc, &Response::Typed("7".into())).error_kind(),
            Some(&ErrorKind::WrongResponseKind)
        );
        // A selection against a free-form problem.
        let ff = problem_with(ExactValue::integer(7), free_form());
        assert_eq!(
            evaluate(&ff, &Response::Selected(0)).error_kind(),
            Some(&ErrorKind::WrongResponseKind)
        );
    }
}
