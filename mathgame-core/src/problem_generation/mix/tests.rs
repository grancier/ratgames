use super::super::test_support::*;
use super::super::*;
use crate::math_core::Operator;
use crate::rng::Rng;

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
    let mul = DirectArithmetic::new("facts", "multiplication", Operator::Multiply, 0..=5).unwrap();
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
