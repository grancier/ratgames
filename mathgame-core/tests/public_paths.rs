//! Consumer contracts for the existing root and nested public module paths.
use mathgame_core::{self as core, problem_generation};

#[test]
fn root_exports_and_legacy_module_exports_are_the_same_types() {
    macro_rules! same_types {
        ($module:ident: $($name:ident),+ $(,)?) => {
            $(let _: Option<core::$name> = None::<core::$module::$name>;)+
        };
    }
    same_types!(math_core: ExactValue, Representation, ValueError, ParseError,
        Operator, Token, EvaluationRule, Expression, EvalError);
    same_types!(problem_generation: Slot, EquationError, Equation, UnreducedFraction,
        Prompt, AnswerContract, Problem, MultipleChoiceError, GeneratorError,
        DirectArithmetic, MissingTerm, SimplifyFraction, FractionArithmetic, Mix);
    same_types!(answer_evaluation: Response, ErrorKind, SkillEvidence, Evaluation);
    same_types!(curriculum: Band, BandId, Curriculum, CurriculumError, Skill, SkillId);
    same_types!(rng: Rng);
}

#[test]
fn a_consumer_can_generate_and_grade_through_legacy_paths() {
    let generator = problem_generation::DirectArithmetic::new(
        "addition",
        "intro",
        core::math_core::Operator::Add,
        2..=4,
    )
    .unwrap();
    // The trait is also available through both original public paths.
    let generator: &dyn core::Generator = &generator;
    let generator: &dyn problem_generation::Generator = generator;
    let mut rng = core::rng::Rng::new(42);
    let problem: core::Problem = generator.generate(&mut rng);
    let answer = problem.canonical_solution().to_fraction_string();
    let response: core::Response = core::answer_evaluation::Response::Typed(answer);
    assert!(core::answer_evaluation::evaluate(&problem, &response).is_correct());

    let choice: core::Problem =
        problem_generation::into_multiple_choice(problem, &mut rng, 4).unwrap();
    let core::AnswerContract::MultipleChoice { options } = choice.answer_contract() else {
        panic!("expected the multiple-choice contract");
    };
    let index = options
        .iter()
        .position(|value| *value == choice.canonical_solution())
        .unwrap();
    assert!(core::evaluate(&choice, &core::Response::Selected(index)).is_correct());
}
