use super::{Equation, Generator, Problem, Prompt};

pub(super) fn equation_of(problem: &Problem) -> Equation {
    let Prompt::Equation(equation) = problem.prompt() else {
        panic!("expected an equation prompt");
    };
    *equation
}

pub(super) fn boxed(generator: impl Generator + 'static) -> Box<dyn Generator> {
    Box::new(generator)
}

pub(super) fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a } else { gcd(b, a % b) }
}
