//! Copy utilities: reusable helpers for game-facing text.
//!
//! A game's copy lives in its config as *data* — titles, prompts, verdicts,
//! and format strings — so no on-screen string is a Rust literal. These
//! helpers turn that data into display strings.

/// Fill a copy template's `{}` placeholders left-to-right with `args`. A
/// template with more placeholders than args leaves the surplus braces in
/// place, and extra args are ignored — a mismatch degrades visibly rather than
/// panicking. This is how a game's format strings live in its config data
/// (`"SCORE {}  LIVES {}  L{}"`) instead of in Rust `format!` literals.
#[must_use]
pub fn fill_placeholders(template: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut args = args.iter();
    let mut rest = template;
    while let Some(pos) = rest.find("{}") {
        out.push_str(&rest[..pos]);
        match args.next() {
            Some(arg) => out.push_str(arg),
            None => out.push_str("{}"),
        }
        rest = &rest[pos + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_substitutes_placeholders_left_to_right() {
        assert_eq!(
            fill_placeholders(
                "SCORE {}  LIVES {}  L{}",
                &["10".into(), "3".into(), "2".into()]
            ),
            "SCORE 10  LIVES 3  L2"
        );
        // A surplus placeholder keeps its braces; extra args are ignored.
        assert_eq!(fill_placeholders("{} of {}", &["1".into()]), "1 of {}");
        assert_eq!(fill_placeholders("no args", &["x".into()]), "no args");
        assert_eq!(fill_placeholders("{}%", &["87".into()]), "87%");
    }
}
