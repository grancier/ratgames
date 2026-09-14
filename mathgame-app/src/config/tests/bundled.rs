use super::super::*;
use ratgames::{
    AttractConfig, BannerStyle, CountdownConfig, FeedbackBeatConfig, FontFamily, FontSource,
    FontWeight, ScoresConfig, Size,
};

#[test]
fn bundled_default_selects_the_product_structure() {
    // The bundled JSON is the source of truth for the product look, not a Rust
    // literal. Pin only the structural choices — the faces (Menlo Bold, input
    // and banners), the screen geometry, the banner glyph source and its
    // resolution, where scores persist. The continuous scalars (scales,
    // offsets, sizes, thresholds, colours, frame counts) are live-tuned in
    // the per-domain files (mostly style.json) while the window runs;
    // exact-value pins broke this test on every tuning pass, and `resolve()`
    // already validates their invariants.
    let config = AppConfig::resolve(None).expect("bundled config must be valid");
    match config.engine.input.font.source {
        FontSource::System {
            family: FontFamily::Named(name),
            weight,
            ..
        } => {
            assert_eq!(name, "Menlo");
            assert_eq!(weight, FontWeight(700));
        }
        other => panic!("expected a named system font, got {other:?}"),
    }
    assert_eq!(config.engine.screen.size, Size::new(640, 360));
    match &config.banner_glyphs {
        GlyphSourceConfig::Raster { cell_px, font, .. } => {
            // The display-text height: at banner_scale 1, cell_px IS the
            // banner height, rasterised at full resolution for that size.
            assert_eq!(*cell_px, 64);
            match font {
                FontSource::System {
                    family: FontFamily::Named(name),
                    weight,
                    ..
                } => {
                    assert_eq!(name, "Menlo");
                    assert_eq!(*weight, FontWeight(700));
                }
                other => panic!("expected a Menlo raster font, got {other:?}"),
            }
        }
        other => panic!("expected a 64px raster banner source, got {other:?}"),
    }
    // The body-text source: half the banner height, so the HUD / lists /
    // board rows render at the full resolution their height allows too.
    match config
        .hud_glyphs
        .as_ref()
        .expect("a hud glyph source is shipped")
    {
        GlyphSourceConfig::Raster { cell_px, font, .. } => {
            assert_eq!(*cell_px, 32);
            match font {
                FontSource::System {
                    family: FontFamily::Named(name),
                    ..
                } => assert_eq!(name, "Menlo"),
                other => panic!("expected a Menlo raster font, got {other:?}"),
            }
        }
        other => panic!("expected a 32px raster hud source, got {other:?}"),
    }
    // The neutral Default shares the banner source (no second source).
    assert!(AppConfig::default().hud_glyphs.is_none());
    assert_eq!(config.scores.capacity, 10);
    assert_eq!(
        config.scores.file,
        std::path::PathBuf::from("mathgame-highscores.json")
    );
    // Run-wide starting lives are shipped game *design* — not a live-tuned
    // visual knob like the scales/offsets/colours above — so pin them. The
    // per-level rules live in the level files, pinned by the test below.
    assert_eq!(config.starting_lives, 3);
}

#[test]
fn bundled_domain_files_hold_disjoint_keys() {
    // Every root config key is authored in exactly one per-domain file; a
    // duplicate would make the bundle order-dependent. The loader already
    // panics on a collision — this pins the authored bundle as clean with a
    // readable failure instead of a poisoned LazyLock.
    let domains = [
        ("engine.json", include_str!("../engine.json")),
        ("style.json", include_str!("../style.json")),
        ("economy.json", include_str!("../economy.json")),
        ("profiles.json", include_str!("../profiles.json")),
    ];
    let mut seen: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    for (name, text) in domains {
        let value: serde_json::Value = serde_json::from_str(text).expect(name);
        let object = value.as_object().expect("domain file must be an object");
        for key in object.keys() {
            assert!(
                !matches!(key.as_str(), "copy" | "layout"),
                "{name} must not supply the whole-file domain key {key:?}"
            );
            if let Some(previous) = seen.insert(key.clone(), name) {
                panic!("config key {key:?} appears in both {previous} and {name}");
            }
        }
    }
}

#[test]
fn bundled_ranks_name_the_shipped_endings() {
    // The rank table is shipped game design: pin its shape — the two win
    // endings, proudest first, every rule win-gated so a game over keeps its
    // plain title. The failure/point thresholds stay tunable.
    let config = AppConfig::resolve(None).expect("bundled config");
    let titles: Vec<_> = config
        .ranks
        .rules
        .iter()
        .map(|rule| rule.title.as_str())
        .collect();
    assert_eq!(titles, vec!["NO MISS CHAMP", "MATH MASTER"]);
    assert!(
        config.ranks.rules.iter().all(|rule| rule.requires_won),
        "a lost run keeps the plain GAME OVER title"
    );
}

#[test]
fn bundled_continues_offer_one_score_keeping_continue() {
    // Shipped game design: one continue that keeps the score, prompted for
    // ten seconds at 60fps. The counts stay tunable; pin that a continue is
    // offered and the prompt has a real hold.
    let config = AppConfig::resolve(None).expect("bundled config");
    assert!(config.continues.allowed >= 1);
    assert!(config.continue_prompt.frames >= 1);
}

#[test]
fn bundled_attract_mode_is_on_with_a_real_rotation() {
    // Shipped game design: the title idles into an attract rotation. The
    // frame counts stay tunable; pin that both holds are real.
    let config = AppConfig::resolve(None).expect("bundled config");
    assert!(config.attract.idle.frames > 0, "attract mode is on");
    assert!(config.attract.card.frames > 0);
    assert!(config.attract.idle_countdown().is_some());
}

#[test]
fn attract_mode_defaults_off_and_rejects_a_rotation_with_no_hold() {
    // The Rust default keeps attract off (no idle trigger)...
    let config = AppConfig::default();
    assert!(config.attract.idle_countdown().is_none());
    assert!(config.validate().is_ok());

    // ...and turning it on with a zero card hold is a config error (the
    // rotation would thrash every frame).
    let broken = AppConfig {
        attract: AttractConfig {
            idle: CountdownConfig { frames: 600 },
            card: CountdownConfig { frames: 0 },
        },
        ..AppConfig::default()
    };
    assert!(matches!(broken.validate(), Err(AppConfigError::Invalid(_))));
}

#[test]
fn rust_default_stays_generic_monospace() {
    // The Rust `Default` is only the serde fallback for omitted fields; the
    // named face lives in the bundled data, never in a Rust literal.
    assert_eq!(
        AppConfig::default().engine.input.font.source,
        FontSource::default()
    );
}

#[test]
fn zero_banner_scale_is_rejected() {
    let config = AppConfig {
        text: BannerStyle {
            banner_scale: 0,
            ..BannerStyle::default()
        },
        ..AppConfig::default()
    };
    assert!(matches!(config.validate(), Err(AppConfigError::Invalid(_))));
}

#[test]
fn zero_scores_capacity_is_rejected() {
    let config = AppConfig {
        scores: ScoresConfig {
            capacity: 0,
            ..ScoresConfig::default()
        },
        ..AppConfig::default()
    };
    assert!(matches!(config.validate(), Err(AppConfigError::Invalid(_))));
}

#[test]
fn zero_feedback_duration_is_rejected() {
    let config = AppConfig {
        feedback: FeedbackBeatConfig {
            duration_frames: 0,
            ..FeedbackBeatConfig::default()
        },
        ..AppConfig::default()
    };
    assert!(matches!(config.validate(), Err(AppConfigError::Invalid(_))));
}

#[test]
fn bundled_copy_supplies_the_shipped_strings() {
    // Copy is product design authored in copy.json; pin a couple of anchors so
    // the per-domain merge stays wired and the strings are present (not the
    // neutral Default). The exact wording stays freely editable.
    let config = AppConfig::resolve(None).expect("bundled config");
    assert_eq!(config.copy.title, "MATH GAME");
    assert_eq!(config.copy.verdict.correct, "CORRECT");
    assert_eq!(config.copy.hud, "SCORE {}  LIVES {}  L{}");
    assert_eq!(config.copy.howto.lines.len(), 4);
    // The neutral Default is genuinely blank, so the merge is doing the work.
    assert!(CopyConfig::default().title.is_empty());
}

#[test]
fn bundled_layout_supplies_the_shipped_positions() {
    // Layout is authored in layout.json; pin a couple of anchors so the
    // per-domain merge stays wired and the positions are present (not the
    // neutral Default). The exact coordinates stay freely tunable.
    let config = AppConfig::resolve(None).expect("bundled config");
    assert_eq!(config.layout.board.name_width, 5);
    assert_eq!(config.layout.timer_bar.size, Size::new(560, 12));
    // The shipped look turns the question clock's digital readout on (its
    // exact anchor stays freely tunable; deleting the key hides it).
    assert!(config.layout.timer_seconds_at.is_some());
    assert_eq!(config.layout.level_intro_ys.len(), 3);
    assert_eq!(config.layout.level_clear_ys.len(), 4);
    // The neutral Default is genuinely zeroed, so the merge is doing the work.
    assert_eq!(LayoutConfig::default().screen_x, 0);
    assert_eq!(LayoutConfig::default().board.name_width, 0);
    assert_eq!(LayoutConfig::default().timer_seconds_at, None);
}
