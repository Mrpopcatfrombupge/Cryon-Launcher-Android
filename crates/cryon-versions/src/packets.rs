use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The protocol phase a packet belongs to.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Handshake,
    Status,
    Login,
    Configuration,
    Play,
}

/// Which way a packet travels.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Clientbound,
    Serverbound,
}

impl State {
    /// The states in the order a connection meets them.
    pub const ALL: [State; 5] = [
        State::Handshake,
        State::Status,
        State::Login,
        State::Configuration,
        State::Play,
    ];

    /// The data generator's key for this state.
    pub fn key(&self) -> &'static str {
        match self {
            State::Handshake => "handshake",
            State::Status => "status",
            State::Login => "login",
            State::Configuration => "configuration",
            State::Play => "play",
        }
    }

    /// The state a data generator key names, or None for one this build doesn't know. A new connection
    /// state means recompile, not a catalog that fails to load.
    pub fn from_key(key: &str) -> Option<State> {
        State::ALL.into_iter().find(|state| state.key() == key)
    }
}

impl Direction {
    /// Both directions.
    pub const ALL: [Direction; 2] = [Direction::Clientbound, Direction::Serverbound];

    /// The data generator's key for this direction.
    pub fn key(&self) -> &'static str {
        match self {
            Direction::Clientbound => "clientbound",
            Direction::Serverbound => "serverbound",
        }
    }

    /// The direction a data generator key names, or None for one this build doesn't know.
    pub fn from_key(key: &str) -> Option<Direction> {
        Direction::ALL
            .into_iter()
            .find(|direction| direction.key() == key)
    }
}

/// One state and direction's packets, addressed both by name and by the id put on the wire.
///
/// Ids stay sparse rather than dense: a version removes a packet without closing the gap it left, and a
/// translated id has to mean the number the other side expects, not our index.
#[derive(Debug, Clone, Default)]
pub struct FlowTable {
    names_by_id: Vec<Option<String>>,
    ids_by_name: BTreeMap<String, i32>,
}

impl FlowTable {
    /// Records a packet and its protocol id. A report that gives one id to two names is read as the later
    /// one winning, on both sides of the table, so a name and an id never disagree about each other.
    pub fn insert(&mut self, name: impl Into<String>, id: i32) {
        let name = name.into();
        if id >= 0 {
            let at = id as usize;
            if self.names_by_id.len() <= at {
                self.names_by_id.resize(at + 1, None);
            }
            if let Some(owner) = self.names_by_id[at].replace(name.clone()) {
                if owner != name {
                    self.release(id, &owner);
                }
            }
        }
        self.ids_by_name.insert(name, id);
    }

    /// Forgets a name that no longer owns this id.
    fn release(&mut self, id: i32, owner: &str) {
        if self.ids_by_name.get(owner) == Some(&id) {
            self.ids_by_name.remove(owner);
        }
    }

    /// The protocol id of a packet named `minecraft:intention`, or None if this version has no such packet.
    pub fn id(&self, name: &str) -> Option<i32> {
        self.ids_by_name.get(name).copied()
    }

    /// The packet an incoming id names, or None for an id this state and direction never sends.
    pub fn name(&self, id: i32) -> Option<&str> {
        let at = usize::try_from(id).ok()?;
        self.names_by_id.get(at)?.as_deref()
    }

    /// How many packets this state and direction has.
    pub fn len(&self) -> usize {
        self.ids_by_name.len()
    }

    /// Whether the flow holds nothing, which for a real version means its report was not read.
    pub fn is_empty(&self) -> bool {
        self.ids_by_name.is_empty()
    }

    /// Every packet as (name, id), in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, i32)> + '_ {
        self.ids_by_name
            .iter()
            .map(|(name, id)| (name.as_str(), *id))
    }

    /// Every packet as (id, name), in the order the wire numbers them.
    pub fn by_id(&self) -> impl Iterator<Item = (i32, &str)> + '_ {
        (0..self.names_by_id.len())
            .filter_map(|at| Some((at as i32, self.names_by_id[at].as_deref()?)))
    }
}

/// One version's packets: every state and direction it has a table for.
#[derive(Debug, Clone, Default)]
pub struct PacketTables {
    flows: BTreeMap<(State, Direction), FlowTable>,
}

impl PacketTables {
    /// The table for one state and direction, or None when this version sends nothing there.
    pub fn flow(&self, state: State, direction: Direction) -> Option<&FlowTable> {
        self.flows.get(&(state, direction))
    }

    /// The id one packet has in this version.
    pub fn id(&self, state: State, direction: Direction, name: &str) -> Option<i32> {
        self.flow(state, direction)?.id(name)
    }

    /// The packet one id names in this version.
    pub fn name(&self, state: State, direction: Direction, id: i32) -> Option<&str> {
        self.flow(state, direction)?.name(id)
    }

    /// The table for one state and direction, making it if absent.
    pub fn flow_mut(&mut self, state: State, direction: Direction) -> &mut FlowTable {
        self.flows.entry((state, direction)).or_default()
    }

    /// Every flow, in connection order.
    pub fn flows(&self) -> impl Iterator<Item = (State, Direction, &FlowTable)> {
        self.flows
            .iter()
            .map(|((state, direction), table)| (*state, *direction, table))
    }

    /// How many flows were read.
    pub fn len(&self) -> usize {
        self.flows.len()
    }

    /// Whether no packets were read at all.
    pub fn is_empty(&self) -> bool {
        self.flows.is_empty()
    }

    /// How many packets the version has across every flow.
    pub fn packet_count(&self) -> usize {
        self.flows.values().map(FlowTable::len).sum()
    }
}

/// Reads a data generator `packets.json`: `{state: {direction: {name: {"protocol_id": n}}}}`.
pub fn packets_from_value(value: &serde_json::Value) -> PacketTables {
    let mut tables = PacketTables::default();
    let Some(object) = value.as_object() else {
        return tables;
    };
    for (state_key, flows) in object {
        let Some(state) = State::from_key(state_key) else {
            continue;
        };
        let Some(flows) = flows.as_object() else {
            continue;
        };
        for (direction_key, packets) in flows {
            let Some(direction) = Direction::from_key(direction_key) else {
                continue;
            };
            let Some(packets) = packets.as_object() else {
                continue;
            };
            let table = tables.flow_mut(state, direction);
            for (name, entry) in packets {
                if let Some(id) = entry.get("protocol_id").and_then(serde_json::Value::as_i64) {
                    table.insert(name.clone(), id as i32);
                }
            }
        }
    }
    tables
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PacketTables {
        packets_from_value(&serde_json::json!({
            "play": {
                "clientbound": {"minecraft:add_entity": {"protocol_id": 1}, "minecraft:air": {"protocol_id": 0}},
                "serverbound": {"minecraft:chat": {"protocol_id": 7}},
                "sideways": {"minecraft:nonsense": {"protocol_id": 2}}
            },
            "status": {"clientbound": {"minecraft:pong_response": {"protocol_id": 1}}},
            "bogus_state": {"clientbound": {"minecraft:nope": {"protocol_id": 3}}}
        }))
    }

    #[test]
    fn ids_resolve_both_ways() {
        let tables = sample();
        let play = tables.flow(State::Play, Direction::Clientbound).unwrap();
        assert_eq!(play.id("minecraft:add_entity"), Some(1));
        assert_eq!(play.name(1), Some("minecraft:add_entity"));
        assert_eq!(play.name(0), Some("minecraft:air"));
        assert_eq!(play.name(9), None);
        assert_eq!(play.len(), 2);
        assert_eq!(
            tables.id(State::Play, Direction::Serverbound, "minecraft:chat"),
            Some(7)
        );
        assert_eq!(
            tables.name(State::Play, Direction::Serverbound, 7),
            Some("minecraft:chat")
        );
        assert_eq!(
            tables.name(State::Play, Direction::Clientbound, 7),
            None,
            "one id per flow, not global"
        );
    }

    #[test]
    fn an_unknown_state_or_direction_is_skipped_not_fatal() {
        let tables = sample();
        assert_eq!(
            tables.id(State::Play, Direction::Clientbound, "minecraft:add_entity"),
            Some(1)
        );
        assert_eq!(
            tables
                .flows()
                .map(|(state, direction, _)| (state, direction))
                .collect::<Vec<_>>(),
            vec![
                (State::Status, Direction::Clientbound),
                (State::Play, Direction::Clientbound),
                (State::Play, Direction::Serverbound),
            ],
            "bogus_state and sideways must not enter the table"
        );
    }

    #[test]
    fn sparse_ids_keep_the_wire_number() {
        let mut table = FlowTable::default();
        table.insert("minecraft:one", 40);
        table.insert("minecraft:two", 3);
        assert_eq!(table.name(40), Some("minecraft:one"));
        assert_eq!(
            table.by_id().collect::<Vec<_>>(),
            vec![(3, "minecraft:two"), (40, "minecraft:one")]
        );
    }

    #[test]
    fn a_reused_id_forgets_the_name_it_no_longer_names() {
        let mut table = FlowTable::default();
        table.insert("minecraft:swing", 4);
        table.insert("minecraft:punch", 4);
        assert_eq!(table.name(4), Some("minecraft:punch"));
        assert_eq!(
            table.id("minecraft:swing"),
            None,
            "a name whose id was taken must not linger"
        );
        assert_eq!(table.id("minecraft:punch"), Some(4));
        assert_eq!(table.len(), 1);
    }
}
