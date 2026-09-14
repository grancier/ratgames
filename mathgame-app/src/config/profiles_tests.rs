use super::*;
use mathgame_core::{AnswerContract, Prompt};
use serde_json::{Value, json};

fn fixture() -> (AppConfig, Vec<AuthoredLevel>) {
    let config = serde_json::from_value(json!({
        "difficulties": [
            {"id":"easy", "label":"FIRST", "starting_lives":3, "time_percent":150},
            {"id":"hard", "label":"SECOND", "starting_lives":2, "time_percent":75}
        ],
        "difficulty_profiles": {
            "small":{"label":"INTRO", "problems":[{"operator":"add","min":2,"max":2}]},
            "large":{"label":"ADVANCED", "problems":[{"operator":"multiply","min":10,"max":10}]}
        }
    }))
    .unwrap();
    let level = serde_json::from_value(json!({
        "name":"FIRST LEVEL", "difficulty":"LEGACY", "required_successes":1,
        "time_limit_frames":600,
        "problems":[{"operator":"subtract","min":1,"max":1}],
        "profiles_by_mode":{"easy":"small","hard":"large"}
    }))
    .unwrap();
    (config, vec![level])
}

#[test]
fn selection_changes_problems_labels_and_timing_using_ids() {
    let (mut config, mut levels) = fixture();
    let mut next = levels[0].clone();
    next.name = "SECOND LEVEL".into();
    next.content
        .profiles_by_mode
        .insert("easy".into(), "large".into());
    levels.push(next);
    config.difficulties[0].label = "RENAMED MENU LABEL".into();
    let prepared = config.prepare_campaigns(&levels).unwrap();
    let mut easy = prepared.difficulties[0].campaign.start(42);
    let hard = prepared.difficulties[1].campaign.start(42);
    assert_eq!(easy.current_prompt(), "2 + 2 = ?");
    assert_eq!(hard.current_prompt(), "10 x 10 = ?");
    assert_eq!(easy.current_difficulty(), "INTRO");
    assert_eq!(hard.current_difficulty(), "ADVANCED");
    assert_eq!(easy.current_time_limit_frames(), 900);
    assert_eq!(hard.current_time_limit_frames(), 450);
    assert_eq!(easy.run().lives().count(), 3);
    assert_eq!(hard.run().lives().count(), 2);
    easy.submit_typed_answer("4");
    assert_eq!(easy.current_level_name(), "SECOND LEVEL");
    assert_eq!(easy.current_prompt(), "10 x 10 = ?");
    assert_eq!(easy.current_difficulty(), "ADVANCED");
}

#[test]
fn every_selectable_mapping_is_checked_before_play() {
    for (value, expected) in [
        (Some("missing"), "missing"),
        (None, "hard"),
        (Some(" "), "profile"),
    ] {
        let (config, mut levels) = fixture();
        levels[0].content.profiles_by_mode.remove("hard");
        if let Some(value) = value {
            levels[0]
                .content
                .profiles_by_mode
                .insert("hard".into(), value.into());
        }
        let error = config.prepare_campaigns(&levels).unwrap_err().to_string();
        assert!(
            error.contains("FIRST LEVEL") && error.contains(expected),
            "{error}"
        );
    }
}

#[test]
fn legacy_presets_and_unmapped_levels_keep_inline_content() {
    let (mut config, mut levels) = fixture();
    config.difficulties[0].id = None;
    let prepared = config.prepare_campaigns(&levels).unwrap();
    let legacy = prepared.difficulties[0].campaign.start(42);
    assert_eq!(legacy.current_prompt(), "1 - 1 = ?");
    assert_eq!(legacy.current_difficulty(), "LEGACY");
    assert_eq!(legacy.current_time_limit_frames(), 900);
    levels[0].content.profiles_by_mode.clear();
    let prepared = config.prepare_campaigns(&levels).unwrap();
    assert_eq!(
        prepared.difficulties[1].campaign.start(42).current_prompt(),
        "1 - 1 = ?"
    );
    config.difficulties.clear();
    assert!(
        config
            .prepare_campaigns(&levels)
            .unwrap()
            .difficulties
            .is_empty()
    );
}

fn answer_correctly(session: &mut mathgame_app::MathgameSession) {
    if let AnswerContract::MultipleChoice { options } = session.current_problem().answer_contract()
    {
        let index = options
            .iter()
            .position(|answer| answer == &session.current_problem().canonical_solution())
            .unwrap();
        assert!(session.submit_choice(index).correct);
    } else {
        assert!(
            session
                .submit_typed_answer(session.current_answer())
                .correct
        );
    }
}

#[test]
fn bundled_normal_replays_legacy_and_all_modes_complete_every_level() {
    let config = AppConfig::resolve(None).unwrap();
    let levels = resolve_levels(None).unwrap();
    let prepared = config.prepare_campaigns(&levels).unwrap();
    for level in &levels {
        let mixes: Vec<_> = ["easy", "normal", "hard"]
            .iter()
            .map(|mode| {
                &config.difficulty_profiles[&level.content.profiles_by_mode[*mode]].problems
            })
            .collect();
        assert_ne!(
            mixes[0], mixes[1],
            "{}: Easy must change the content",
            level.name
        );
        assert_ne!(
            mixes[1], mixes[2],
            "{}: Hard must change the content",
            level.name
        );
    }
    let normal = config
        .difficulties
        .iter()
        .position(|mode| mode.id.as_deref() == Some("normal"))
        .unwrap();
    let mut baseline = prepared.initial.start(99);
    let mut session = prepared.difficulties[normal].campaign.start(99);
    while session.run().phase() == ratgames::RunPhase::Playing {
        assert_eq!(session.current_problem(), baseline.current_problem());
        assert_eq!(session.current_difficulty(), baseline.current_difficulty());
        assert_eq!(
            session.current_time_limit_frames(),
            baseline.current_time_limit_frames()
        );
        answer_correctly(&mut session);
        answer_correctly(&mut baseline);
    }
    for preset in &prepared.difficulties {
        let mut session = preset.campaign.start(99);
        let mut levels_seen = std::collections::BTreeSet::new();
        while session.run().phase() == ratgames::RunPhase::Playing {
            levels_seen.insert(session.run().levels().current());
            answer_correctly(&mut session);
        }
        assert_eq!(levels_seen.len(), levels.len(), "{}", preset.label);
        assert_eq!(session.run().phase(), ratgames::RunPhase::Won);
    }
}

#[test]
fn override_file_applies_and_bad_references_fail_at_startup() {
    let root =
        std::env::temp_dir().join(format!("mathgame-profile-override-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("config.json");
    let result = std::panic::catch_unwind(|| {
        // Exercise the actual --config parser and loader, using bundled level mappings.
        let mut json: Value = serde_json::from_str(include_str!("profiles.json")).unwrap();
        json["difficulties"] = serde_json::from_str::<Value>(include_str!("economy.json")).unwrap()
            ["difficulties"]
            .clone();
        let levels = resolve_levels(None).unwrap();
        let profile_id = levels[0].content.profiles_by_mode["easy"].clone();
        json["difficulty_profiles"][&profile_id] =
            json!({"label":"CUSTOM", "problems":[{"operator":"add","min":7,"max":7}]});
        std::fs::write(&path, json.to_string()).unwrap();
        let (path, _) = ratgames::parse_config_flag([
            "--config".to_string(),
            path.to_str().unwrap().to_string(),
        ])
        .unwrap();
        let config = AppConfig::resolve(path).unwrap();
        let prepared = config.prepare_campaigns(&levels).unwrap();
        assert_eq!(
            prepared.difficulties[0].campaign.start(12).current_prompt(),
            "7 + 7 = ?"
        );
        json["difficulty_profiles"]
            .as_object_mut()
            .unwrap()
            .remove(&profile_id);
        std::fs::write(root.join("config.json"), json.to_string()).unwrap();
        let config = AppConfig::resolve(Some(root.join("config.json"))).unwrap();
        let error = config.prepare_campaigns(&levels).unwrap_err().to_string();
        assert!(error.contains(&profile_id), "{error}");

        // An older config has neither IDs nor profiles. Even with the new
        // bundled level mappings it keeps the original arithmetic sequence.
        json.as_object_mut().unwrap().remove("difficulty_profiles");
        for preset in json["difficulties"].as_array_mut().unwrap() {
            preset.as_object_mut().unwrap().remove("id");
        }
        std::fs::write(root.join("config.json"), json.to_string()).unwrap();
        let config = AppConfig::resolve(Some(root.join("config.json"))).unwrap();
        let prepared = config.prepare_campaigns(&levels).unwrap();
        for preset in prepared.difficulties {
            assert_eq!(
                preset.campaign.start(12).current_problem(),
                prepared.initial.start(12).current_problem()
            );
        }

        // An older --levels pack has no mappings. New bundled presets must
        // continue to play its inline content, including its difficulty label.
        let mut level: Value = serde_json::from_str(include_str!("levels/level_0.json")).unwrap();
        level.as_object_mut().unwrap().remove("profiles_by_mode");
        std::fs::write(root.join("level_0.json"), level.to_string()).unwrap();
        let custom_levels = resolve_levels(Some(root.clone())).unwrap();
        let prepared = AppConfig::resolve(None)
            .unwrap()
            .prepare_campaigns(&custom_levels)
            .unwrap();
        for preset in prepared.difficulties {
            assert_eq!(
                preset.campaign.start(12).current_problem(),
                prepared.initial.start(12).current_problem()
            );
            assert_eq!(
                preset.campaign.start(12).current_difficulty(),
                custom_levels[0].difficulty
            );
        }
    });
    std::fs::remove_dir_all(root).unwrap();
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

#[test]
fn only_the_new_profile_object_rejects_unknown_fields() {
    let error = serde_json::from_value::<AppConfig>(json!({
        "difficulty_profiles": {"intro": {
            "label":"INTRO", "problems":[{"operator":"add","min":1,"max":2}],
            "lable":"typo"
        }}
    }))
    .unwrap_err()
    .to_string();
    assert!(error.contains("lable"), "{error}");
    let legacy: AppConfig = serde_json::from_value(json!({"future_field":true})).unwrap();
    assert!(legacy.validate().is_ok());
}

#[test]
fn explicit_ranges_constrain_many_seeded_problems() {
    let (mut config, levels) = fixture();
    let spec = &mut config
        .difficulty_profiles
        .get_mut("large")
        .unwrap()
        .problems[0];
    spec.operator = mathgame_app::OperatorConfig::Add;
    spec.min = 30;
    spec.max = 39;
    let prepared = config.prepare_campaigns(&levels).unwrap();
    for seed in 0..256 {
        let session = prepared.difficulties[1].campaign.start(seed);
        let Prompt::Equation(equation) = session.current_problem().prompt() else {
            panic!("equation");
        };
        for operand in [equation.lhs(), equation.rhs()] {
            let value: i64 = operand.to_fraction_string().parse().unwrap();
            assert!((30..=39).contains(&value), "{value}");
        }
    }
}
