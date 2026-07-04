//! Unit tests for the config module.

use super::*;
use crate::color::Color;
use crate::geometry::{Point, Size};
use crate::theme::Theme;

#[test]
fn panel_sits_in_the_bottom_fraction() {
    let cfg = InputConfig::default();
    let l = cfg.layout(Size::new(1000, 1000));
    assert_eq!(l.panel.size, Size::new(1000, 150)); // 15%
    assert_eq!(l.panel.origin, Point::new(0, 850));
}

#[test]
fn nested_borders_match_configured_count_and_nest_inward() {
    let cfg = InputConfig::default();
    let l = cfg.layout(Size::new(800, 600));
    assert_eq!(l.borders.len(), cfg.border.line_count as usize);
    // Each successive border is strictly inside the previous.
    for pair in l.borders.windows(2) {
        assert!(pair[1].origin.x > pair[0].origin.x);
        assert!(pair[1].size.w < pair[0].size.w);
    }
    // Text area is inside the innermost border.
    let inner = l.borders.last().unwrap();
    assert!(l.text_area.origin.x >= inner.origin.x);
}

#[test]
fn defaults_hold_no_magic_numbers_downstream() {
    // The one place 20px / 0.15 / light-blue live is here.
    let cfg = InputConfig::default();
    assert!((cfg.font.size_px - 20.0).abs() < f32::EPSILON);
    assert!((cfg.height_fraction - 0.15).abs() < f32::EPSILON);
    assert_eq!(cfg.border.line_count, 2);
}

#[test]
fn defaults_round_trip_through_toml() {
    let cfg = Config::default();
    let text = toml::to_string(&cfg).expect("serialize");
    let parsed: Config = toml::from_str(&text).expect("deserialize");
    assert_eq!(parsed, cfg);
}

#[test]
fn a_partial_file_falls_back_to_defaults() {
    let parsed: Config = toml::from_str("[window]\ntitle = \"custom\"\n").expect("parse");
    assert_eq!(parsed.window.title, "custom");
    assert_eq!(parsed.window.width, WindowConfig::default().width);
    assert_eq!(parsed.screen.size, ScreenConfig::default().size);
    assert_eq!(
        parsed.marquee.text_scale,
        MarqueeConfig::default().text_scale
    );
}

#[test]
fn component_colours_default_from_the_theme() {
    let theme = Theme::default();
    let cfg = Config::default();
    assert_eq!(cfg.screen.backdrop, theme.background);
    assert_eq!(cfg.input.border.color, theme.accent);
    assert_eq!(cfg.input.text_color, theme.ink);
    assert_eq!(cfg.input.background_color, theme.panel);
}

#[test]
fn a_colour_can_be_overridden_in_toml() {
    let parsed: Config = toml::from_str("[input.border]\ncolor = \"#FF0000\"\n").expect("parse");
    assert_eq!(parsed.input.border.color, Color::rgb(0xFF, 0, 0));
    // A colour the file left alone keeps its theme default.
    assert_eq!(parsed.input.text_color, Theme::default().ink);
}

#[test]
fn validate_rejects_out_of_range_height_fraction() {
    let mut cfg = Config::default();
    cfg.input.height_fraction = 1.5;
    assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
    cfg.input.height_fraction = 0.0;
    assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
    cfg.input.height_fraction = 0.15;
    assert!(cfg.validate().is_ok());
}

#[test]
fn validate_rejects_zero_dimensions() {
    let mut cfg = Config::default();
    cfg.screen.size = Size::new(0, 240);
    assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
}

#[test]
fn load_missing_file_is_an_io_error() {
    let err = Config::load("/no/such/ratgames-config.toml").unwrap_err();
    assert!(matches!(err, ConfigError::Io { .. }));
}

#[test]
fn raster_glyph_source_round_trips_through_toml() {
    let raster = GlyphSourceConfig::Raster {
        cell_px: 24,
        threshold: 128,
        font: FontSource::default(),
    };
    let marquee = MarqueeConfig {
        glyph_source: raster.clone(),
        ..MarqueeConfig::default()
    };
    let text = toml::to_string(&marquee).expect("serialize");
    let parsed: MarqueeConfig = toml::from_str(&text).expect("deserialize");
    assert_eq!(parsed.glyph_source, raster);
}

#[test]
fn raster_threshold_defaults_to_128_when_omitted() {
    // A raster source declared without an explicit threshold falls back to
    // 128, so existing configs keep the current look.
    let parsed: GlyphSourceConfig = toml::from_str(
        "kind = \"raster\"\ncell_px = 24\n[font]\nkind = \"system\"\nfamily = \"Menlo\"\n",
    )
    .expect("parse");
    match parsed {
        GlyphSourceConfig::Raster { threshold, .. } => assert_eq!(threshold, 128),
        other => panic!("expected a raster source, got {other:?}"),
    }
}

#[test]
fn raster_threshold_round_trips_through_toml() {
    let raster = GlyphSourceConfig::Raster {
        cell_px: 24,
        threshold: 200,
        font: FontSource::default(),
    };
    let marquee = MarqueeConfig {
        glyph_source: raster.clone(),
        ..MarqueeConfig::default()
    };
    let text = toml::to_string(&marquee).expect("serialize");
    let parsed: MarqueeConfig = toml::from_str(&text).expect("deserialize");
    assert_eq!(parsed.glyph_source, raster);
}

#[test]
fn validate_rejects_zero_text_scale() {
    let mut cfg = Config::default();
    cfg.marquee.text_scale = 0;
    assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
}

#[test]
fn validate_rejects_zero_raster_cell_px() {
    let mut cfg = Config::default();
    cfg.marquee.glyph_source = GlyphSourceConfig::Raster {
        cell_px: 0,
        threshold: 128,
        font: FontSource::default(),
    };
    assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
}

#[test]
fn validate_rejects_oversized_raster_cell_px() {
    let mut cfg = Config::default();
    cfg.marquee.glyph_source = GlyphSourceConfig::Raster {
        cell_px: 10_000, // beyond the private safety ceiling
        threshold: 128,
        font: FontSource::default(),
    };
    assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
}

#[test]
fn validate_accepts_a_reasonable_raster_marquee() {
    let mut cfg = Config::default();
    cfg.marquee.glyph_source = GlyphSourceConfig::Raster {
        cell_px: 32,
        threshold: 160,
        font: FontSource::default(),
    };
    assert!(cfg.validate().is_ok());
}

#[test]
fn config_source_is_a_file_when_a_path_is_given() {
    assert_eq!(
        ConfigSource::resolve(Some(PathBuf::from("/game.toml"))),
        ConfigSource::File(PathBuf::from("/game.toml"))
    );
}

#[test]
fn config_source_defaults_without_a_path() {
    assert_eq!(ConfigSource::resolve(None), ConfigSource::Default);
}

#[test]
fn config_source_load_or_else_uses_the_supplied_default() {
    // The Default source defers to the caller's preset, not Config::default.
    let cfg = ConfigSource::Default
        .load_or_else(|| {
            let mut c = Config::default();
            c.marquee.text_scale = 9;
            c
        })
        .expect("default builds");
    assert_eq!(cfg.marquee.text_scale, 9);
}

#[test]
fn config_source_default_loads_builtin_defaults() {
    let cfg = ConfigSource::Default.load().expect("defaults load");
    assert_eq!(cfg, Config::default());
}

#[test]
fn parse_config_flag_extracts_path_and_keeps_positionals() {
    let (path, positionals) =
        parse_config_flag(["--config", "banner.toml", "HELLO"].map(String::from)).expect("parses");
    assert_eq!(path, Some(PathBuf::from("banner.toml")));
    assert_eq!(positionals, vec!["HELLO".to_string()]);
}

#[test]
fn parse_config_flag_supports_equals_form() {
    let (path, positionals) = parse_config_flag(["--config=x.toml".to_string()]).expect("parses");
    assert_eq!(path, Some(PathBuf::from("x.toml")));
    assert!(positionals.is_empty());
}

#[test]
fn parse_config_flag_without_flag_is_all_positional() {
    // Preserves `cargo run -- "GAME OVER"`: a bare positional is banner text.
    let (path, positionals) = parse_config_flag(["GAME OVER".to_string()]).expect("parses");
    assert_eq!(path, None);
    assert_eq!(positionals, vec!["GAME OVER".to_string()]);
}

#[test]
fn parse_config_flag_missing_value_is_an_error() {
    let err = parse_config_flag(["--config".to_string()]).unwrap_err();
    assert!(matches!(err, ConfigError::Invalid(_)));
}

#[test]
fn sample_marquee_toml_loads_and_validates() {
    // The shipped TOML sample parses, validates, and selects a raster source.
    let cfg = Config::load("examples/marquee.toml").expect("toml sample loads");
    assert!(matches!(
        cfg.marquee.glyph_source,
        GlyphSourceConfig::Raster { .. }
    ));
}

#[test]
fn sample_marquee_json_loads_and_validates() {
    // The JSON sample loads via the same Config::load (dispatched by
    // extension) and selects the same raster source.
    let cfg = Config::load("examples/marquee.json").expect("json sample loads");
    assert!(matches!(
        cfg.marquee.glyph_source,
        GlyphSourceConfig::Raster { .. }
    ));
}

#[test]
fn load_rejects_an_unsupported_extension() {
    // The extension is checked before the file is read, so even a missing path
    // is rejected as an unsupported format rather than an IO error.
    let err = Config::load("game.yaml").unwrap_err();
    assert!(matches!(err, ConfigError::Invalid(_)));
}

#[test]
fn defaults_round_trip_through_json() {
    let cfg = Config::default();
    let text = serde_json::to_string(&cfg).expect("serialize");
    let parsed: Config = serde_json::from_str(&text).expect("deserialize");
    assert_eq!(parsed, cfg);
}

#[test]
fn oversized_banner_is_rejected_before_allocation() {
    // A bitmap banner at an extreme scale exceeds the scaled-pixel ceiling
    // and is rejected without allocating — deterministic, no system font.
    let marquee = MarqueeConfig {
        text_scale: 256,
        ..MarqueeConfig::default()
    };
    assert!(matches!(
        marquee.text_sprite("GAME OVER"),
        Err(ConfigError::SpriteTooLarge { .. })
    ));
}

#[test]
fn a_reasonable_banner_builds_within_limits() {
    let marquee = MarqueeConfig {
        text_scale: 4,
        ..MarqueeConfig::default()
    };
    assert!(marquee.text_sprite("OK").is_ok());
}

#[test]
#[ignore = "requires a system font; run with `cargo test -- --ignored`"]
fn sample_marquee_bakes_through_the_raster_source() {
    // End-to-end: a file-selected raster source bakes a real, non-empty banner
    // (needs the sample's Menlo font).
    let cfg = Config::load("examples/marquee.toml").expect("sample loads");
    assert!(matches!(
        cfg.marquee.glyph_source,
        GlyphSourceConfig::Raster { .. }
    ));
    let sprite = cfg
        .marquee
        .text_sprite("HELLO")
        .expect("raster banner bakes");
    assert!(sprite.size().area() > 0, "raster banner is non-empty");
}

#[test]
fn builder_rejects_oversized_cell_px_without_a_font() {
    // The builder validates the glyph source before resolving a font, so an
    // oversized raster cell_px is rejected deterministically — no system font
    // loaded, no giant allocation attempted.
    let marquee = MarqueeConfig {
        glyph_source: GlyphSourceConfig::Raster {
            cell_px: 5000,
            threshold: 128,
            font: FontSource::default(),
        },
        ..MarqueeConfig::default()
    };
    assert!(matches!(
        marquee.text_sprite("HI"),
        Err(ConfigError::Invalid(_))
    ));
}

#[test]
fn system_source_parses_named_weight_style_and_stretch() {
    // The full styling vocabulary parses off a system source.
    let src: FontSource = toml::from_str(
        "kind = \"system\"\n\
             family = \"Helvetica Neue\"\n\
             weight = \"bold\"\n\
             style = \"italic\"\n\
             stretch = \"condensed\"\n",
    )
    .expect("parse");
    assert_eq!(
        src,
        FontSource::System {
            family: FontFamily::Named("Helvetica Neue".to_string()),
            weight: FontWeight(700),
            style: FontStyle::Italic,
            stretch: FontStretch::Condensed,
        }
    );
}

#[test]
fn system_source_accepts_a_numeric_weight() {
    // A raw number is an alternative spelling of the same weight.
    let src: FontSource = toml::from_str("kind = \"system\"\nweight = 700\n").expect("parse");
    match src {
        FontSource::System { weight, .. } => assert_eq!(weight, FontWeight(700)),
        other => panic!("expected a system source, got {other:?}"),
    }
}

#[test]
fn system_source_defaults_every_field_when_omitted() {
    // Backward compatibility: a bare system source still parses, with an
    // unnamed (generic-monospace) family and every style knob at Normal.
    let src: FontSource = toml::from_str("kind = \"system\"\n").expect("parse");
    assert_eq!(
        src,
        FontSource::System {
            family: FontFamily::Default,
            weight: FontWeight::default(),
            style: FontStyle::default(),
            stretch: FontStretch::default(),
        }
    );
    assert_eq!(FontWeight::default(), FontWeight(400));
    assert_eq!(FontStyle::default(), FontStyle::Normal);
    assert_eq!(FontStretch::default(), FontStretch::Normal);
}

#[test]
fn font_weight_rejects_an_unknown_name() {
    let parsed = toml::from_str::<FontSource>("kind = \"system\"\nweight = \"heavy\"\n");
    assert!(parsed.is_err(), "an unknown weight name must not parse");
}

#[test]
fn font_weight_rejects_out_of_range_numbers() {
    for bad in ["weight = 0", "weight = 2000"] {
        let text = format!("kind = \"system\"\n{bad}\n");
        assert!(
            toml::from_str::<FontSource>(&text).is_err(),
            "out-of-range {bad:?} must not parse",
        );
    }
}

#[test]
fn named_weight_round_trips_through_its_name() {
    // A standard step serialises back to its name and re-parses unchanged.
    let src = FontSource::System {
        family: FontFamily::Default,
        weight: FontWeight(700),
        style: FontStyle::Oblique,
        stretch: FontStretch::Expanded,
    };
    let text = toml::to_string(&src).expect("serialize");
    assert!(
        text.contains("weight = \"bold\""),
        "a standard weight serialises by name: {text}"
    );
    let parsed: FontSource = toml::from_str(&text).expect("deserialize");
    assert_eq!(parsed, src);
}

#[test]
fn nonstandard_weight_round_trips_as_a_number() {
    // A value between the standard steps has no name, so it serialises as the
    // number and still round-trips (here through JSON, for the web path).
    let src = FontSource::System {
        family: FontFamily::Default,
        weight: FontWeight(650),
        style: FontStyle::Normal,
        stretch: FontStretch::SemiCondensed,
    };
    let text = serde_json::to_string(&src).expect("serialize");
    assert!(
        text.contains("\"weight\":650"),
        "a nonstandard weight serialises as a number: {text}"
    );
    let parsed: FontSource = serde_json::from_str(&text).expect("deserialize");
    assert_eq!(parsed, src);
}

#[test]
fn font_family_default_and_named_forms() {
    // Omitted family and the reserved "default" both mean the generic
    // monospace; any other string names a specific family.
    let family = |src: &str| match toml::from_str::<FontSource>(src).expect("parse") {
        FontSource::System { family, .. } => family,
        other => panic!("expected a system source, got {other:?}"),
    };
    assert_eq!(family("kind = \"system\"\n"), FontFamily::Default);
    assert_eq!(
        family("kind = \"system\"\nfamily = \"default\"\n"),
        FontFamily::Default
    );
    assert_eq!(
        family("kind = \"system\"\nfamily = \"Menlo\"\n"),
        FontFamily::Named("Menlo".to_string())
    );
}

#[test]
fn default_family_round_trips_as_the_keyword() {
    // The default family is a real font, so it serialises as the explicit
    // "default" keyword (not omitted) and re-parses unchanged.
    let src = FontSource::default();
    let text = toml::to_string(&src).expect("serialize");
    assert!(
        text.contains("family = \"default\""),
        "the default family serialises as the reserved keyword: {text}"
    );
    let parsed: FontSource = toml::from_str(&text).expect("deserialize");
    assert_eq!(parsed, src);
}

#[test]
fn default_strings_are_sourced_from_the_bundled_json() {
    // The window title lives in config/defaults.json; Config pulls it in via
    // Default. This pins the wiring and forces the bundle to parse.
    assert_eq!(WindowConfig::default().title, "ratgames");
}
