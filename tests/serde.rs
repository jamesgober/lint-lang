//! The `serde` feature: `Level` (de)serializes as its lowercase name.

#![cfg(feature = "serde")]
#![allow(clippy::unwrap_used)] // test assertions on known-good JSON

use std::collections::BTreeMap;

use lint_lang::Level;

#[test]
fn test_level_serializes_as_lowercase_name() {
    assert_eq!(serde_json::to_string(&Level::Allow).unwrap(), "\"allow\"");
    assert_eq!(serde_json::to_string(&Level::Warn).unwrap(), "\"warn\"");
    assert_eq!(serde_json::to_string(&Level::Deny).unwrap(), "\"deny\"");
}

#[test]
fn test_level_round_trips() {
    for level in [Level::Allow, Level::Warn, Level::Deny] {
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(serde_json::from_str::<Level>(&json).unwrap(), level);
    }
}

#[test]
fn test_unknown_level_is_rejected() {
    assert!(serde_json::from_str::<Level>("\"loud\"").is_err());
    assert!(serde_json::from_str::<Level>("\"Warn\"").is_err());
}

#[test]
fn test_config_map_deserializes() {
    let config: BTreeMap<String, Level> =
        serde_json::from_str(r#"{ "double-negation": "deny", "identity-op": "allow" }"#).unwrap();
    assert_eq!(config["double-negation"], Level::Deny);
    assert_eq!(config["identity-op"], Level::Allow);
}
