//! Minecraft versions as runtime data instead of compile-time constants.
//!
//! Cryon pins one version: `cryon_core::version` holds `pub const PROTOCOL_VERSION`, and every site that
//! needs a version reads it, so a build speaks exactly one protocol and rejects every other. This crate is
//! the other shape. A [`VersionProfile`] holds one Minecraft version's packet ids, registries and facts,
//! loaded from that version's data generator reports; a [`VersionCatalog`] holds many of them together and
//! [negotiates](VersionCatalog::negotiate) the protocol number a client announced. One binary then carries
//! many versions, and a new release is an entry added to the catalog rather than a table regenerated into
//! the source.
//!
//! The catalog also translates between versions, because their numeric ids are not stable: `minecraft:air`
//! is one protocol id in 26.2 and another in 26.4. [`translate`] resolves an id through its name into the
//! id another version gives it, which is what a connection shared between versions needs.
//!
//! A build can carry the profiles as data instead of reading a version store at run time:
//! [`ProfileBundle`] exports every catalog entry to one compact document, which `include_str!` pulls into
//! the binary and [`ProfileBundle::catalog`] reads back.
//!
//! ```no_run
//! use std::path::Path;
//! let catalog = cryon_versions::VersionCatalog::open(Path::new("/path/to/version/store")).unwrap();
//! let profile = catalog.negotiate(777).unwrap();
//! assert_eq!(profile.id(), "26.3");
//! ```
//!
//! Reading is deliberately tolerant of fields a report gains or loses: a catalog that spans versions must
//! not fail to load a new one because its data generator started writing a key the older ones did not have.

mod bundle;
mod catalog;
mod facts;
mod packets;
mod profile;
mod registry;
mod store;
mod translate;

pub use bundle::{BundleError, BundleFlow, BundleRegistry, BundleVersion, FORMAT, ProfileBundle};
pub use catalog::{Negotiation, VersionCatalog};
pub use facts::Facts;
pub use packets::{Direction, FlowTable, PacketTables, State, packets_from_value};
pub use profile::VersionProfile;
pub use registry::{Registries, Registry, RegistryEntries, registries_from_value};
pub use store::{Prepared, StoreError, read_prepared};
pub use translate::{
    Entry, IdDrift, PacketDrift, Shift, Translation, packet_drift, registry_drift,
    shared_registries, translate_id, translate_packet,
};
