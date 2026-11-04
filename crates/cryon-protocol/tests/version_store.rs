//! The version store and the generated tables have to agree, or one of them is the wrong source of truth.
//!
//! `crates/cryon-protocol/src/ids.rs` is generated from the pinned version's `packets.json` report
//! (`tools/generate_packet_ids.py`). This test reads the same report through `cryon-versions` and compares
//! them packet by packet, so the store can become what a build reads instead of a generated source file —
//! which is the step every other version depends on. It needs a prepared store, so it skips without
//! `CRYON_VERSIONS` (`tools/version_port.py prepare VERSION`).

use std::path::PathBuf;

use cryon_protocol::{Direction, State, ids};
use cryon_versions::{Direction as StoreDirection, State as StoreState, VersionCatalog, VersionProfile};

/// One flow of each side's enums, with a label for the assertion.
const FLOWS: [(StoreState, StoreDirection, State, Direction, &str); 9] = [
    (StoreState::Handshake, StoreDirection::Serverbound, State::Handshake, Direction::Serverbound, "handshake/serverbound"),
    (StoreState::Status, StoreDirection::Clientbound, State::Status, Direction::Clientbound, "status/clientbound"),
    (StoreState::Status, StoreDirection::Serverbound, State::Status, Direction::Serverbound, "status/serverbound"),
    (StoreState::Login, StoreDirection::Clientbound, State::Login, Direction::Clientbound, "login/clientbound"),
    (StoreState::Login, StoreDirection::Serverbound, State::Login, Direction::Serverbound, "login/serverbound"),
    (
        StoreState::Configuration,
        StoreDirection::Clientbound,
        State::Configuration,
        Direction::Clientbound,
        "configuration/clientbound"
    ),
    (
        StoreState::Configuration,
        StoreDirection::Serverbound,
        State::Configuration,
        Direction::Serverbound,
        "configuration/serverbound"
    ),
    (StoreState::Play, StoreDirection::Clientbound, State::Play, Direction::Clientbound, "play/clientbound"),
    (StoreState::Play, StoreDirection::Serverbound, State::Play, Direction::Serverbound, "play/serverbound"),
];

/// A prepared version store, when this machine has one.
fn store() -> Option<PathBuf> {
    let directory = std::env::var_os("CRYON_VERSIONS").map(PathBuf::from)?;
    directory.is_dir().then_some(directory)
}

/// The pinned version's profile, or None when no store is prepared (then the test skips).
fn pinned() -> Option<VersionProfile> {
    let directory = store()?;
    let (catalog, problems) = VersionCatalog::open_reporting(&directory).expect("the store reads");
    assert!(
        problems.is_empty(),
        "in the store but not in the catalog: {:?}",
        problems.iter().map(|(name, error)| (name, error.to_string())).collect::<Vec<_>>()
    );
    match catalog.negotiate(cryon_protocol::PROTOCOL_VERSION) {
        Ok(profile) => Some(profile.clone()),
        // Two releases can share the pinned protocol number; the store's own id then has to say which.
        Err(cryon_versions::Negotiation::Ambiguous { versions, .. }) => versions
            .iter()
            .find(|id| id.as_str() == cryon_protocol::GAME_VERSION)
            .and_then(|id| catalog.get(id))
            .cloned(),
        Err(_) => None,
    }
}

/// `minecraft:swing` as the generated table names it, `swing`.
fn unnamespaced(name: &str) -> &str {
    name.split_once(':').map_or(name, |(_, rest)| rest)
}

#[test]
fn the_generated_packet_table_is_what_the_version_store_reports() {
    let Some(pinned) = pinned() else {
        eprintln!("CRYON_VERSIONS is not a prepared store: skipping the version store check");
        return;
    };
    assert_eq!(pinned.id(), cryon_protocol::GAME_VERSION, "the store's pinned version and the crate's name disagree");
    assert_eq!(pinned.protocol(), cryon_protocol::PROTOCOL_VERSION);

    let mut compared = 0;
    for (store_state, store_direction, protocol_state, protocol_direction, label) in FLOWS {
        let Some(flow) = pinned.flow(store_state, store_direction) else {
            assert!(
                ids::name(protocol_state, protocol_direction, 0).is_none(),
                "{label}: the store reports no packets but ids.rs names some"
            );
            continue;
        };
        let highest = flow.by_id().map(|(id, _)| id).max().unwrap_or(-1);
        for id in 0..=highest {
            assert_eq!(
                ids::name(protocol_state, protocol_direction, id),
                flow.name(id).map(unnamespaced),
                "{label} id {id} disagrees between ids.rs and {}'s packets.json",
                pinned.id()
            );
            compared += 1;
        }
        // ids.rs is a dense array: nothing may continue past the report's last id.
        let mut past = highest + 1;
        let mut extra = Vec::new();
        while let Some(name) = ids::name(protocol_state, protocol_direction, past) {
            extra.push((past, name));
            past += 1;
        }
        assert!(extra.is_empty(), "{label}: ids.rs names {extra:?} past the report's {highest}");
    }
    assert!(compared > 200, "only {compared} packet ids were compared: the store is thinner than expected");
}

#[test]
fn the_store_agrees_with_the_pinned_versions_own_facts() {
    let Some(pinned) = pinned() else {
        eprintln!("CRYON_VERSIONS is not a prepared store: skipping the version store check");
        return;
    };
    let facts = pinned.facts();
    assert_eq!(facts.protocol_version, cryon_protocol::PROTOCOL_VERSION);
    assert_eq!(
        facts.world_version,
        cryon_world::DATA_VERSION,
        "the save format the crate claims is not the store's"
    );
}
