//! What the crate is for: one build carrying several Minecraft versions at once.
//!
//! The fixture store beside these tests holds two made-up versions whose ids deliberately disagree, so the
//! translation paths are checked without downloading a game. When `CRYON_VERSIONS` points at a real version
//! store, the same questions are asked of the real releases it holds.

use std::path::{Path, PathBuf};

use cryon_versions::{
    Direction, IdDrift, Negotiation, ProfileBundle, State, VersionCatalog, VersionProfile,
    packet_drift, registry_drift, shared_registries, translate_id, translate_packet,
};

fn fixture(id: &str) -> VersionProfile {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/store")
        .join(id);
    VersionProfile::load(&directory).expect("the fixture version reads")
}

fn catalog() -> VersionCatalog {
    let mut catalog = VersionCatalog::new();
    catalog.add(fixture("0.1-test")).unwrap();
    catalog.add(fixture("0.2-test")).unwrap();
    catalog
}

fn real_store() -> Option<PathBuf> {
    let store = std::env::var_os("CRYON_VERSIONS").map(PathBuf::from);
    store.filter(|path| path.is_dir())
}

#[test]
fn one_build_answers_either_protocol() {
    let catalog = catalog();
    assert_eq!(catalog.len(), 2);
    assert_eq!(catalog.negotiate(101).unwrap().id(), "0.1-test");
    assert_eq!(catalog.negotiate(102).unwrap().id(), "0.2-test");
    let error = catalog.negotiate(575).unwrap_err();
    assert!(
        matches!(&error, Negotiation::Unsupported { protocol: 575, supported } if supported == &vec![101, 102]),
        "{error:?}"
    );
    assert!(!catalog.supports(575));
    assert_eq!(
        catalog.newest_release().unwrap().id(),
        "0.1-test",
        "only 0.1 is a release"
    );
}

#[test]
fn each_version_keeps_its_own_packet_ids() {
    let catalog = catalog();
    let (old, new) = (
        catalog.get("0.1-test").unwrap(),
        catalog.get("0.2-test").unwrap(),
    );
    assert_eq!(
        old.packet_id(
            State::Play,
            Direction::Clientbound,
            "minecraft:spawn_entity"
        ),
        Some(2)
    );
    assert_eq!(
        new.packet_id(
            State::Play,
            Direction::Clientbound,
            "minecraft:spawn_entity"
        ),
        Some(6)
    );
    assert_eq!(
        old.packet_id(State::Play, Direction::Serverbound, "minecraft:swing"),
        Some(5)
    );
    assert_eq!(
        new.packet_id(State::Play, Direction::Serverbound, "minecraft:swing"),
        None
    );
}

#[test]
fn an_id_translates_through_its_name() {
    let (old, new) = (fixture("0.1-test"), fixture("0.2-test"));
    let stone = translate_id(&old, &new, "minecraft:block", 1).unwrap();
    assert_eq!(stone.name, "minecraft:stone");
    assert_eq!(
        stone.to_id,
        Some(2),
        "the block kept its name and lost its number"
    );
    assert!(stone.is_translatable());

    let same = translate_id(&old, &new, "minecraft:block", 0).unwrap();
    assert_eq!(
        (same.name.as_str(), same.to_id),
        ("minecraft:air", Some(0)),
        "an unchanged entry passes through"
    );

    let swing = translate_packet(&old, &new, State::Play, Direction::Serverbound, 5).unwrap();
    assert_eq!(swing.name, "minecraft:swing");
    assert_eq!(
        swing.to_id, None,
        "the newer version renamed the packet that had that id"
    );
    assert!(!swing.is_translatable());
    assert!(translate_packet(&old, &new, State::Play, Direction::Serverbound, 99).is_none());
}

#[test]
fn a_name_the_other_version_lacks_is_refused_not_guessed() {
    let (old, new) = (fixture("0.1-test"), fixture("0.2-test"));
    let golem = translate_id(&new, &old, "minecraft:block", 1).unwrap();
    assert_eq!(golem.name, "minecraft:copper_golem");
    assert_eq!(golem.to_id, None);
    assert!(!golem.is_translatable());
}

#[test]
fn drift_counts_what_moved() {
    let (old, new) = (fixture("0.1-test"), fixture("0.2-test"));
    let blocks = registry_drift(&old, &new, "minecraft:block").unwrap();
    assert_eq!(blocks.kept, 1, "only air keeps its number");
    assert_eq!(
        blocks
            .added
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["minecraft:copper_golem"]
    );
    assert_eq!(blocks.shifted.len(), 2);
    assert!(!blocks.is_still_translatable());
    let items = registry_drift(&old, &new, "minecraft:item").unwrap();
    assert!(items.is_still_translatable());
    assert_eq!(registry_drift(&old, &new, "minecraft:nope"), None);
    assert_eq!(
        shared_registries(&old, &new),
        ["minecraft:block", "minecraft:item"]
    );

    let flows = packet_drift(&old, &new);
    let play = flows
        .iter()
        .find(|flow| flow.state == State::Play && flow.direction == Direction::Serverbound)
        .unwrap();
    assert_eq!(
        play.removed
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["minecraft:swing"]
    );
    assert_eq!(
        play.added
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["minecraft:punch"]
    );
}

#[test]
fn drift_accounts_for_every_entry_on_both_sides() {
    let (old, new) = (fixture("0.1-test"), fixture("0.2-test"));
    for registry in shared_registries(&old, &new) {
        let drift = registry_drift(&old, &new, &registry).unwrap();
        let IdDrift {
            kept,
            added,
            removed,
            shifted,
            ..
        } = &drift;
        assert_eq!(
            kept + added.len() + shifted.len(),
            new.registry(&registry).unwrap().entries.len(),
            "{registry}"
        );
        assert_eq!(
            kept + removed.len() + shifted.len(),
            old.registry(&registry).unwrap().entries.len(),
            "{registry}"
        );
    }
}

#[test]
fn a_bundle_carries_every_version_as_data_and_reads_back() {
    let catalog = catalog();
    let bundle = ProfileBundle::of(&catalog);
    assert_eq!(bundle.len(), 2);
    let text = bundle.to_json();
    let read = ProfileBundle::from_json(&text).unwrap();
    assert_eq!(read, bundle);
    let back = read.catalog().unwrap();
    assert_eq!(back.protocols(), catalog.protocols());
    assert_eq!(
        back.get("0.2-test")
            .unwrap()
            .entry_id("minecraft:block", "minecraft:stone"),
        Some(2)
    );

    let trimmed = ProfileBundle::with_registries(&catalog, Some(&["minecraft:block".to_owned()]));
    assert!(trimmed.json_len() < text.len());
    assert_eq!(
        trimmed
            .catalog()
            .unwrap()
            .get("0.1-test")
            .unwrap()
            .registries()
            .len(),
        1
    );
    assert!(ProfileBundle::from_json(r#"{"format": 99, "versions": []}"#).is_err());
}

#[test]
fn a_version_it_cannot_read_is_reported_not_dropped() {
    let store = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/store");
    let error = VersionCatalog::open(&store).unwrap_err();
    assert!(error.to_string().contains("0.2-nodata"), "{error}");
}

#[test]
fn a_store_loads_what_it_can_and_names_the_rest() {
    let store = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/store");
    let (catalog, problems) = VersionCatalog::open_reporting(&store).unwrap();
    assert_eq!(
        catalog.ids().collect::<Vec<_>>(),
        ["0.1-test", "0.2-test", "0.3-test", "0.4-nopackets"],
        "the good versions still load"
    );
    assert_eq!(
        problems
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["0.2-nodata"]
    );
    assert!(
        problems[0]
            .1
            .to_string()
            .contains("no data generator reports"),
        "{}",
        problems[0].1
    );
}

#[test]
fn two_releases_sharing_a_protocol_resolve_by_their_names() {
    // Real, not hypothetical: Mojang shipped 1.20.5 and 1.20.6 under one protocol number, 766.
    let store = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/store");
    let (catalog, _) = VersionCatalog::open_reporting(&store).unwrap();
    assert!(catalog.supports(101), "a shared number is supported");
    assert_eq!(
        catalog.protocols(),
        vec![101, 102, 103],
        "one number listed once"
    );
    let error = catalog.negotiate(101).unwrap_err();
    assert!(
        matches!(&error, Negotiation::Ambiguous { protocol: 101, versions } if versions.len() == 2),
        "{error:?}"
    );
    assert_eq!(
        catalog
            .negotiate_with_id(101, "0.3-test")
            .unwrap()
            .data_version(),
        4200
    );
    assert_eq!(
        catalog
            .negotiate_with_id(101, "0.1-test")
            .unwrap()
            .data_version(),
        4000
    );
    assert!(
        catalog
            .get("0.3-test")
            .unwrap()
            .packet_id(State::Play, Direction::Clientbound, "minecraft:world_event")
            .is_some()
    );
    assert!(
        catalog
            .get("0.1-test")
            .unwrap()
            .packet_id(State::Play, Direction::Clientbound, "minecraft:world_event")
            .is_none()
    );
}

#[test]
fn a_version_without_a_packet_report_still_carries_its_registries() {
    // Mojang only writes packets.json from 1.21: 1.20.6 and older report registries and nothing else.
    let profile = fixture("0.4-nopackets");
    assert!(!profile.has_packets());
    assert!(profile.has_registries());
    assert_eq!(
        profile.entry_id("minecraft:block", "minecraft:stone"),
        Some(1)
    );
    assert!(
        profile.summary().contains("no packet report"),
        "{}",
        profile.summary()
    );

    let other = fixture("0.1-test");
    assert!(
        packet_drift(&other, &profile).is_empty(),
        "no report means unknown, and an empty list is unknown"
    );
    assert!(
        packet_drift(&other, &other)
            .iter()
            .any(|flow| flow.kept > 0),
        "two versions that report do compare"
    );
    let drift = registry_drift(&other, &profile, "minecraft:block").unwrap();
    assert!(
        drift.is_still_translatable(),
        "its registry ids still compare"
    );

    let mut catalog = VersionCatalog::new();
    catalog.add(profile.clone()).unwrap();
    let carried = ProfileBundle::of(&catalog).catalog().unwrap();
    assert!(
        !carried.get("0.4-nopackets").unwrap().has_packets(),
        "a bundle does not invent a packet table"
    );
    assert_eq!(
        carried
            .negotiate(103)
            .unwrap()
            .entry_id("minecraft:item", "minecraft:stone"),
        Some(1)
    );
}

#[test]
fn the_real_store_negotiates_and_translates() {
    let Some(store) = real_store() else {
        eprintln!("CRYON_VERSIONS is not a directory: skipping the real-version half");
        return;
    };
    let (catalog, problems) = VersionCatalog::open_reporting(&store).expect("the store reads");
    assert!(
        problems.is_empty(),
        "in the store but not in the catalog: {:?}",
        problems
            .iter()
            .map(|(name, error)| (name, error.to_string()))
            .collect::<Vec<_>>()
    );
    assert!(
        catalog.len() >= 2,
        "{} needs at least two prepared versions",
        store.display()
    );

    for profile in catalog.profiles() {
        assert_eq!(
            catalog
                .negotiate_with_id(profile.protocol(), profile.id())
                .unwrap()
                .id(),
            profile.id()
        );
        match catalog.negotiate(profile.protocol()) {
            Ok(only) => assert_eq!(
                only.id(),
                profile.id(),
                "{} is not alone on {}",
                profile.id(),
                profile.protocol()
            ),
            Err(Negotiation::Ambiguous { versions, .. }) => {
                assert!(
                    versions.contains(&profile.id().to_owned()),
                    "{versions:?} leaves {} out",
                    profile.id()
                );
            }
            Err(other) => panic!(
                "{} announces protocol {}: {other}",
                profile.id(),
                profile.protocol()
            ),
        }
        for (name, id) in profile.registry("minecraft:block").unwrap().entries.iter() {
            assert_eq!(
                profile.entry_name("minecraft:block", id),
                Some(name),
                "{} numbers one block twice",
                profile.id()
            );
        }
    }

    let versions = catalog.ids().collect::<Vec<_>>();
    for pair in versions.windows(2) {
        let (old, new) = (catalog.get(pair[0]).unwrap(), catalog.get(pair[1]).unwrap());
        for registry in ["minecraft:block", "minecraft:item", "minecraft:entity_type"] {
            let drift = registry_drift(old, new, registry).expect("both versions have it");
            assert_eq!(
                drift.kept + drift.added.len() + drift.shifted.len(),
                new.registry(registry).unwrap().entries.len(),
                "{} -> {} {registry}",
                old.id(),
                new.id()
            );
        }
        if old.has_packets() && new.has_packets() {
            assert!(
                !packet_drift(old, new).is_empty(),
                "{} -> {} reports packets and has no flows",
                old.id(),
                new.id()
            );
        }
    }

    let bundle = ProfileBundle::of(&catalog);
    assert_eq!(bundle.catalog().unwrap().len(), catalog.len());
}
