//! What a mod's Java code does, read from its class files without running
//! or decompiling them: each class's superclass, interfaces, declared
//! methods and the classes and annotations its constant pool names. The
//! mod's own classes are followed up to the first game class they extend,
//! and that class says what the code is: a block entity, a menu, an entity,
//! a block or item with behaviour of its own, a worldgen feature...
//!
//! Since 26.1 the game ships unobfuscated, and NeoForge has run on Mojang's
//! names since 1.20.5, so game classes and methods are read by name. Fabric
//! jars before 26.1 name them in intermediary (`net/minecraft/class_2248`)
//! and Forge's methods are SRG (`m_60503_`). The intermediary names are
//! resolved ([`crate::intermediary`]: `class_2248` is `Block`), so a Fabric
//! class is counted as what it extends; a class whose game superclass no
//! table names is [`Feature::Unread`].
use crate::intermediary;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// One class file, as much as this module reads.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Class {
    pub name: String,
    pub superclass: String,
    pub interfaces: Vec<String>,
    /// Declared methods, by name (constructors and lambdas left out).
    pub methods: Vec<String>,
    /// Every class the constant pool names (`net/minecraft/...`).
    pub refs: BTreeSet<String>,
    /// Annotation and descriptor strings (`Lorg/spongepowered/asm/mixin/Mixin;`).
    pub mixin: bool,
}

/// Reads a class file's header, constant pool and declared methods.
pub fn parse(bytes: &[u8]) -> Option<Class> {
    let mut r = Reader { bytes, at: 0 };
    if r.u4()? != 0xCAFE_BABE {
        return None;
    }
    r.skip(4)?; // minor, major
    let count = r.u2()? as usize;
    let mut utf8: HashMap<usize, String> = HashMap::new();
    let mut class_of: HashMap<usize, usize> = HashMap::new();
    let mut i = 1;
    while i < count {
        let tag = r.u1()?;
        match tag {
            1 => {
                let len = r.u2()? as usize;
                let s = r.take(len)?;
                utf8.insert(i, String::from_utf8_lossy(s).into_owned());
            }
            7 => {
                class_of.insert(i, r.u2()? as usize);
            }
            8 | 16 | 19 | 20 => r.skip(2)?,
            3 | 4 | 9..=12 | 17 | 18 => r.skip(4)?,
            15 => r.skip(3)?,
            5 | 6 => {
                r.skip(8)?;
                i += 1;
            }
            _ => return None,
        }
        i += 1;
    }
    let class_name = |idx: usize| -> String {
        class_of
            .get(&idx)
            .and_then(|u| utf8.get(u))
            .cloned()
            .unwrap_or_default()
    };
    r.skip(2)?; // access flags
    let name = class_name(r.u2()? as usize);
    let superclass = class_name(r.u2()? as usize);
    let n = r.u2()? as usize;
    let mut interfaces = Vec::with_capacity(n);
    for _ in 0..n {
        interfaces.push(class_name(r.u2()? as usize));
    }
    // Fields, then methods: flags, name, descriptor, attributes.
    let mut methods = Vec::new();
    for member in 0..2 {
        let n = r.u2()?;
        for _ in 0..n {
            r.skip(2)?;
            let name = r.u2()? as usize;
            r.skip(2)?;
            let attributes = r.u2()?;
            for _ in 0..attributes {
                r.skip(2)?;
                let len = r.u4()? as usize;
                r.skip(len)?;
            }
            if member == 1
                && let Some(m) = utf8.get(&name)
                && !m.starts_with('<')
                && !m.starts_with("lambda$")
            {
                methods.push(m.clone());
            }
        }
    }
    let refs = class_of
        .values()
        .filter_map(|u| utf8.get(u))
        .filter(|s| !s.starts_with('['))
        .cloned()
        .collect();
    let mixin = utf8
        .values()
        .any(|s| s == "Lorg/spongepowered/asm/mixin/Mixin;");
    Some(Class {
        name,
        superclass,
        interfaces,
        methods,
        refs,
        mixin,
    })
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.bytes.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(drop)
    }
    fn u1(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u2(&mut self) -> Option<u16> {
        let b = self.take(2)?;
        Some(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u4(&mut self) -> Option<u32> {
        let b = self.take(4)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// What a class does, as the port sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Feature {
    /// A block with behaviour of its own (use, ticks, collisions...).
    BlockBehaviour,
    /// A block with a shape of its own: it ports with its template's.
    BlockShape,
    /// An item with behaviour of its own (use, on hit...).
    ItemBehaviour,
    BlockEntity,
    Menu,
    Entity,
    Screen,
    Renderer,
    /// A worldgen feature, placement or structure type written in Java.
    Worldgen,
    /// A recipe type (its recipes don't craft) or ingredient type.
    Recipe,
    Effect,
    Enchantment,
    Particle,
    Command,
    Network,
    /// Reacts to game events (NeoForge's bus, Fabric's callbacks).
    Events,
    /// A Mixin: a patch to the game's own code.
    Mixin,
    /// Extends a game class the census has no feature for (its own data
    /// loader, explosion, loot function…) or, rarely, an intermediary
    /// number no table names. The line names what it extends.
    Unread,
}

impl Feature {
    /// How the coverage line names `n` of them.
    pub fn describe(self, n: usize) -> String {
        let (one, many) = match self {
            Feature::BlockBehaviour => (
                "block with behaviour of its own (use, ticks, collisions)",
                "blocks with behaviour of their own (use, ticks, collisions)",
            ),
            Feature::BlockShape => (
                "block with a shape of its own: it ports with its template's",
                "blocks with shapes of their own: they port with their templates'",
            ),
            Feature::ItemBehaviour => (
                "item with behaviour of its own (use, on hit)",
                "items with behaviour of their own (use, on hit)",
            ),
            Feature::BlockEntity => (
                "block entity (machine, container or animated block)",
                "block entities (machines, containers, animated blocks)",
            ),
            Feature::Menu => ("menu (a container screen)", "menus (container screens)"),
            Feature::Entity => ("mob or entity", "mobs or entities"),
            Feature::Screen => ("screen of its own", "screens of its own"),
            Feature::Renderer => ("renderer or model in code", "renderers or models in code"),
            Feature::Worldgen => (
                "worldgen feature or structure type in code",
                "worldgen features or structure types in code",
            ),
            Feature::Recipe => (
                "recipe type of its own (its recipes don't craft)",
                "recipe types of its own (their recipes don't craft)",
            ),
            Feature::Effect => ("status effect", "status effects"),
            Feature::Enchantment => ("enchantment effect in code", "enchantment effects in code"),
            Feature::Particle => ("particle type", "particle types"),
            Feature::Command => ("command", "commands"),
            Feature::Network => ("network message", "network messages"),
            Feature::Events => ("game event it reacts to", "game events it reacts to"),
            Feature::Mixin => (
                "Mixin (a patch to the game's code)",
                "Mixins (patches to the game's code)",
            ),
            Feature::Unread => (
                "class extending a game class with no feature of ours",
                "classes extending game classes with no feature of ours",
            ),
        };
        format!("{n} {}", if n == 1 { one } else { many })
    }
}

/// Block methods that only say how it looks or is placed: the template's
/// do the same for the states a blockstate file lists.
const BLOCK_LOOKS: [&str; 18] = [
    "codec",
    "createBlockStateDefinition",
    "getStateForPlacement",
    "rotate",
    "mirror",
    "getRenderShape",
    "skipRendering",
    "getShadeBrightness",
    "propagatesSkylightDown",
    "getCloneItemStack",
    "isPathfindable",
    "getFluidState",
    "updateShape",
    "canBeReplaced",
    "getLightBlock",
    "useShapeForLightOcclusion",
    "getDescriptionId",
    "animateTick",
];
/// Block methods that only change its outline or collision.
const BLOCK_SHAPES: [&str; 6] = [
    "getShape",
    "getCollisionShape",
    "getVisualShape",
    "getOcclusionShape",
    "getInteractionShape",
    "getBlockSupportShape",
];
/// Item methods that only say how it looks or is named.
const ITEM_LOOKS: [&str; 6] = [
    "appendHoverText",
    "getName",
    "isFoil",
    "getDescriptionId",
    "getBarColor",
    "getBarWidth",
];

/// The game's own, loader and library classes: the first of these a mod's
/// class chain reaches is what it extends.
fn external(name: &str, own: &BTreeMap<String, Class>) -> bool {
    !own.contains_key(name)
}

/// Whether `name` is a class a name table has to resolve: Fabric's
/// intermediary, top level of the game package.
fn is_intermediary(name: &str) -> bool {
    name.strip_prefix("net/minecraft/")
        .is_some_and(|rest| rest.starts_with("class_"))
}

/// What a named (Yarn) game class is, by its package. Yarn names the same
/// classes Mojang's does, under different packages: `block/Block` is
/// `world/level/block/Block`, `screen/ScreenHandler` is a menu.
fn kind_of_yarn_class(name: &str) -> Option<Feature> {
    let rest = name.strip_prefix("net/minecraft/")?;
    let (package, _) = rest.rsplit_once('/').unwrap_or(("", rest));
    Some(match package {
        "block/entity" => Feature::BlockEntity,
        p if p.starts_with("block") => Feature::BlockBehaviour,
        p if p.starts_with("item/recipe") || p.starts_with("recipe") => Feature::Recipe,
        p if p.starts_with("item") => Feature::ItemBehaviour,
        p if p.starts_with("enchantment") => Feature::Enchantment,
        p if p.starts_with("entity") => Feature::Entity,
        p if p.starts_with("screen") || p.starts_with("inventory") => Feature::Menu,
        p if p.starts_with("world/gen") || p.starts_with("structure") => Feature::Worldgen,
        p if p.starts_with("status") => Feature::Effect,
        p if p.starts_with("client/gui/screen") || p.starts_with("client/gui/widget") => {
            Feature::Screen
        }
        p if p.starts_with("client/render") || p.starts_with("client/model") => Feature::Renderer,
        p if p.starts_with("client/particle") || p.starts_with("particle") => Feature::Particle,
        p if p.starts_with("network/packet") => Feature::Network,
        _ => return None,
    })
}

/// What a game class is, by its package and name, under Mojang's names or,
/// once `class_2248` is resolved, Yarn's. An intermediary class no table
/// names stays unread.
fn kind_of_game_class(name: &str, map: &intermediary::Map) -> Option<Feature> {
    if is_intermediary(name) {
        return match map.resolve(name) {
            Some(named) => kind_of_yarn_class(named).or(Some(Feature::Unread)),
            // A number no table names: the class can't be read.
            None => Some(Feature::Unread),
        };
    }
    kind_of_mojang_class(name)
}

/// The name a person reads for a game class: `Block`, not the package.
/// Unresolved intermediary classes have no name to give.
fn simple_game_name(name: &str, map: &intermediary::Map) -> Option<String> {
    if !name.starts_with("net/minecraft/") {
        return None;
    }
    let named = if is_intermediary(name) {
        map.resolve(name)?
    } else {
        name
    };
    let simple = named.rsplit('/').next().unwrap_or(named);
    Some(simple.split('$').next().unwrap_or(simple).to_owned())
}

/// What a game class (Mojang's names) is, by its package and name.
fn kind_of_mojang_class(name: &str) -> Option<Feature> {
    let rest = name.strip_prefix("net/minecraft/")?;
    let (package, simple) = rest.rsplit_once('/').unwrap_or(("", rest));
    Some(match package {
        "world/level/block/entity" => Feature::BlockEntity,
        p if p.starts_with("world/level/block") => Feature::BlockBehaviour,
        "world/inventory" if simple.ends_with("Menu") => Feature::Menu,
        p if p.starts_with("world/entity") => Feature::Entity,
        p if p.starts_with("world/item/crafting") => Feature::Recipe,
        p if p.starts_with("world/item/enchantment") => Feature::Enchantment,
        p if p.starts_with("world/item") => Feature::ItemBehaviour,
        p if p.starts_with("world/level/levelgen") => Feature::Worldgen,
        "world/effect" => Feature::Effect,
        p if p.starts_with("client/gui/screens") || p.starts_with("client/gui/components") => {
            Feature::Screen
        }
        p if p.starts_with("client/renderer") || p.starts_with("client/model") => Feature::Renderer,
        p if p.starts_with("client/particle") || p.starts_with("core/particles") => {
            Feature::Particle
        }
        _ => return None,
    })
}

/// Events a content mod uses to register what it has: not behaviour.
fn registration_event(name: &str) -> bool {
    [
        "BuildCreativeModeTabContentsEvent",
        "RegisterEvent",
        "Register",
        "Lifecycle",
        "ModConfigEvent",
        "AddPackFindersEvent",
        "ItemGroupEvents",
        "ClientLifecycleEvents",
        "ServerLifecycleEvents",
    ]
    .iter()
    .any(|k| name.rsplit('/').next().unwrap_or(name).contains(k))
        || name.contains("/lifecycle/")
        || name.contains("/registries/")
        || name.contains("/registry/")
}

/// The code census: how many of each feature, and examples.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Code {
    pub classes: usize,
    pub features: BTreeMap<Feature, usize>,
    /// The game events it subscribes to, by simple name.
    pub events: BTreeSet<String>,
    /// Game classes the Mixins name — targets and what they touch — by
    /// simple name, once the table names them.
    #[serde(default)]
    pub mixin_targets: BTreeSet<String>,
    /// What the `Unread` classes extend: resolved simple names, or the
    /// `class_…` number when no table names it.
    #[serde(default)]
    pub unread_names: BTreeSet<String>,
    /// Blocks and items with behaviour, and the methods they override.
    pub examples: BTreeMap<String, Vec<String>>,
}

impl Code {
    /// Whether it does anything beyond registering blocks and items.
    pub fn behaviour(&self) -> bool {
        self.features
            .keys()
            .any(|f| *f != Feature::BlockShape && *f != Feature::Particle)
    }
}

/// Reads every class of a jar against the built-in intermediary table.
pub fn census<'a>(classes: impl IntoIterator<Item = &'a [u8]>) -> Code {
    census_with(classes, intermediary::embedded())
}

/// Reads every class of a jar. `classes` are the class files' bytes, `map`
/// names the intermediary classes they extend.
pub fn census_with<'a>(
    classes: impl IntoIterator<Item = &'a [u8]>,
    map: &intermediary::Map,
) -> Code {
    let own: BTreeMap<String, Class> = classes
        .into_iter()
        .filter_map(parse)
        .map(|c| (c.name.clone(), c))
        .collect();
    let mut code = Code {
        classes: own.len(),
        ..Code::default()
    };
    let mut count = |f: Feature| *code.features.entry(f).or_default() += 1;
    for c in own.values() {
        if c.mixin {
            count(Feature::Mixin);
            for r in &c.refs {
                if let Some(simple) = simple_game_name(r, map) {
                    code.mixin_targets.insert(simple);
                }
            }
            continue;
        }
        // The first class outside the jar the chain reaches, and the
        // methods declared along the way.
        let mut methods: BTreeSet<&str> = c.methods.iter().map(String::as_str).collect();
        let mut at = c;
        let mut seen = 0;
        while !external(&at.superclass, &own) && seen < 64 {
            at = &own[&at.superclass];
            methods.extend(at.methods.iter().map(String::as_str));
            seen += 1;
        }
        let base = at.superclass.as_str();
        let kind = kind_of_game_class(base, map).or_else(|| {
            c.interfaces.iter().find_map(|i| match i.as_str() {
                "net/minecraft/network/protocol/common/custom/CustomPacketPayload" => {
                    Some(Feature::Network)
                }
                "net/minecraft/world/MenuProvider" => Some(Feature::Menu),
                _ => kind_of_game_class(i, map),
            })
        });
        let Some(kind) = kind else {
            continue;
        };
        if kind == Feature::Unread && !methods.is_empty() {
            // Name what it extends, so the line tells the porting agent
            // even what we cannot map: a resolved class, or the number.
            let game = std::iter::once(base.to_owned())
                .chain(c.interfaces.iter().cloned())
                .find(|n| n.starts_with("net/minecraft/"));
            if let Some(n) = game {
                let shown = simple_game_name(&n, map)
                    .unwrap_or_else(|| n.rsplit('/').next().unwrap_or(&n).to_owned());
                code.unread_names.insert(shown);
            }
        }
        let feature = match kind {
            Feature::BlockBehaviour => {
                let behaviour: Vec<&str> = methods
                    .iter()
                    .copied()
                    .filter(|m| !BLOCK_LOOKS.contains(m) && !BLOCK_SHAPES.contains(m))
                    .collect();
                if !behaviour.is_empty() {
                    // The class extends a block and declares something of
                    // its own; methods named in intermediary or SRG are
                    // listed as named, unread but counted.
                    code.examples.insert(
                        c.name.rsplit('/').next().unwrap_or("").to_owned(),
                        behaviour.iter().map(|m| m.to_string()).collect(),
                    );
                    Some(Feature::BlockBehaviour)
                } else if methods.iter().any(|m| BLOCK_SHAPES.contains(m)) {
                    Some(Feature::BlockShape)
                } else {
                    None
                }
            }
            Feature::ItemBehaviour => {
                let behaviour: Vec<&str> = methods
                    .iter()
                    .copied()
                    .filter(|m| !ITEM_LOOKS.contains(m))
                    .collect();
                if behaviour.is_empty() {
                    None
                } else {
                    code.examples.insert(
                        c.name.rsplit('/').next().unwrap_or("").to_owned(),
                        behaviour.iter().map(|m| m.to_string()).collect(),
                    );
                    Some(Feature::ItemBehaviour)
                }
            }
            // A class extending an obfuscated game class without declaring
            // anything adds nothing to read.
            Feature::Unread if methods.is_empty() => None,
            k => Some(k),
        };
        if let Some(f) = feature {
            count(f);
        }
    }
    for c in own.values() {
        for r in &c.refs {
            let event = (r.starts_with("net/neoforged/neoforge/event/")
                || r.starts_with("net/minecraftforge/event/")
                || r.starts_with("net/fabricmc/fabric/api/event/")
                || r.starts_with("net/fabricmc/fabric/api/entity/event/"))
                && !registration_event(r);
            if event {
                let simple = r.rsplit('/').next().unwrap_or(r);
                code.events
                    .insert(simple.split('$').next().unwrap_or(simple).to_owned());
            }
            if r.starts_with("com/mojang/brigadier/")
                && !code.features.contains_key(&Feature::Command)
            {
                code.features.insert(Feature::Command, 1);
            }
        }
    }
    if !code.events.is_empty() {
        code.features.insert(Feature::Events, code.events.len());
    }
    code
}

#[cfg(test)]
pub(crate) mod tests_support {
    /// A class file with `name` extending `superclass`, declaring `methods`
    /// and naming `refs` in its constant pool.
    pub fn class_with(
        name: &str,
        superclass: &str,
        methods: &[&str],
        refs: &[&str],
        mixin: bool,
    ) -> Vec<u8> {
        let mut pool: Vec<Vec<u8>> = Vec::new();
        let utf = |pool: &mut Vec<Vec<u8>>, s: &str| -> u16 {
            let mut e = vec![1];
            e.extend((s.len() as u16).to_be_bytes());
            e.extend(s.as_bytes());
            pool.push(e);
            pool.len() as u16
        };
        let class = |pool: &mut Vec<Vec<u8>>, s: &str| -> u16 {
            let u = utf(pool, s);
            let mut e = vec![7];
            e.extend(u.to_be_bytes());
            pool.push(e);
            pool.len() as u16
        };
        let this = class(&mut pool, name);
        let sup = class(&mut pool, superclass);
        for r in refs {
            class(&mut pool, r);
        }
        // A long takes two slots.
        pool.push(vec![5, 0, 0, 0, 0, 0, 0, 0, 1]);
        pool.push(Vec::new());
        if mixin {
            utf(&mut pool, "Lorg/spongepowered/asm/mixin/Mixin;");
        }
        let desc = utf(&mut pool, "()V");
        let names: Vec<u16> = methods.iter().map(|m| utf(&mut pool, m)).collect();
        let mut out = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 65];
        out.extend(((pool.len() + 1) as u16).to_be_bytes());
        for e in &pool {
            out.extend(e);
        }
        out.extend([0, 0x21]);
        out.extend(this.to_be_bytes());
        out.extend(sup.to_be_bytes());
        out.extend([0, 0]); // interfaces
        out.extend([0, 0]); // fields
        out.extend((names.len() as u16).to_be_bytes());
        for n in names {
            out.extend([0, 1]);
            out.extend(n.to_be_bytes());
            out.extend(desc.to_be_bytes());
            // One attribute, to be skipped.
            out.extend([0, 1, 0, 1, 0, 0, 0, 2, 0, 0]);
        }
        out.extend([0, 0]); // class attributes
        out
    }

    /// A class extending `superclass` and declaring nothing.
    pub fn class_file(name: &str, superclass: &str) -> Vec<u8> {
        class_with(name, superclass, &[], &[], false)
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::class_with as class_file;
    use super::*;

    #[test]
    fn reads_the_header_methods_and_pool() {
        let bytes = class_file(
            "a/Lamp",
            "net/minecraft/world/level/block/Block",
            &["<init>", "useWithoutItem", "lambda$x$0"],
            &["net/minecraft/world/item/Item"],
            false,
        );
        let c = parse(&bytes).unwrap();
        assert_eq!(c.name, "a/Lamp");
        assert_eq!(c.superclass, "net/minecraft/world/level/block/Block");
        assert_eq!(c.methods, ["useWithoutItem"]);
        assert!(c.refs.contains("net/minecraft/world/item/Item"));
        assert!(!c.mixin);
        assert!(parse(&bytes[..bytes.len() - 3]).is_none());
        assert!(parse(b"not a class").is_none());
    }

    #[test]
    fn follows_own_classes_to_the_game() {
        let block = "net/minecraft/world/level/block/Block";
        let files = [
            // Looks only: a template covers it.
            class_file("m/Plain", block, &["getStateForPlacement"], &[], false),
            class_file("m/Round", block, &["getShape"], &[], false),
            // Behaviour two levels down.
            class_file("m/Base", block, &["randomTick"], &[], false),
            class_file("m/Lamp", "m/Base", &[], &[], false),
            class_file(
                "m/Tile",
                "net/minecraft/world/level/block/entity/BlockEntity",
                &[],
                &[],
                false,
            ),
            // A Mixin into a block: named by the table, even in intermediary.
            class_file(
                "m/Mix",
                "java/lang/Object",
                &[],
                &["net/minecraft/class_2248"],
                true,
            ),
            // Fabric intermediary: class_2248 is the game's Block, and a
            // class extending it is read as a block with behaviour even
            // while its methods are still named method_….
            class_file(
                "m/Old",
                "net/minecraft/class_2248",
                &["method_9534"],
                &[],
                false,
            ),
            // A number no table names stays unread.
            class_file(
                "m/Unknown",
                "net/minecraft/class_7777777",
                &["method_9534"],
                &[],
                false,
            ),
            class_file(
                "m/Events",
                "java/lang/Object",
                &[],
                &[
                    "net/neoforged/neoforge/event/entity/player/PlayerInteractEvent$RightClickBlock",
                    "net/neoforged/neoforge/event/BuildCreativeModeTabContentsEvent",
                    "net/neoforged/neoforge/registries/DeferredRegister",
                ],
                false,
            ),
            class_file(
                "m/Gem",
                "net/minecraft/world/item/Item",
                &["appendHoverText"],
                &[],
                false,
            ),
        ];
        let code = census(files.iter().map(Vec::as_slice));
        assert_eq!(code.classes, 10);
        let n = |f| code.features.get(&f).copied().unwrap_or(0);
        assert_eq!(n(Feature::BlockBehaviour), 3, "{code:?}");
        assert_eq!(n(Feature::BlockShape), 1);
        assert_eq!(n(Feature::BlockEntity), 1);
        assert_eq!(n(Feature::Mixin), 1);
        assert_eq!(n(Feature::Unread), 1);
        assert_eq!(n(Feature::ItemBehaviour), 0);
        assert_eq!(
            code.events.iter().collect::<Vec<_>>(),
            ["PlayerInteractEvent"]
        );
        assert_eq!(
            code.mixin_targets.iter().collect::<Vec<_>>(),
            ["Block"],
            "the Mixin's target class_2248 is named"
        );
        assert!(
            code.unread_names.contains("class_7777777"),
            "{:?}",
            code.unread_names
        );
        assert!(code.behaviour());
    }

    #[test]
    fn fabric_intermediary_classes_are_named() {
        let files = [
            class_file("m/Chest", "net/minecraft/class_2586", &[], &[], false),
            class_file("m/Gui", "net/minecraft/class_437", &["init"], &[], false),
            class_file("m/Menu", "net/minecraft/class_1703", &[], &[], false),
            class_file("m/Mob", "net/minecraft/class_1297", &[], &[], false),
            class_file("m/Recipe", "net/minecraft/class_1865", &[], &[], false),
            // Declares nothing and extends a class we can name: the census
            // says what it is, not that it can't be read.
        ];
        let code = census(files.iter().map(Vec::as_slice));
        let n = |f| code.features.get(&f).copied().unwrap_or(0);
        assert_eq!(n(Feature::BlockEntity), 1, "{code:?}");
        assert_eq!(n(Feature::Screen), 1);
        assert_eq!(n(Feature::Menu), 1);
        assert_eq!(n(Feature::Entity), 1);
        assert_eq!(n(Feature::Recipe), 1);
        assert_eq!(n(Feature::Unread), 0);
    }

    #[test]
    fn registration_only_is_not_behaviour() {
        let files = [
            class_file(
                "m/Mod",
                "java/lang/Object",
                &["register"],
                &[
                    "net/neoforged/neoforge/registries/DeferredRegister",
                    "net/minecraft/world/level/block/Block",
                ],
                false,
            ),
            class_file(
                "m/Pillar",
                "net/minecraft/world/level/block/RotatedPillarBlock",
                &[],
                &[],
                false,
            ),
        ];
        let code = census(files.iter().map(Vec::as_slice));
        assert!(!code.behaviour(), "{code:?}");
    }

    /// Reads real Fabric jars from disk with the built-in table: the four
    /// that used to be blind, and the big one. The jars are not in the
    /// repo; the folder is `PORT_PACK_JARS` or, failing that, the design
    /// folder next to this checkout. It skips quietly when they are not
    /// there, so CI never needs them.
    ///
    /// Run it with:
    /// `cargo test -p cryon-port-pack -- intermediary_names_real_jars -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn intermediary_names_real_jars() {
        use std::io::Read;
        let dir = std::env::var("PORT_PACK_JARS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                // rust-port/design/mods/jars, beside the horizon checkout.
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../design/mods/jars")
            });
        for name in [
            "comforts",
            "waystones",
            "supplementaries",
            "refined-storage",
            "botania",
        ] {
            let path = dir.join(format!("{name}.jar"));
            let Ok(file) = std::fs::File::open(&path) else {
                eprintln!("{name}: {} not found, skipped", path.display());
                continue;
            };
            let mut zip = zip::ZipArchive::new(file).unwrap();
            let mut bytes = Vec::new();
            for i in 0..zip.len() {
                let mut e = zip.by_index(i).unwrap();
                if e.name().is_ok_and(|n| n.ends_with(".class")) && e.size() < 1 << 24 {
                    let mut b = Vec::new();
                    if e.read_to_end(&mut b).is_ok() {
                        bytes.push(b);
                    }
                }
            }
            let code = census(bytes.iter().map(Vec::as_slice));
            let unread = code.features.get(&Feature::Unread).copied().unwrap_or(0);
            println!("{name}: {} classes, {unread} still unread", code.classes);
            for (f, n) in &code.features {
                println!("  {}", f.describe(*n));
            }
            // What is left: intermediary numbers the built-in table does not
            // know, i.e. classes a newer game added.
            let map = intermediary::embedded();
            for b in &bytes {
                if let Some(c) = parse(b)
                    && let Some(rest) = c.superclass.strip_prefix("net/minecraft/")
                    && rest.starts_with("class_")
                    && map.resolve(&c.superclass).is_none()
                    && !c.methods.is_empty()
                {
                    println!("  still unread: {}", c.superclass);
                }
            }
            let named = code
                .features
                .iter()
                .filter(|(f, _)| **f != Feature::Unread)
                .map(|(_, n)| n)
                .sum::<usize>();
            // Almost every class these jars extend is a game class the
            // table names; only numbers newer than the table are left.
            assert!(
                unread * 10 <= code.classes,
                "{name}: {unread} of {} still unread",
                code.classes
            );
            assert!(named > 0, "{name}: nothing read");
        }
    }
}
