use std::path::Path;

use crate::facts::Facts;
use crate::packets::{Direction, FlowTable, PacketTables, State, packets_from_value};
use crate::registry::{Registries, Registry, registries_from_value};
use crate::store::{self, StoreError};

/// One Minecraft version's wire facts, packet ids and registries.
///
/// This is what Cryon currently compiles in: `crates/cryon-core/src/version.rs` for the facts,
/// `crates/cryon-protocol/src/ids.rs` for the packets and the registry tables. As a value it can be built
/// for several versions, held together, and picked per connection instead of baked into the binary.
#[derive(Debug, Clone)]
pub struct VersionProfile {
    facts: Facts,
    packets: PacketTables,
    registries: Registries,
}

impl VersionProfile {
    /// Assembles a profile from already-read parts, which is how a bundled profile comes back
    /// ([`ProfileBundle::profile`](crate::ProfileBundle::profile)).
    pub fn new(facts: Facts, packets: PacketTables, registries: Registries) -> Self {
        Self {
            facts,
            packets,
            registries,
        }
    }

    /// Reads a prepared version directory: its `facts.json` and the data generator reports its
    /// `prepared.json` points at.
    ///
    /// A version reports what it has: Mojang only started writing `packets.json` in 1.21, so an older one
    /// loads with its registries and no packet table. Only a version with neither report is a failure.
    pub fn load(directory: &Path) -> Result<Self, StoreError> {
        let prepared = store::read_prepared(directory)?;
        let facts_path = directory.join("facts.json");
        let facts: Facts = serde_json::from_value(store::read_json(&facts_path)?)
            .map_err(|error| StoreError::json(&facts_path, &error))?;
        let packets_path = prepared.reports.join("packets.json");
        let registries_path = prepared.reports.join("registries.json");
        if !packets_path.is_file() && !registries_path.is_file() {
            return Err(StoreError::NoReports {
                version: prepared.version,
                path: prepared.reports,
            });
        }
        let packets = if packets_path.is_file() {
            packets_from_value(&store::read_json(&packets_path)?)
        } else {
            Default::default()
        };
        let registries = if registries_path.is_file() {
            registries_from_value(&store::read_json(&registries_path)?)
        } else {
            Default::default()
        };
        Ok(Self::new(facts, packets, registries))
    }

    /// Reads one named version out of a store.
    pub fn from_store(store: &Path, version: &str) -> Result<Self, StoreError> {
        let directory = store.join(version);
        if !directory.join("prepared.json").is_file() {
            return Err(StoreError::Unknown {
                store: store.to_path_buf(),
                version: version.to_owned(),
            });
        }
        Self::load(&directory)
    }

    /// The version's own statements: protocol number, data version, pack formats, Java.
    pub fn facts(&self) -> &Facts {
        &self.facts
    }

    /// The version id, `26.3`.
    pub fn id(&self) -> &str {
        &self.facts.id
    }

    /// `SharedConstants.getProtocolVersion()`, the number a client announces at handshake.
    pub fn protocol(&self) -> i32 {
        self.facts.protocol_version
    }

    /// `WorldVersion.dataVersion()`, what saved chunks and entities declare themselves to be.
    pub fn data_version(&self) -> i32 {
        self.facts.world_version
    }

    /// Every packet table of this version.
    pub fn packets(&self) -> &PacketTables {
        &self.packets
    }

    /// One flow's packets.
    pub fn flow(&self, state: State, direction: Direction) -> Option<&FlowTable> {
        self.packets.flow(state, direction)
    }

    /// The id this version puts on the wire for a packet.
    pub fn packet_id(&self, state: State, direction: Direction, name: &str) -> Option<i32> {
        self.packets.id(state, direction, name)
    }

    /// The packet this version's id in this flow names.
    pub fn packet_name(&self, state: State, direction: Direction, id: i32) -> Option<&str> {
        self.packets.name(state, direction, id)
    }

    /// Every registry of this version.
    pub fn registries(&self) -> &Registries {
        &self.registries
    }

    /// One registry, `minecraft:block`.
    pub fn registry(&self, name: &str) -> Option<&Registry> {
        self.registries.get(name)
    }

    /// The id this version gives an entry of a registry, the number a chunk palette or a protocol id field
    /// carries.
    pub fn entry_id(&self, registry: &str, entry: &str) -> Option<i32> {
        self.registries.entry_id(registry, entry)
    }

    /// The entry this version's registry id names.
    pub fn entry_name(&self, registry: &str, id: i32) -> Option<&str> {
        self.registries.entry_name(registry, id)
    }

    /// A one-line description for lists and logs.
    pub fn summary(&self) -> String {
        let kind = if self.facts.is_stable() {
            "release"
        } else {
            "snapshot"
        };
        let packets = if self.has_packets() {
            format!("{} packets", self.packets.packet_count())
        } else {
            "no packet report".to_owned()
        };
        format!(
            "{} ({}): protocol {}, data version {}, {}, {} registries",
            self.facts.id,
            kind,
            self.facts.protocol_version,
            self.facts.world_version,
            packets,
            self.registries.len()
        )
    }

    /// Whether this version's reports carried packet ids: they only do from 1.21 up.
    pub fn has_packets(&self) -> bool {
        !self.packets.is_empty()
    }

    /// Whether this version's reports carried any registry.
    pub fn has_registries(&self) -> bool {
        !self.registries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> VersionProfile {
        let directory =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/store/0.1-test");
        VersionProfile::load(&directory).expect("the fixture store reads")
    }

    #[test]
    fn reads_a_prepared_version() {
        let profile = fixture();
        assert_eq!(profile.id(), "0.1-test");
        assert_eq!(profile.protocol(), 101);
        assert_eq!(profile.data_version(), 4000);
        assert_eq!(
            profile.packet_id(
                State::Play,
                Direction::Clientbound,
                "minecraft:spawn_entity"
            ),
            Some(2)
        );
        assert_eq!(
            profile.entry_id("minecraft:block", "minecraft:stone"),
            Some(1)
        );
        assert_eq!(
            profile
                .registry("minecraft:block")
                .unwrap()
                .default
                .as_deref(),
            Some("minecraft:air")
        );
    }

    #[test]
    fn a_name_and_an_id_always_agree() {
        let profile = fixture();
        for (name, id) in profile.registry("minecraft:block").unwrap().entries.iter() {
            assert_eq!(profile.entry_name("minecraft:block", id), Some(name));
        }
    }
}
