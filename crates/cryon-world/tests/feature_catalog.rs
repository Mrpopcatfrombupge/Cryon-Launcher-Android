use cryon_world::worldgen::{feature_catalog::FeatureCatalog, tags::TagDefinitions};
use std::collections::BTreeMap;
fn map(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}
#[test]
fn canonical_duplicates_cycles_and_unknown_unselected_children_fail() {
    let tags = TagDefinitions::default();
    assert!(
        FeatureCatalog::from_resources(
            &map(&[
                ("a", r#"{"type":"no_op"}"#),
                ("minecraft:a", r#"{"type":"no_op"}"#)
            ]),
            &BTreeMap::new(),
            &tags
        )
        .is_err()
    );
    let features = map(&[
        (
            "cryon:loop",
            r#"{"type":"sequence","features":["cryon:loop"]}"#,
        ),
        ("cryon:unused", r#"{"type":"not_implemented"}"#),
    ]);
    let placed = map(&[(
        "cryon:loop",
        r#"{"feature":"cryon:loop","placement":[]}"#,
    )]);
    let catalog = FeatureCatalog::from_resources(&features, &placed, &tags).unwrap();
    assert!(catalog.feature(r#""cryon:loop""#).is_err());
    assert!(catalog.placed_feature(r#""cryon:loop""#).is_err());
    assert!(catalog.feature(r#"{"type":"no_op"}"#).is_ok());
    assert!(catalog.feature(r#"{"type":"weighted_random_selector","features":[{"data":"cryon:missing","weight":0}]}"#).is_err());
}
#[test]
fn shared_reference_expansion_is_bounded_before_building_a_large_tree() {
    let leaf = r#"{"feature":{"type":"no_op"},"placement":[]}"#;
    let branch = format!(
        r#"{{"type":"sequence","features":[{}]}}"#,
        vec![r#""cryon:leaf""#; 8192].join(",")
    );
    let features = BTreeMap::from([("cryon:branch".into(), branch)]);
    let placed = map(&[
        ("cryon:leaf", leaf),
        (
            "cryon:branch",
            r#"{"feature":"cryon:branch","placement":[]}"#,
        ),
    ]);
    let catalog =
        FeatureCatalog::from_resources(&features, &placed, &TagDefinitions::default()).unwrap();
    let root = format!(
        r#"{{"type":"sequence","features":[{}]}}"#,
        [r#""cryon:branch""#; 9].join(",")
    );
    assert!(catalog.feature(&root).is_err());
}
