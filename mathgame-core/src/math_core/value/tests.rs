use super::*;

fn r(num: i64, den: i64) -> ExactValue {
    ExactValue::rational(num, den).unwrap()
}

fn parse(s: &str) -> ExactValue {
    ExactValue::parse(s).unwrap().0
}

#[test]
fn rationals_reduce_and_sign_normalize() {
    assert_eq!(r(2, 4), r(1, 2));
    assert_eq!(r(6, 3), ExactValue::integer(2));
    // The denominator is always positive; a negative sign lives on the top.
    let half_neg = r(1, -2);
    assert_eq!(half_neg.numerator(), -1);
    assert_eq!(half_neg.denominator(), 2);
    assert_eq!(r(-1, -2), r(1, 2));
    assert_eq!(ExactValue::ZERO, r(0, 5));
}

#[test]
fn zero_denominator_is_rejected() {
    assert_eq!(ExactValue::rational(1, 0), Err(ValueError::DivideByZero));
}

#[test]
fn equivalent_forms_are_one_value() {
    // The whole point: 1/2, 2/4, 0.5, and 50% are the same exact value.
    let half = r(1, 2);
    assert_eq!(parse("1/2"), half);
    assert_eq!(parse("2/4"), half);
    assert_eq!(parse("0.5"), half);
    assert_eq!(parse("50%"), half);
    // 25% == 1/4 == 0.25 (a foundational invariant).
    assert_eq!(parse("25%"), r(1, 4));
    assert_eq!(parse("0.25"), r(1, 4));
}

#[test]
fn parse_reports_the_written_representation() {
    assert_eq!(
        ExactValue::parse("347").unwrap(),
        (ExactValue::integer(347), Representation::Integer)
    );
    assert_eq!(
        ExactValue::parse("3/4").unwrap(),
        (r(3, 4), Representation::Fraction)
    );
    assert_eq!(
        ExactValue::parse("0.25").unwrap(),
        (r(1, 4), Representation::Decimal)
    );
    assert_eq!(
        ExactValue::parse("25%").unwrap(),
        (r(1, 4), Representation::Percent)
    );
}

#[test]
fn parse_handles_signs_bare_decimals_and_whitespace() {
    assert_eq!(parse("-3"), ExactValue::integer(-3));
    assert_eq!(parse("  12/8 "), r(3, 2));
    assert_eq!(parse(".5"), r(1, 2));
    assert_eq!(parse("-.25"), r(-1, 4));
    assert_eq!(parse("12.5%"), r(1, 8));
    assert_eq!(parse("1."), ExactValue::integer(1));
}

#[test]
fn parse_rejects_garbage() {
    assert_eq!(ExactValue::parse(""), Err(ParseError::Empty));
    assert_eq!(ExactValue::parse("   "), Err(ParseError::Empty));
    assert_eq!(ExactValue::parse("1/0"), Err(ParseError::DivideByZero));
    assert!(matches!(
        ExactValue::parse("abc"),
        Err(ParseError::Malformed(_))
    ));
    assert!(matches!(
        ExactValue::parse("1.2.3"),
        Err(ParseError::Malformed(_))
    ));
    assert!(matches!(
        ExactValue::parse("."),
        Err(ParseError::Malformed(_))
    ));
}

#[test]
fn parse_reports_overflow_distinct_from_malformed() {
    // A magnitude too large for i64 is diagnosed as overflow, not garbage.
    assert_eq!(
        ExactValue::parse("99999999999999999999999999"),
        Err(ParseError::Overflow)
    );
    assert_eq!(
        ExactValue::parse("-99999999999999999999999999"),
        Err(ParseError::Overflow)
    );
    // Overflow inside a fraction term is reported the same way.
    assert_eq!(
        ExactValue::parse("1/99999999999999999999999999"),
        Err(ParseError::Overflow)
    );
}

#[test]
fn ordering_is_by_value_not_by_field() {
    assert!(r(3, 4) > r(2, 3));
    assert!(r(1, 2) < ExactValue::ONE);
    // 1/2 vs 1/3: equal numerators, but 1/2 is larger — field order would lie.
    assert!(r(1, 2) > r(1, 3));
    let mut xs = [r(3, 4), r(1, 4), r(1, 2), ExactValue::ZERO];
    xs.sort();
    assert_eq!(xs, [ExactValue::ZERO, r(1, 4), r(1, 2), r(3, 4)]);
}

#[test]
fn arithmetic_is_exact() {
    assert_eq!(r(1, 2).try_add(r(1, 3)).unwrap(), r(5, 6));
    assert_eq!(r(3, 4).try_sub(r(1, 4)).unwrap(), r(1, 2));
    assert_eq!(r(2, 3).try_mul(r(3, 4)).unwrap(), r(1, 2));
    assert_eq!(r(1, 2).try_div(r(1, 4)).unwrap(), ExactValue::integer(2));
    assert_eq!(
        r(1, 2).try_div(ExactValue::ZERO),
        Err(ValueError::DivideByZero)
    );
}

#[test]
fn overflow_is_reported_not_panicked() {
    let big = ExactValue::integer(i64::MAX);
    assert_eq!(big.try_add(ExactValue::ONE), Err(ValueError::Overflow));
    // i64::MIN cannot be sign-flipped into a positive denominator.
    assert_eq!(ExactValue::rational(1, i64::MIN), Err(ValueError::Overflow));
}

#[test]
fn fraction_formatting() {
    assert_eq!(r(1, 2).to_fraction_string(), "1/2");
    assert_eq!(ExactValue::integer(3).to_fraction_string(), "3");
    assert_eq!(r(-1, 2).to_fraction_string(), "-1/2");
    assert_eq!(r(6, 3).to_fraction_string(), "2");
}

#[test]
fn decimal_formatting_is_exact_or_none() {
    assert_eq!(r(1, 4).to_decimal_string().as_deref(), Some("0.25"));
    assert_eq!(r(1, 2).to_decimal_string().as_deref(), Some("0.5"));
    assert_eq!(r(1, 8).to_decimal_string().as_deref(), Some("0.125"));
    assert_eq!(r(1, 5).to_decimal_string().as_deref(), Some("0.2"));
    assert_eq!(
        ExactValue::integer(5).to_decimal_string().as_deref(),
        Some("5")
    );
    assert_eq!(r(-1, 4).to_decimal_string().as_deref(), Some("-0.25"));
    // 1/3 does not terminate in base 10 — we refuse to round.
    assert_eq!(r(1, 3).to_decimal_string(), None);
}

#[test]
fn percent_formatting_is_exact_or_none() {
    assert_eq!(r(1, 4).to_percent_string().as_deref(), Some("25%"));
    assert_eq!(r(1, 8).to_percent_string().as_deref(), Some("12.5%"));
    assert_eq!(r(1, 2).to_percent_string().as_deref(), Some("50%"));
    assert_eq!(ExactValue::ONE.to_percent_string().as_deref(), Some("100%"));
    assert_eq!(r(1, 3).to_percent_string(), None);
}

#[test]
fn fraction_text_round_trips_through_parse() {
    // Property-style: every small rational survives format -> parse unchanged.
    for num in -20i64..=20 {
        for den in 1i64..=20 {
            let value = r(num, den);
            let reparsed = parse(&value.to_fraction_string());
            assert_eq!(value, reparsed, "round trip failed for {num}/{den}");
        }
    }
}

#[test]
fn addition_matches_an_independent_reference() {
    // Cross-check add() against a straight i128 computation for many pairs.
    for a in -8i64..=8 {
        for b in 1i64..=8 {
            for c in -8i64..=8 {
                for d in 1i64..=8 {
                    let got = r(a, b).try_add(r(c, d)).unwrap();
                    let ref_num = i128::from(a) * i128::from(d) + i128::from(c) * i128::from(b);
                    let ref_den = i128::from(b) * i128::from(d);
                    // got == ref_num/ref_den  <=>  got.num*ref_den == ref_num*got.den
                    let lhs = i128::from(got.numerator()) * ref_den;
                    let rhs = ref_num * i128::from(got.denominator());
                    assert_eq!(lhs, rhs, "{a}/{b} + {c}/{d}");
                }
            }
        }
    }
}
