use std::collections::BTreeMap;

use crate::packets::{Direction, State};
use crate::profile::VersionProfile;

/// A name and the id one version gives it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry {
    /// The packet or registry entry, `minecraft:swing_animation`.
    pub name: String,
    /// The number one version puts on the wire for it.
    pub id: i32,
}

/// One name's id moving between two versions.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Shift {
    /// The packet or entry.
    pub name: String,
    /// Its id in the version compared from.
    pub from: i32,
    /// Its id in the version compared to.
    pub to: i32,
}

/// One name's journey through a version pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Translation {
    /// What the id meant in the version it arrived under.
    pub name: String,
    /// The id asked about.
    pub from_id: i32,
    /// The id the other version uses for the same name, or None when it has no such entry: the packet or
    /// block cannot be passed on, and saying so beats guessing an id.
    pub to_id: Option<i32>,
}

impl Translation {
    /// Whether the other version knows this name at all.
    pub fn is_translatable(&self) -> bool {
        self.to_id.is_some()
    }
}

/// How one registry's protocol ids moved between two versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdDrift {
    /// The registry, `minecraft:block`.
    pub registry: String,
    /// The version compared from.
    pub from: String,
    /// The version compared to.
    pub to: String,
    /// Entries both versions number alike.
    pub kept: usize,
    /// Entries only the newer version has.
    pub added: Vec<Entry>,
    /// Entries only the older version had.
    pub removed: Vec<Entry>,
    /// Entries both have under different ids: what breaks a chunk palette, an entity type field or an item
    /// component that names ids rather than names.
    pub shifted: Vec<Shift>,
}

impl IdDrift {
    /// Whether both versions number this registry identically, which is what makes its ids safe to pass
    /// between them untouched.
    pub fn is_still_translatable(&self) -> bool {
        self.shifted.is_empty() && self.added.is_empty() && self.removed.is_empty()
    }
}

/// How one flow's packets moved between two versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketDrift {
    /// The protocol phase.
    pub state: State,
    /// Which way the packets travel.
    pub direction: Direction,
    /// The version compared from.
    pub from: String,
    /// The version compared to.
    pub to: String,
    /// Packets the newer version added, under its ids.
    pub added: Vec<Entry>,
    /// Packets the newer version no longer sends, under the older ids.
    pub removed: Vec<Entry>,
    /// Packets both send under different ids.
    pub renumbered: Vec<Shift>,
    /// Packets both send under the same id.
    pub kept: usize,
}

/// Two name-to-id tables, compared.
fn compare(
    old: &BTreeMap<String, i32>,
    new: &BTreeMap<String, i32>,
) -> (Vec<Entry>, Vec<Entry>, Vec<Shift>, usize) {
    let added = new
        .iter()
        .filter(|(name, _)| !old.contains_key(*name))
        .map(|(name, id)| Entry {
            name: name.clone(),
            id: *id,
        })
        .collect();
    let removed = old
        .iter()
        .filter(|(name, _)| !new.contains_key(*name))
        .map(|(name, id)| Entry {
            name: name.clone(),
            id: *id,
        })
        .collect();
    let mut shifted = Vec::new();
    let mut kept = 0;
    for (name, before) in old {
        match new.get(name) {
            Some(after) if after != before => shifted.push(Shift {
                name: name.clone(),
                from: *before,
                to: *after,
            }),
            Some(_) => kept += 1,
            None => {}
        }
    }
    shifted.sort();
    (added, removed, shifted, kept)
}

fn registry_table(profile: &VersionProfile, registry: &str) -> Option<BTreeMap<String, i32>> {
    Some(
        profile
            .registry(registry)?
            .entries
            .iter()
            .map(|(name, id)| (name.to_owned(), id))
            .collect(),
    )
}

/// Compares one registry's protocol ids between two versions, or None when neither has the registry.
///
/// Registry ids are what a chunk palette, an entity type field and an item component carry on the wire, so
/// a pair whose ids shifted cannot exchange those bytes directly: each id has to pass through its name,
/// which [`translate_id`] does.
pub fn registry_drift(
    from: &VersionProfile,
    to: &VersionProfile,
    registry: &str,
) -> Option<IdDrift> {
    let (old, new) = (
        registry_table(from, registry)?,
        registry_table(to, registry)?,
    );
    let (added, removed, shifted, kept) = compare(&old, &new);
    Some(IdDrift {
        registry: registry.to_owned(),
        from: from.id().to_owned(),
        to: to.id().to_owned(),
        kept,
        added,
        removed,
        shifted,
    })
}

/// The registries both versions have, which is what a comparison can cover.
pub fn shared_registries(from: &VersionProfile, to: &VersionProfile) -> Vec<String> {
    let mut names = from
        .registries()
        .names()
        .filter(|name| to.registries().get(name).is_some())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.into_iter().map(str::to_owned).collect()
}

fn packets_of(profile: &VersionProfile) -> BTreeMap<(State, Direction), BTreeMap<String, i32>> {
    let mut out: BTreeMap<(State, Direction), BTreeMap<String, i32>> = BTreeMap::new();
    for (state, direction, flow) in profile.packets().flows() {
        out.insert(
            (state, direction),
            flow.iter()
                .map(|(name, id)| (name.to_owned(), id))
                .collect(),
        );
    }
    out
}

/// Every flow's packet ids compared, in connection order. A flow only one version has is included, so a
/// new connection state shows up as a whole flow of additions rather than as silence.
///
/// A version whose reports carry no packet table (Mojang only writes one from 1.21) has nothing to compare,
/// and the answer is then an empty list: that means *unknown*, not *identical*. Check
/// [`has_packets`](VersionProfile::has_packets) before reading zeros as agreement.
pub fn packet_drift(from: &VersionProfile, to: &VersionProfile) -> Vec<PacketDrift> {
    if !from.has_packets() || !to.has_packets() {
        return Vec::new();
    }
    let (old, new) = (packets_of(from), packets_of(to));
    let mut keys = old.keys().chain(new.keys()).copied().collect::<Vec<_>>();
    keys.sort_unstable();
    keys.dedup();
    let empty = BTreeMap::new();
    keys.into_iter()
        .map(|(state, direction)| {
            let (a, b) = (
                old.get(&(state, direction)).unwrap_or(&empty),
                new.get(&(state, direction)).unwrap_or(&empty),
            );
            let (added, removed, renumbered, kept) = compare(a, b);
            PacketDrift {
                state,
                direction,
                from: from.id().to_owned(),
                to: to.id().to_owned(),
                added,
                removed,
                renumbered,
                kept,
            }
        })
        .collect()
}

/// Resolves an id in one version to the same entry's id in another, through its name.
///
/// None means the source version has no such id at all, which a connection must learn before it writes a
/// byte it cannot justify.
pub fn translate_id(
    from: &VersionProfile,
    to: &VersionProfile,
    registry: &str,
    id: i32,
) -> Option<Translation> {
    let name = from.entry_name(registry, id)?.to_owned();
    Some(Translation {
        to_id: to.entry_id(registry, &name),
        from_id: id,
        name,
    })
}

/// Resolves a packet id of one version's state and direction to the same packet's id in the other's.
pub fn translate_packet(
    from: &VersionProfile,
    to: &VersionProfile,
    state: State,
    direction: Direction,
    id: i32,
) -> Option<Translation> {
    let name = from.packet_name(state, direction, id)?.to_owned();
    Some(Translation {
        to_id: to.packet_id(state, direction, &name),
        from_id: id,
        name,
    })
}
