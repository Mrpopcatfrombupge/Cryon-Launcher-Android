//! Fabric's intermediary class names, resolved: a Fabric jar says
//! `net/minecraft/class_2248` where the game says `Block`, and reading what
//! a mod extends means naming those classes. Intermediary is a stable
//! public mapping; its named side is Yarn (CC0, FabricMC and Quilt), so
//! `class_2248` reads as `net/minecraft/block/Block` and the census can say
//! what it is.
//!
//! Two sources, in order:
//! - `update` fetches the full mapping for a Minecraft version from
//!   `maven.fabricmc.net` (the `mergedv2` artifact of `net.fabricmc:yarn`,
//!   a tiny-v2 file whose columns are intermediary ↔ named) and caches it
//!   as `<work>/intermediary.json`, the way Modrinth's answers are cached
//!   (`modrinth::Cache`), so a rerun is offline and a new game version
//!   needs no rebuild.
//! - the built-in table (`EMBEDDED`) is that file's class lines, generated
//!   and shipped: tests and every offline run resolve against it.
//!
//! The names it hands back are Yarn's. They name the same classes Mojang's
//! do (`Block`, `BlockEntity`, `Screen`), and
//! [`crate::classes::kind_of_game_class`] knows both spellings.
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// The built-in table: every class line of the `mergedv2` mappings of
/// Yarn 1.20.1+build.10 and 1.21.1+build.3 — the games the mod jars in
/// service were built for — merged with the newer build winning. Source:
/// https://maven.fabricmc.net/net/fabricmc/yarn/ (tiny v2, columns
/// intermediary and named; Yarn's licence is CC0). Regenerate by keeping
/// the `c` lines of the two downloads' `mappings/mappings.tiny`, `named`
/// entries only.
const EMBEDDED: &str = include_str!("intermediary_classes.tiny");

/// What the fetched cache is named inside the work folder.
pub const CACHE_FILE: &str = "intermediary.json";

/// intermediary class name (`net/minecraft/class_2248`) → named class name
/// (`net/minecraft/block/Block`).
#[derive(Debug, Default, Clone)]
pub struct Map {
    named: BTreeMap<String, String>,
}

impl Map {
    pub fn len(&self) -> usize {
        self.named.len()
    }

    pub fn is_empty(&self) -> bool {
        self.named.is_empty()
    }

    /// The named name of an intermediary class, when this map knows it.
    pub fn resolve(&self, name: &str) -> Option<&str> {
        self.named.get(name).map(String::as_str)
    }

    /// Entries that name an intermediary class (`class_…`); the rest of a
    /// mapping (classes that shipped named) resolves by itself.
    pub fn insert(&mut self, intermediary: &str, named: &str) {
        if intermediary.contains("/class_") && !named.contains("/class_") {
            self.named.insert(intermediary.to_owned(), named.to_owned());
        }
    }

    /// Add `other`'s entries: a fetched mapping wins over the built-in
    /// table for the classes both name, so a newer game version's file
    /// replaces stale entries.
    pub fn merge(&mut self, other: &Map) {
        self.named.extend(other.named.clone());
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.named).unwrap_or_default()
    }

    pub fn from_json(text: &str) -> Map {
        let mut map = Map::default();
        if let Ok(Value::Object(entries)) = serde_json::from_str::<Value>(text) {
            for (inter, named) in entries {
                if let Some(named) = named.as_str() {
                    map.insert(&inter, named);
                }
            }
        }
        map
    }

    /// A previously cached fetch, when the folder holds one.
    pub fn load_cached(work: &Path) -> Map {
        std::fs::read_to_string(work.join(CACHE_FILE))
            .map(|t| Map::from_json(&t))
            .unwrap_or_default()
    }

    pub fn save(&self, work: &Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(work).map_err(|e| format!("{}: {e}", work.display()))?;
        let path = work.join(CACHE_FILE);
        std::fs::write(&path, self.to_json()).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }
}

/// The built-in table, parsed once per process.
pub fn embedded() -> &'static Map {
    static ONCE: OnceLock<Map> = OnceLock::new();
    ONCE.get_or_init(|| parse_tiny(EMBEDDED))
}

/// What a run reading `work` should resolve with: the built-in table, plus
/// the fetched mapping when the folder has one cached.
pub fn map_for(work: &Path) -> Map {
    let mut map = embedded().clone();
    map.merge(&Map::load_cached(work));
    map
}

/// Reads a tiny-v2 mapping's class lines, keeping the intermediary → named
/// pairs. Member lines (indented) and the official column are left alone:
/// this names classes, nothing else. A file whose header lists both columns
/// works whether or not it also has `official`.
pub fn parse_tiny(text: &str) -> Map {
    let mut map = Map::default();
    let mut inter = None;
    let mut named = None;
    for line in text.lines() {
        if line.starts_with('\t') {
            continue; // Members of the class above.
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let Some(first) = fields.first() else {
            continue;
        };
        match *first {
            "tiny" => {
                // tiny 2 0 <namespaces…>: the k-th namespace is field k+1
                // of a class line (the kind word takes the first).
                inter = fields
                    .iter()
                    .position(|f| *f == "intermediary")
                    .and_then(|p| p.checked_sub(2));
                named = fields
                    .iter()
                    .position(|f| *f == "named")
                    .and_then(|p| p.checked_sub(2));
            }
            "c" => {
                if let (Some(i), Some(n)) = (inter, named)
                    && fields.len() > i.max(n)
                {
                    let (i, n) = (fields[i], fields[n]);
                    if !i.is_empty() && !n.is_empty() {
                        map.insert(i, n);
                    }
                }
            }
            _ => {}
        }
    }
    map
}

/// The yarn build for `mc` that Fabric's meta lists first (the newest), and
/// where its `mergedv2` jar is.
fn mergedv2_url(meta: &Value, mc: &str) -> Result<String, String> {
    let version = meta
        .get(0)
        .and_then(|v| v["version"].as_str())
        .ok_or(format!("Fabric's meta lists no yarn build for {mc}"))?;
    Ok(format!(
        "https://maven.fabricmc.net/net/fabricmc/yarn/{}/yarn-{version}-mergedv2.jar",
        version.replace('+', "%2B")
    ))
}

/// Fetch the full intermediary ↔ named mapping for `mc` and cache it in
/// `dir` as [`CACHE_FILE`]. Network only; nothing in the default test suite
/// calls it.
pub fn update(dir: &Path, mc: &str) -> Result<usize, String> {
    let meta_url = format!("https://meta.fabricmc.net/v2/versions/yarn/{mc}");
    let meta = get(&meta_url)?;
    let jar_url = mergedv2_url(&meta, mc)?;
    let jar = get_bytes(&jar_url)?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(jar)).map_err(|e| e.to_string())?;
    let mut tiny = String::new();
    zip.by_name("mappings/mappings.tiny")
        .map_err(|e| format!("{jar_url}: no mappings inside ({e})"))?
        .read_to_string(&mut tiny)
        .map_err(|e| e.to_string())?;
    let map = parse_tiny(&tiny);
    let n = map.len();
    if n == 0 {
        return Err(format!("{jar_url}: no intermediary classes in it"));
    }
    map.save(dir)?;
    Ok(n)
}

fn get(url: &str) -> Result<Value, String> {
    let response = crate::net::agent(url)
        .get(url)
        .set("User-Agent", crate::modrinth::AGENT)
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let text = response.into_string().map_err(|e| format!("{url}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{url}: {e}"))
}

fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let response = crate::net::agent(url)
        .get(url)
        .set("User-Agent", crate::modrinth::AGENT)
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{url}: {e}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &str = "tiny\t2\t0\tofficial\tintermediary\tnamed\n\
        c\tdfy\tnet/minecraft/class_2248\tnet/minecraft/block/Block\n\
        \tm\t()V\ta\tmethod_9639\toffinalize\n\
        c\taaa\tnet/minecraft/class_9\tnet/minecraft/client/gui/screen/Screen\n";

    #[test]
    fn reads_the_class_lines_of_a_tiny_v2_mapping() {
        let map = parse_tiny(TINY);
        assert_eq!(map.len(), 2, "member lines are left out");
        assert_eq!(
            map.resolve("net/minecraft/class_2248"),
            Some("net/minecraft/block/Block")
        );
        // A class both sides name needs no resolution, and isn't kept.
        assert_eq!(map.resolve("net/minecraft/util/Hand"), None);
        // A header without an intermediary column yields nothing to resolve.
        assert!(parse_tiny("tiny\t2\t0\tofficial\tnamed\nc\ta\tnet/minecraft/A\n").is_empty());
    }

    #[test]
    fn the_built_in_table_names_the_classes_the_census_reads() {
        let map = embedded();
        for (inter, named) in [
            ("net/minecraft/class_2248", "net/minecraft/block/Block"),
            (
                "net/minecraft/class_2586",
                "net/minecraft/block/entity/BlockEntity",
            ),
            (
                "net/minecraft/class_437",
                "net/minecraft/client/gui/screen/Screen",
            ),
            ("net/minecraft/class_1792", "net/minecraft/item/Item"),
            (
                "net/minecraft/class_1703",
                "net/minecraft/screen/ScreenHandler",
            ),
            ("net/minecraft/class_1297", "net/minecraft/entity/Entity"),
        ] {
            assert_eq!(map.resolve(inter), Some(named), "{inter}");
        }
    }

    #[test]
    fn a_cached_fetch_replaces_the_built_in_entries() {
        let dir = std::env::temp_dir().join(format!("port-pack-inter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut fetched = Map::default();
        // A newer game version renamed the class; the fetch is the answer.
        fetched.insert("net/minecraft/class_2248", "net/minecraft/block/OtherBlock");
        fetched.save(&dir).unwrap();
        let map = map_for(&dir);
        assert_eq!(
            map.resolve("net/minecraft/class_2248"),
            Some("net/minecraft/block/OtherBlock")
        );
        // And the built-in table still covers what the fetch left out.
        assert_eq!(
            map.resolve("net/minecraft/class_2586"),
            Some("net/minecraft/block/entity/BlockEntity")
        );
        // A folder with no cache is just the built-in table.
        std::fs::remove_dir_all(&dir).unwrap();
        let map = map_for(&dir);
        assert_eq!(
            map.resolve("net/minecraft/class_2248"),
            Some("net/minecraft/block/Block")
        );
    }

    #[test]
    fn json_round_trips_the_table() {
        let mut map = Map::default();
        map.insert("net/minecraft/class_2248", "net/minecraft/block/Block");
        map.insert("net/minecraft/util/Hand", "net/minecraft/util/Hand");
        let back = Map::from_json(&map.to_json());
        assert_eq!(back.len(), 1, "only what names an intermediary class");
        assert_eq!(
            back.resolve("net/minecraft/class_2248"),
            Some("net/minecraft/block/Block")
        );
    }

    #[test]
    fn the_download_url_follows_fabric_meta() {
        let meta: Value = serde_json::from_str(
            r#"[{"gameVersion":"1.21.1","maven":"net.fabricmc:yarn:1.21.1+build.3","version":"1.21.1+build.3"}]"#,
        )
        .unwrap();
        assert_eq!(
            mergedv2_url(&meta, "1.21.1").unwrap(),
            "https://maven.fabricmc.net/net/fabricmc/yarn/1.21.1%2Bbuild.3/yarn-1.21.1+build.3-mergedv2.jar"
        );
        assert!(mergedv2_url(&serde_json::json!([]), "1.99").is_err());
    }
}
