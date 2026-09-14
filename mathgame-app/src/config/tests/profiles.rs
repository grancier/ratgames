use super::super::*;

#[test]
fn invalid_profile_definitions_fail_at_the_config_boundary() {
    for (profile, expected) in [
        (
            r#"{"label":" ","problems":[{"operator":"add","min":1,"max":2}]}"#,
            "label",
        ),
        (r#"{"label":"HARD","problems":[]}"#, "problems"),
        (
            r#"{"label":"HARD","problems":[{"operator":"add","min":9,"max":2}]}"#,
            "problems",
        ),
        (
            r#"{"label":"HARD","problems":[{"operator":"add","min":1,"max":2,"weight":0}]}"#,
            "problems",
        ),
    ] {
        let json = format!(r#"{{"difficulty_profiles":{{"advanced":{profile}}}}}"#);
        let config: AppConfig = serde_json::from_str(&json).unwrap();
        let error = config
            .validate()
            .expect_err("invalid authored profile must fail")
            .to_string();
        assert!(
            error.contains("advanced") && error.contains(expected),
            "{error}"
        );
    }
}

#[test]
fn mode_ids_must_be_nonblank_and_unique_independent_of_labels() {
    for ids in [["", "hard"], [" ", "hard"], ["same", "same"]] {
        let config: AppConfig = serde_json::from_value(serde_json::json!({
            "difficulties": [
                {"id": ids[0], "label": "FIRST", "starting_lives": 1},
                {"id": ids[1], "label": "SECOND", "starting_lives": 1}
            ]
        }))
        .unwrap();
        let error = config
            .validate()
            .expect_err("ambiguous mode identity must fail")
            .to_string();
        assert!(error.contains("id"), "{error}");
    }
}

#[test]
fn bundled_difficulties_form_the_shipped_ladder() {
    // Shipped game design: three difficulties, easy to hard. Pin the shape
    // (labels, and that lives / time scale move the right way); the exact
    // values stay tunable.
    let config = AppConfig::resolve(None).expect("bundled config");
    let labels: Vec<_> = config
        .difficulties
        .iter()
        .map(|preset| preset.label.as_str())
        .collect();
    assert_eq!(labels, vec!["EASY", "NORMAL", "HARD"]);
    for pair in config.difficulties.windows(2) {
        assert!(
            pair[0].starting_lives >= pair[1].starting_lives,
            "lives never grow as the ladder hardens"
        );
        assert!(
            pair[0].time_percent >= pair[1].time_percent,
            "time never grows as the ladder hardens"
        );
    }
}

#[test]
fn validate_rejects_a_degenerate_difficulty_preset() {
    let preset = |label: &str, lives: u32, percent: u32| DifficultyPreset {
        id: None,
        label: label.to_string(),
        starting_lives: lives,
        time_percent: percent,
    };
    let with = |difficulties: Vec<DifficultyPreset>| AppConfig {
        difficulties,
        ..AppConfig::default()
    };

    assert!(with(vec![preset("OK", 3, 100)]).validate().is_ok());
    assert!(matches!(
        with(vec![preset("", 3, 100)]).validate(),
        Err(AppConfigError::Invalid(_))
    ));
    assert!(matches!(
        with(vec![preset("DEAD", 0, 100)]).validate(),
        Err(AppConfigError::Invalid(_))
    ));
    assert!(matches!(
        with(vec![preset("FROZEN", 3, 0)]).validate(),
        Err(AppConfigError::Invalid(_))
    ));

    // A preset the scoring lives cap forbids is caught at startup, not at
    // select time mid-flow.
    let mut capped = with(vec![preset("TOO ALIVE", 6, 100)]);
    capped.scoring.one_up.max_lives = 5;
    assert!(matches!(capped.validate(), Err(AppConfigError::Invalid(_))));
}
