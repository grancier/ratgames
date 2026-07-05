//! [`AnswerMode`] — how a player supplies an answer: typed, or a pick from a
//! fixed number of choices.

/// How a player answers a challenge: free-form typed text, or a selection from
/// `options` generated choices.
///
/// A generic game concept, not a math one — a game that grades typed answers
/// ([`InputField`](crate::InputField)) or presents multiple choice
/// ([`Menu`](crate::Menu) / [`MultipleChoice`](crate::MultipleChoice)) selects
/// between them here. The choice *content* (distractors) and the grading live in
/// the game's own domain; this only names the modality.
///
/// Serde-tagged so a game's config can carry the chosen mode, like
/// [`GameRules`](crate::GameRules): `{"kind":"typed"}` or
/// `{"kind":"multiple_choice","options":4}`. The [`Default`] is [`Typed`](Self::Typed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnswerMode {
    /// A typed free-form answer, graded by value.
    #[default]
    Typed,
    /// A pick from `options` generated choices (the answer plus distractors).
    MultipleChoice { options: usize },
}

/// Why an [`AnswerMode`] was rejected as unplayable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AnswerModeError {
    /// Multiple choice needs at least two options: the answer and one distractor.
    #[error("multiple_choice.options must be at least 2")]
    TooFewOptions,
}

impl AnswerMode {
    /// Check that the answer mode is playable.
    ///
    /// # Errors
    /// [`AnswerModeError::TooFewOptions`] when multiple-choice mode has fewer than
    /// two options.
    pub fn validate(self) -> Result<(), AnswerModeError> {
        match self {
            Self::Typed => Ok(()),
            Self::MultipleChoice { options } if options < 2 => Err(AnswerModeError::TooFewOptions),
            Self::MultipleChoice { .. } => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_typed() {
        assert_eq!(AnswerMode::default(), AnswerMode::Typed);
    }

    #[test]
    fn round_trips_through_json() {
        for mode in [AnswerMode::Typed, AnswerMode::MultipleChoice { options: 4 }] {
            let text = serde_json::to_string(&mode).expect("serialize");
            let parsed: AnswerMode = serde_json::from_str(&text).expect("deserialize");
            assert_eq!(parsed, mode);
        }
    }

    #[test]
    fn parses_the_tagged_forms() {
        let typed: AnswerMode = serde_json::from_str(r#"{"kind":"typed"}"#).expect("typed");
        assert_eq!(typed, AnswerMode::Typed);
        let mc: AnswerMode =
            serde_json::from_str(r#"{"kind":"multiple_choice","options":3}"#).expect("mc");
        assert_eq!(mc, AnswerMode::MultipleChoice { options: 3 });
    }

    #[test]
    fn validates_multiple_choice_option_count() {
        assert_eq!(AnswerMode::Typed.validate(), Ok(()));
        assert_eq!(
            AnswerMode::MultipleChoice { options: 1 }.validate(),
            Err(AnswerModeError::TooFewOptions)
        );
        assert_eq!(AnswerMode::MultipleChoice { options: 2 }.validate(), Ok(()));
    }
}
