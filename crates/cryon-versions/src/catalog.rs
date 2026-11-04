use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use crate::profile::VersionProfile;
use crate::store::StoreError;

/// Every Minecraft version one build can speak.
///
/// A client announces a protocol number at handshake and the catalog answers which version's tables to
/// read with, which is what today's build cannot do: it compares against one constant and rejects every
/// other number.
#[derive(Debug, Clone, Default)]
pub struct VersionCatalog {
    by_id: BTreeMap<String, VersionProfile>,
}

/// What went wrong matching a handshake to a version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Negotiation {
    /// No version in the catalog answers that number.
    Unsupported {
        /// What the client asked for.
        protocol: i32,
        /// What this build does answer.
        supported: Vec<i32>,
    },
    /// More than one release shares that number: Mojang has done this (1.20.5 and 1.20.6 are both protocol
    /// 766), so the number alone cannot pick a profile and the handshake's version name must.
    Ambiguous {
        /// What the client asked for.
        protocol: i32,
        /// The versions that answer it, in id order.
        versions: Vec<String>,
    },
}

impl fmt::Display for Negotiation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Negotiation::Unsupported {
                protocol,
                supported,
            } => {
                let supported = if supported.is_empty() {
                    "none".to_owned()
                } else {
                    supported
                        .iter()
                        .map(i32::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                write!(
                    f,
                    "protocol {protocol} is not supported by this build (it speaks: {supported})"
                )
            }
            Negotiation::Ambiguous { protocol, versions } => write!(
                f,
                "protocol {protocol} is answered by {} versions ({}); the handshake must name one",
                versions.len(),
                versions.join(", ")
            ),
        }
    }
}

impl std::error::Error for Negotiation {}

impl VersionCatalog {
    /// An empty catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads every prepared version in a store (`CRYON_VERSIONS`, by default
    /// `~/code/cryon-parity/versions`).
    ///
    /// All or nothing: a build that meant to carry twelve versions and silently carries eleven is the bug
    /// this crate exists to avoid, so one unreadable version fails the read. Use
    /// [`open_reporting`](Self::open_reporting) to keep loading and see what was left out.
    pub fn open(store_directory: &Path) -> Result<Self, StoreError> {
        let (catalog, problems) = Self::open_reporting(store_directory)?;
        match problems.into_iter().next() {
            Some((_, error)) => Err(error),
            None => Ok(catalog),
        }
    }

    /// Reads a store, loading every version it can and returning the rest with why.
    ///
    /// Each problem is paired with the version's directory name, so a launcher can offer the versions that
    /// read and log the ones that did not. A duplicated protocol is a problem too: the earlier version
    /// keeps the number and the later one is reported.
    pub fn open_reporting(
        store_directory: &Path,
    ) -> Result<(Self, Vec<(String, StoreError)>), StoreError> {
        let mut catalog = Self::new();
        let mut problems = Vec::new();
        let entries = std::fs::read_dir(store_directory)
            .map_err(|error| StoreError::io(store_directory, &error))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect::<Vec<_>>();
        let mut directories = entries
            .into_iter()
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        directories.sort();
        for directory in directories {
            if !directory.join("prepared.json").is_file() {
                continue;
            }
            let name = directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            match VersionProfile::load(&directory) {
                Ok(profile) => {
                    if let Err(error) = catalog.add(profile) {
                        problems.push((name, error));
                    }
                }
                Err(error) => problems.push((name, error)),
            }
        }
        Ok((catalog, problems))
    }

    /// Adds a version. Two releases may answer one protocol number, which is real rather than hypothetical
    /// (1.20.5 and 1.20.6 are both 766), so only the same version id twice is refused.
    pub fn add(&mut self, profile: VersionProfile) -> Result<(), StoreError> {
        if self.by_id.contains_key(profile.id()) {
            return Err(StoreError::DuplicateVersion {
                version: profile.id().to_owned(),
            });
        }
        self.by_id.insert(profile.id().to_owned(), profile);
        Ok(())
    }

    /// A version by its id, `26.3`.
    pub fn get(&self, id: &str) -> Option<&VersionProfile> {
        self.by_id.get(id)
    }

    /// Every version that answers to a protocol number, in id order: usually one, sometimes a pair that
    /// shares it.
    pub fn profiles_for(&self, protocol: i32) -> Vec<&VersionProfile> {
        self.by_id
            .values()
            .filter(|profile| profile.protocol() == protocol)
            .collect()
    }

    /// The version answering that number when exactly one does, and None when none does or when several
    /// share it. A caller that has only the number to go on must not guess between a pair, so it gets None
    /// and the ambiguity: use [`profiles_for`](Self::profiles_for) or [`negotiate`](Self::negotiate).
    pub fn by_protocol(&self, protocol: i32) -> Option<&VersionProfile> {
        let mut answering = self.profiles_for(protocol).into_iter();
        match (answering.next(), answering.next()) {
            (Some(only), None) => Some(only),
            _ => None,
        }
    }

    /// Picks the version for a handshake's protocol number, or says what this build does speak.
    ///
    /// This is the decision `crates/cryon-client/src/lan/conn.rs` makes today with `!= PROTOCOL_VERSION`.
    /// A pair of releases sharing the number is [`Negotiation::Ambiguous`], and the client does name its
    /// version beside it in the handshake, so answer that with
    /// [`negotiate_with_id`](Self::negotiate_with_id).
    pub fn negotiate(&self, protocol: i32) -> Result<&VersionProfile, Negotiation> {
        match self.profiles_for(protocol).as_slice() {
            [] => Err(Negotiation::Unsupported {
                protocol,
                supported: self.protocols(),
            }),
            [only] => Ok(only),
            many => Err(Negotiation::Ambiguous {
                protocol,
                versions: many.iter().map(|profile| profile.id().to_owned()).collect(),
            }),
        }
    }

    /// Chooses between the versions that share a protocol number by the version id the client names.
    pub fn negotiate_with_id(
        &self,
        protocol: i32,
        version_id: &str,
    ) -> Result<&VersionProfile, Negotiation> {
        self.profiles_for(protocol)
            .into_iter()
            .find(|profile| profile.id() == version_id)
            .ok_or_else(|| Negotiation::Unsupported {
                protocol,
                supported: self.protocols(),
            })
    }

    /// Whether this build can serve a client of that protocol at all, shared number or not.
    pub fn supports(&self, protocol: i32) -> bool {
        !self.profiles_for(protocol).is_empty()
    }

    /// Every version id, oldest key first.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.by_id.keys().map(String::as_str)
    }

    /// Every profile.
    pub fn profiles(&self) -> impl Iterator<Item = &VersionProfile> {
        self.by_id.values()
    }

    /// Every protocol number this build answers, ascending, with a shared number listed once.
    pub fn protocols(&self) -> Vec<i32> {
        let mut found = self
            .by_id
            .values()
            .map(VersionProfile::protocol)
            .collect::<Vec<_>>();
        found.sort_unstable();
        found.dedup();
        found
    }

    /// How many versions the build carries.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Whether the build speaks no version, which cannot launch.
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// The newest release (not snapshot) in the catalog, by data version.
    pub fn newest_release(&self) -> Option<&VersionProfile> {
        self.by_id
            .values()
            .filter(|profile| profile.facts().is_stable())
            .max_by_key(|profile| profile.data_version())
    }

    /// The newest version of any kind, by data version: what a launcher would default to while a snapshot
    /// is being tested.
    pub fn newest(&self) -> Option<&VersionProfile> {
        self.by_id
            .values()
            .max_by_key(|profile| profile.data_version())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::Facts;
    use crate::packets::{Direction, State, packets_from_value};
    use crate::registry::Registries;

    fn profile(id: &str, protocol: i32, data_version: i32, stable: bool) -> VersionProfile {
        VersionProfile::new(
            Facts {
                id: id.into(),
                world_version: data_version,
                protocol_version: protocol,
                resource_pack: String::new(),
                data_pack: String::new(),
                java_version: 21,
                stable: Some(stable),
            },
            packets_from_value(&serde_json::json!({
                "play": {"clientbound": {"minecraft:keep_alive": {"protocol_id": 0}}}
            })),
            Registries::default(),
        )
    }

    fn catalog() -> VersionCatalog {
        let mut catalog = VersionCatalog::new();
        catalog.add(profile("26.2", 776, 4903, true)).unwrap();
        catalog.add(profile("26.3", 777, 5023, true)).unwrap();
        catalog
            .add(profile("26.4-snapshot-6", 1_073_742_168, 5126, false))
            .unwrap();
        catalog
    }

    #[test]
    fn one_build_answers_several_protocols() {
        let catalog = catalog();
        assert_eq!(catalog.negotiate(776).unwrap().id(), "26.2");
        assert_eq!(catalog.negotiate(777).unwrap().id(), "26.3");
        assert_eq!(
            catalog.negotiate(1_073_742_168).unwrap().id(),
            "26.4-snapshot-6"
        );
        assert_eq!(catalog.protocols(), vec![776, 777, 1_073_742_168]);
    }

    #[test]
    fn an_unknown_protocol_says_what_the_build_speaks() {
        let error = catalog().negotiate(575).unwrap_err();
        assert!(
            matches!(error, Negotiation::Unsupported { protocol: 575, .. }),
            "{error:?}"
        );
        assert!(
            error.to_string().contains("776, 777, 1073742168"),
            "{error}"
        );
    }

    #[test]
    fn two_releases_may_share_one_protocol_and_the_catalog_says_so() {
        let mut catalog = catalog();
        catalog.add(profile("26.3-remap", 777, 5024, true)).unwrap();
        assert!(catalog.supports(777), "a shared number is still supported");
        assert_eq!(
            catalog.protocols(),
            vec![776, 777, 1_073_742_168],
            "one number listed once"
        );
        let error = catalog.negotiate(777).unwrap_err();
        assert!(
            matches!(&error, Negotiation::Ambiguous { protocol: 777, versions } if versions.len() == 2),
            "{error:?}"
        );
        assert!(error.to_string().contains("26.3"), "{error}");
        assert!(
            catalog.by_protocol(777).is_none(),
            "the number alone picks nobody"
        );
        assert_eq!(
            catalog
                .negotiate_with_id(777, "26.3-remap")
                .unwrap()
                .data_version(),
            5024
        );
        assert!(
            matches!(
                catalog.negotiate_with_id(777, "26.9").unwrap_err(),
                Negotiation::Unsupported { protocol: 777, .. }
            ),
            "an id that does not answer that number is refused"
        );
    }

    #[test]
    fn the_same_version_id_twice_is_refused() {
        let mut catalog = catalog();
        let error = catalog.add(profile("26.3", 778, 5024, true)).unwrap_err();
        assert!(
            matches!(error, StoreError::DuplicateVersion { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn newest_skips_snapshots_when_asked_for_a_release() {
        let catalog = catalog();
        assert_eq!(catalog.newest().unwrap().id(), "26.4-snapshot-6");
        assert_eq!(catalog.newest_release().unwrap().id(), "26.3");
        assert!(VersionCatalog::new().newest_release().is_none());
    }

    #[test]
    fn packets_come_from_the_negotiated_version_not_the_build() {
        let catalog = catalog();
        let profile = catalog.negotiate(776).unwrap();
        assert_eq!(
            profile.packet_id(State::Play, Direction::Clientbound, "minecraft:keep_alive"),
            Some(0)
        );
    }
}
