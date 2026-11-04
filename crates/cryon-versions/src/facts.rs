use serde::{Deserialize, Serialize};

/// What a Minecraft version says about itself, from the client jar's `version.json` (`tools/version_port.py
/// prepare` writes it to the version store as `facts.json`).
///
/// These are the values Cryon currently bakes into `crates/cryon-core/src/version.rs` as constants. Here
/// they travel with a profile, so a build can hold several versions at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    /// The version id, `26.3`.
    pub id: String,
    /// `WorldVersion.dataVersion()`: the DataVersion saved chunks and entities carry.
    #[serde(default)]
    pub world_version: i32,
    /// `SharedConstants.getProtocolVersion()`, the number a client announces at handshake.
    #[serde(default)]
    pub protocol_version: i32,
    /// The client resource pack format, `major.minor`.
    #[serde(default)]
    pub resource_pack: String,
    /// The server data pack format, `major.minor`.
    #[serde(default)]
    pub data_pack: String,
    /// The Java major version the game runs on.
    #[serde(default)]
    pub java_version: u32,
    /// `WorldVersion.stable()`; Mojang leaves it out or null for a snapshot's own jar in some versions.
    #[serde(default)]
    pub stable: Option<bool>,
}

impl Facts {
    /// Whether Mojang calls this a release rather than a snapshot, pre-release or release candidate.
    pub fn is_stable(&self) -> bool {
        self.stable.unwrap_or(false)
    }

    /// Whether this is a development version, the inverse of [`is_stable`](Self::is_stable).
    pub fn is_snapshot(&self) -> bool {
        !self.is_stable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_full_record() {
        let facts: Facts = serde_json::from_str(
            r#"{"id":"26.3","world_version":5023,"protocol_version":777,"resource_pack":"97.1",
                "data_pack":"121.0","java_version":25,"stable":true}"#,
        )
        .unwrap();
        assert_eq!(facts.protocol_version, 777);
        assert!(facts.is_stable());
        assert!(!facts.is_snapshot());
    }

    #[test]
    fn missing_and_null_fields_do_not_stop_a_load() {
        let facts: Facts = serde_json::from_str(r#"{"id":"1.20.4","stable":null}"#).unwrap();
        assert_eq!(facts.protocol_version, 0);
        assert_eq!(facts.resource_pack, "");
        assert!(!facts.is_stable());
        assert!(facts.is_snapshot());
    }
}
