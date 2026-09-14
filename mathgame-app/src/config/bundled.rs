//! Compile-time composition of the bundled app configuration.
use super::AppConfig;
use std::sync::LazyLock;

/// The bundled default, composed from per-domain files under `config/` and parsed
/// once. `engine.json` holds the ratgames engine config; `style.json` the visual
/// style (text scale/shadow, banner glyphs, feedback, timer bar); `economy.json`
/// the run economy and pacing (lives, scoring, ranks, continues, attract,
/// interstitials, difficulties); `profiles.json` the named problem mixes;
/// `copy.json` every user-facing string (under the
/// `copy` key); `layout.json` every on-screen position (under the `layout` key).
/// Every root key is authored in exactly one file — a collision panics rather
/// than letting file order decide. The merged object deserialises into one
/// [`AppConfig`]. A malformed bundle is caught by the unit tests below (a
/// build-time guarantee), not left as a runtime risk.
pub(super) static BUNDLED: LazyLock<AppConfig> = LazyLock::new(|| {
    let mut root = serde_json::Map::new();
    merge_domain(&mut root, "engine.json", include_str!("engine.json"));
    merge_domain(&mut root, "style.json", include_str!("style.json"));
    merge_domain(&mut root, "economy.json", include_str!("economy.json"));
    merge_domain(&mut root, "profiles.json", include_str!("profiles.json"));
    insert_domain_key(
        &mut root,
        "copy",
        bundled_json(include_str!("copy.json"), "copy.json"),
    );
    insert_domain_key(
        &mut root,
        "layout",
        bundled_json(include_str!("layout.json"), "layout.json"),
    );
    serde_json::from_value(serde_json::Value::Object(root))
        .expect("bundled config must deserialise into AppConfig")
});

/// Merge a root-spanning per-domain file (its top-level keys become root config
/// keys) into the bundle, panicking if a key was already supplied by an earlier
/// file — a build-time guarantee, like the parses themselves.
fn merge_domain(root: &mut serde_json::Map<String, serde_json::Value>, name: &str, text: &str) {
    let serde_json::Value::Object(map) = bundled_json(text, name) else {
        panic!("bundled config/{name} must be a JSON object");
    };
    for (key, value) in map {
        insert_domain_key(root, &key, value);
    }
}

/// Insert one root config key, panicking on a duplicate across domain files.
fn insert_domain_key(
    root: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: serde_json::Value,
) {
    assert!(
        root.insert(key.to_string(), value).is_none(),
        "bundled config key {key:?} is supplied by more than one per-domain file"
    );
}

/// Parse a bundled per-domain config file, panicking on a malformed bundle — a
/// build-time guarantee, since these are `include_str!`'d at compile time.
fn bundled_json(text: &str, name: &str) -> serde_json::Value {
    serde_json::from_str(text)
        .unwrap_or_else(|_| panic!("bundled config/{name} must be valid JSON"))
}
