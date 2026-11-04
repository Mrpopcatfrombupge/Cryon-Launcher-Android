use std::collections::BTreeMap;

/// One registry's entries, addressed by name (`minecraft:acacia_button`) and by the protocol id the wire
/// sends. Both indexes are kept in step, so a name and an id can never disagree about each other.
#[derive(Debug, Clone, Default)]
pub struct RegistryEntries {
    names_by_id: Vec<Option<String>>,
    ids_by_name: BTreeMap<String, i32>,
}

impl RegistryEntries {
    /// Records an entry and its protocol id; a second entry taking an id evicts the first from both sides.
    pub fn insert(&mut self, name: impl Into<String>, id: i32) {
        let name = name.into();
        if id >= 0 {
            let at = id as usize;
            if self.names_by_id.len() <= at {
                self.names_by_id.resize(at + 1, None);
            }
            if let Some(owner) = self.names_by_id[at].replace(name.clone()) {
                if owner != name && self.ids_by_name.get(&owner) == Some(&id) {
                    self.ids_by_name.remove(&owner);
                }
            }
        }
        self.ids_by_name.insert(name, id);
    }

    /// The protocol id of an entry, or None when this version has no entry of that name.
    pub fn id(&self, name: &str) -> Option<i32> {
        self.ids_by_name.get(name).copied()
    }

    /// The entry an id names, or None for an id outside this registry.
    pub fn name(&self, id: i32) -> Option<&str> {
        let at = usize::try_from(id).ok()?;
        self.names_by_id.get(at)?.as_deref()
    }

    /// How many entries the registry has.
    pub fn len(&self) -> usize {
        self.ids_by_name.len()
    }

    /// Whether the registry is empty, which for `minecraft:block` means its report was not read.
    pub fn is_empty(&self) -> bool {
        self.ids_by_name.is_empty()
    }

    /// Every entry as (name, id), in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, i32)> + '_ {
        self.ids_by_name
            .iter()
            .map(|(name, id)| (name.as_str(), *id))
    }

    /// Every entry as (id, name), in the order the wire numbers them.
    pub fn by_id(&self) -> impl Iterator<Item = (i32, &str)> + '_ {
        (0..self.names_by_id.len())
            .filter_map(|at| Some((at as i32, self.names_by_id[at].as_deref()?)))
    }
}

/// One registry of one version: its own protocol id, its default entry and its entries.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    /// The registry's own sync id, which the configuration state sends when it hands a client the registry.
    pub protocol_id: Option<i32>,
    /// The entry the registry falls back to, `minecraft:air` for blocks.
    pub default: Option<String>,
    pub entries: RegistryEntries,
}

/// Every registry of one version, keyed as the report keys them (`minecraft:block`).
#[derive(Debug, Clone, Default)]
pub struct Registries {
    by_name: BTreeMap<String, Registry>,
}

impl Registries {
    /// One registry by name.
    pub fn get(&self, name: &str) -> Option<&Registry> {
        self.by_name.get(name)
    }

    /// Adds or replaces a registry.
    pub fn insert(&mut self, name: impl Into<String>, registry: Registry) {
        self.by_name.insert(name.into(), registry);
    }

    /// Every registry's name, in the order the report lists them.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(String::as_str)
    }

    /// Every registry as (name, registry).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Registry)> {
        self.by_name
            .iter()
            .map(|(name, registry)| (name.as_str(), registry))
    }

    /// How many registries the version has: 95 in 26.3.
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// Whether no registry was read.
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// The protocol id an entry of a named registry has in this version.
    pub fn entry_id(&self, registry: &str, entry: &str) -> Option<i32> {
        self.get(registry)?.entries.id(entry)
    }

    /// The entry an id names in a registry.
    pub fn entry_name(&self, registry: &str, id: i32) -> Option<&str> {
        self.get(registry)?.entries.name(id)
    }
}

/// Reads a data generator `registries.json`: `{registry: {default, protocol_id, entries: {name:
/// {"protocol_id": n}}}}`. Anything a version writes differently is skipped rather than fatal, since a
/// catalog is meant to read several versions with one parser.
pub fn registries_from_value(value: &serde_json::Value) -> Registries {
    let mut registries = Registries::default();
    let Some(object) = value.as_object() else {
        return registries;
    };
    for (name, body) in object {
        let Some(body) = body.as_object() else {
            continue;
        };
        let mut registry = Registry {
            protocol_id: body
                .get("protocol_id")
                .and_then(serde_json::Value::as_i64)
                .map(|id| id as i32),
            default: body
                .get("default")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            entries: RegistryEntries::default(),
        };
        if let Some(entries) = body.get("entries").and_then(serde_json::Value::as_object) {
            for (entry, record) in entries {
                if let Some(id) = record
                    .get("protocol_id")
                    .and_then(serde_json::Value::as_i64)
                {
                    registry.entries.insert(entry.clone(), id as i32);
                }
            }
        }
        registries.insert(name.clone(), registry);
    }
    registries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Registries {
        registries_from_value(&serde_json::json!({
            "minecraft:block": {
                "default": "minecraft:air",
                "protocol_id": 3,
                "entries": {"minecraft:air": {"protocol_id": 0}, "minecraft:stone": {"protocol_id": 1}}
            },
            "minecraft:item": {"entries": {"minecraft:air": {"protocol_id": 0}}},
            "minecraft:splash": "not an object"
        }))
    }

    #[test]
    fn entries_resolve_both_ways() {
        let registries = sample();
        assert_eq!(
            registries.entry_id("minecraft:block", "minecraft:stone"),
            Some(1)
        );
        assert_eq!(
            registries.entry_name("minecraft:block", 1),
            Some("minecraft:stone")
        );
        assert_eq!(
            registries.entry_name("minecraft:item", 1),
            None,
            "ids belong to one registry"
        );
        assert_eq!(
            registries.len(),
            2,
            "a malformed registry is skipped, not fatal"
        );
    }

    #[test]
    fn reads_the_registry_metadata() {
        let block = sample().get("minecraft:block").cloned().unwrap();
        assert_eq!(block.protocol_id, Some(3));
        assert_eq!(block.default.as_deref(), Some("minecraft:air"));
        assert_eq!(block.entries.len(), 2);
        let item = sample().get("minecraft:item").cloned().unwrap();
        assert_eq!(item.protocol_id, None);
        assert_eq!(item.default, None);
    }

    #[test]
    fn an_evicted_entry_leaves_both_indexes() {
        let mut entries = RegistryEntries::default();
        entries.insert("minecraft:old", 9);
        entries.insert("minecraft:new", 9);
        assert_eq!(entries.name(9), Some("minecraft:new"));
        assert_eq!(entries.id("minecraft:old"), None);
        assert_eq!(entries.iter().count(), 1);
    }

    #[test]
    fn a_hole_from_a_removed_entry_keeps_the_other_ids() {
        let mut entries = RegistryEntries::default();
        entries.insert("minecraft:a", 0);
        entries.insert("minecraft:b", 1);
        entries.insert("minecraft:c", 2);
        entries.insert("minecraft:d", 1);
        assert_eq!(
            entries.by_id().collect::<Vec<_>>(),
            vec![(0, "minecraft:a"), (1, "minecraft:d"), (2, "minecraft:c")]
        );
        assert_eq!(
            entries.id("minecraft:b"),
            None,
            "the entry whose id was taken leaves both indexes"
        );
        assert_eq!(
            entries.id("minecraft:c"),
            Some(2),
            "an entry keeps its id when a neighbour is displaced"
        );
    }
}
