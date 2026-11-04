use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::catalog::VersionCatalog;
use crate::facts::Facts;
use crate::packets::{Direction, PacketTables, State};
use crate::profile::VersionProfile;
use crate::registry::{Registries, Registry};
use crate::store::StoreError;

/// What a bundle document is, so a build that cannot read it says so instead of loading half a version.
pub const FORMAT: u32 = 1;

/// One flow's packets as data: `{"state": "play", "direction": "clientbound", "entries": [["name", id]…]}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleFlow {
    /// The protocol phase.
    pub state: State,
    /// Which way the packets travel.
    pub direction: Direction,
    /// Every packet with the id this version gives it.
    pub entries: Vec<(String, i32)>,
}

/// One registry as data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleRegistry {
    /// The registry's name, `minecraft:block`.
    pub name: String,
    /// The registry's own sync id.
    pub protocol_id: Option<i32>,
    /// The entry the registry falls back to.
    pub default: Option<String>,
    /// Every entry with the id this version gives it.
    pub entries: Vec<(String, i32)>,
}

/// One version, ready to write out or read back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleVersion {
    /// What the version states about itself.
    pub facts: Facts,
    /// Its packet ids.
    pub flows: Vec<BundleFlow>,
    /// Its registry ids.
    pub registries: Vec<BundleRegistry>,
}

/// Every version a build carries, as one document.
///
/// A shipping client does not need a version store at run time: `to_json` writes this once at build time,
/// `include_str!` pulls it into the binary and [`catalog`](Self::catalog) reads it back, so every version's
/// ids travel as data instead of as generated source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileBundle {
    /// The document format, [`FORMAT`].
    pub format: u32,
    /// The versions it carries.
    pub versions: Vec<BundleVersion>,
}

/// What can go wrong with a bundle document.
#[derive(Debug)]
pub enum BundleError {
    /// The file could not be read or written.
    Io {
        path: std::path::PathBuf,
        message: String,
    },
    /// The file was not a bundle.
    Json { message: String },
    /// The document was written by a newer or older tool.
    Format { found: u32 },
    /// A version in it cannot become a profile.
    Store(StoreError),
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BundleError::Io { path, message } => write!(f, "{}: {message}", path.display()),
            BundleError::Json { message } => write!(f, "not a version bundle: {message}"),
            BundleError::Format { found } => {
                write!(f, "bundle format {found}, this build reads format {FORMAT}")
            }
            BundleError::Store(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for BundleError {}

impl ProfileBundle {
    /// Writes every version of a catalog out, with all its registries.
    pub fn of(catalog: &VersionCatalog) -> Self {
        Self::with_registries(catalog, None)
    }

    /// Writes every version out, keeping only the named registries (all of them when `keep` is None).
    ///
    /// A client on a server needs the block, item, entity type and a few more to interpret an id it is
    /// sent; the full 95 registries of a version are far more bytes than that, so a shipping build picks.
    pub fn with_registries(catalog: &VersionCatalog, keep: Option<&[String]>) -> Self {
        ProfileBundle {
            format: FORMAT,
            versions: catalog
                .profiles()
                .map(|profile| BundleVersion {
                    facts: profile.facts().clone(),
                    flows: profile
                        .packets()
                        .flows()
                        .map(|(state, direction, flow)| BundleFlow {
                            state,
                            direction,
                            entries: flow
                                .iter()
                                .map(|(name, id)| (name.to_owned(), id))
                                .collect(),
                        })
                        .collect(),
                    registries: profile
                        .registries()
                        .iter()
                        .filter(|(name, _)| {
                            keep.is_none_or(|keep| keep.iter().any(|want| want == *name))
                        })
                        .map(|(name, registry)| BundleRegistry {
                            name: name.to_owned(),
                            protocol_id: registry.protocol_id,
                            default: registry.default.clone(),
                            entries: registry
                                .entries
                                .iter()
                                .map(|(entry, id)| (entry.to_owned(), id))
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// The document as one compact JSON line.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Reads a bundle document, refusing one written in another format.
    pub fn from_json(text: &str) -> Result<Self, BundleError> {
        let bundle: ProfileBundle =
            serde_json::from_str(text).map_err(|error| BundleError::Json {
                message: error.to_string(),
            })?;
        if bundle.format != FORMAT {
            return Err(BundleError::Format {
                found: bundle.format,
            });
        }
        Ok(bundle)
    }

    /// Reads a bundle document from a file.
    pub fn load(path: &Path) -> Result<Self, BundleError> {
        let text = std::fs::read_to_string(path).map_err(|error| BundleError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        Self::from_json(&text)
    }

    /// Writes the document to a file.
    pub fn save(&self, path: &Path) -> Result<(), BundleError> {
        std::fs::write(path, self.to_json()).map_err(|error| BundleError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
    }

    /// The catalog the document describes, which is what a build hands its connection code.
    pub fn catalog(&self) -> Result<VersionCatalog, BundleError> {
        let mut catalog = VersionCatalog::new();
        for version in &self.versions {
            catalog.add(version.profile()).map_err(BundleError::Store)?;
        }
        Ok(catalog)
    }

    /// How many versions the document carries.
    pub fn len(&self) -> usize {
        self.versions.len()
    }

    /// Whether the document carries no version.
    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }

    /// Bytes the compact document takes, to judge what a build carries in its binary.
    pub fn json_len(&self) -> usize {
        self.to_json().len()
    }
}

impl BundleVersion {
    /// Rebuilds the profile this version's data describes.
    pub fn profile(&self) -> VersionProfile {
        let mut packets = PacketTables::default();
        for flow in &self.flows {
            let table = packets.flow_mut(flow.state, flow.direction);
            for (name, id) in &flow.entries {
                table.insert(name.clone(), *id);
            }
        }
        let mut registries = Registries::default();
        for registry in &self.registries {
            let mut body = Registry {
                protocol_id: registry.protocol_id,
                default: registry.default.clone(),
                ..Default::default()
            };
            for (entry, id) in &registry.entries {
                body.entries.insert(entry.clone(), *id);
            }
            registries.insert(registry.name.clone(), body);
        }
        VersionProfile::new(self.facts.clone(), packets, registries)
    }
}
