//! One mod jar on its own, as the launcher's mod browser installs it: what
//! of it Cryon runs, said before install (Works, Partly, Doesn't run),
//! from the jar itself. The jar is carried over ([`crate::carry`]) into a
//! folder content mod, and its classes are read ([`crate::classes`]) for
//! what the carry can't take: block entities, entities, Mixins, behaviour.
//!
//! - **Works**: content only. Its blocks, items, recipes, loot, tags and
//!   worldgen port, and its code only registers them.
//! - **Partly**: the content ports; the code features listed don't.
//! - **Doesn't run**: nothing to port (everything it does is Java code), or
//!   the jar can't be carried at all.
use crate::{carry, classes, intermediary, jar};
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, fs, io::Read, path::Path};

/// Bound both compressed input and expanded data before the converter reads it.
fn preflight(path: &Path) -> Result<(), String> {
    const FILE_LIMIT: u64 = 16 << 20;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 128 << 20 {
        return Err("The jar exceeds the 128 MiB import limit".into());
    }
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if zip.len() > 20_000 { return Err("The jar contains more than 20,000 entries".into()); }
    let (mut total, mut classes) = (0u64, 0u64);
    let mut names = BTreeSet::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().map_err(|e| e.to_string())?.into_owned();
        let plain = name.trim_end_matches('/');
        if plain.is_empty() || plain.contains(['\\', ':', '\0'])
            || plain.split('/').any(|p| p.is_empty() || p == "." || p == "..")
            || !names.insert(plain.to_ascii_lowercase())
            || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err(format!("Unsafe, duplicate or symlink entry: {name:?}"));
        }
        total = total.checked_add(entry.size()).ok_or("Archive size overflow")?;
        if name.ends_with(".class") { classes += entry.size(); }
        if entry.size() > FILE_LIMIT || total > 256 << 20 || classes > 64 << 20 {
            return Err("Archive exceeds entry (16 MiB), total (256 MiB), or class (64 MiB) limit".into());
        }
        if !entry.is_dir() {
            let size = entry.size();
            let read = std::io::copy(&mut entry.by_ref().take(FILE_LIMIT + 1), &mut std::io::sink())
                .map_err(|e| format!("{name}: {e}"))?;
            if read != size || read > FILE_LIMIT { return Err(format!("Invalid entry length: {name}")); }
        }
    }
    Ok(())
}

fn valid_mod_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && !["minecraft", "cryon", "c"].contains(&id)
}

/// Import exactly the checked content. New imports are disabled and existing installs are never overwritten.
pub fn import_one(jar: &Path, out: &Path, game: &Path, resources: Option<&Path>) -> Result<Check, String> {
    if out.exists() { return Err("Import work folder already exists; choose a fresh folder".into()); }
    let check = check_with_resources(jar, out, resources)?;
    if check.verdict == Verdict::No { return Ok(check); }
    let carry = out.join("carry");
    let package = cryon_mods::Package::read(&carry)?;
    if package.wasm.is_some() { return Err("Imported Java code must not be executable".into()); }
    let content = cryon_mods::Content::read(&carry, &package.manifest)?;
    if content.is_empty() { return Err("The checked carrier has no usable content".into()); }
    let mods = game.join("cryon-mods");
    if mods.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("The mods directory must not be a symlink".into());
    }
    fs::create_dir_all(&mods).map_err(|e| e.to_string())?;
    let target = mods.join(&check.mod_id);
    if target.symlink_metadata().is_ok() { return Err(format!("{} is already installed; no files replaced", check.mod_id)); }
    // Work folders are under the selected instance so the publication is one filesystem rename.
    fs::rename(&carry, &target).map_err(|e| format!("Cannot publish checked content: {e}"))?;
    // Keep the report with the installed content and leave cryon-mods.txt unchanged (off by default).
    if let Err(e) = fs::copy(out.join("check.json"), target.join("port-report.json")) {
        let _ = fs::rename(&target, &carry);
        return Err(format!("Cannot preserve compatibility report: {e}"));
    }
    // Keep the mod's own Java beside the port, for the behaviour runtime: the
    // on-device dexer reads this jar, not the original download. It is inert
    // bytes — nothing here executes it; only the behaviour runtime ever dexes
    // it, and the report has already told the player what its code can and
    // cannot do. Best effort: a mod without it simply has no behaviour to run.
    let behavior = target.join("behavior");
    if fs::create_dir_all(&behavior).is_ok() {
        if let Err(e) = fs::copy(jar, behavior.join("source.jar")) {
            let _ = fs::remove_dir(&behavior);
            log::warn!("{}: could not keep the Java for the behaviour runtime: {e}", check.mod_id);
        }
    }
    Ok(check)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Works,
    Partly,
    No,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Works => "Works",
            Verdict::Partly => "Partly",
            Verdict::No => "Doesn't run",
        }
    }
}

/// What checking a jar found. Written as `check.json` beside the carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub mod_id: String,
    pub name: String,
    pub version: String,
    /// `neoforge`, `forge`, `fabric`, `quilt`, or `` for a plain zip.
    pub loader: String,
    pub license: String,
    pub verdict: Verdict,
    /// One line: why this verdict.
    pub summary: String,
    /// What ports: "12 blocks", "40 recipes"...
    pub ports: Vec<String>,
    /// What doesn't, with why.
    pub skipped: Vec<String>,
    /// Required mods, by id.
    pub depends: Vec<String>,
    /// The folder mod's namespaces (its id first): two mods on at once may
    /// not share one.
    pub namespaces: Vec<String>,
    pub code: classes::Code,
}

/// The loader a jar is made for, from its metadata file.
fn loader(names: &[String]) -> &'static str {
    let has = |n: &str| names.iter().any(|x| x == n);
    if has("META-INF/neoforge.mods.toml") {
        "neoforge"
    } else if has("META-INF/mods.toml") {
        "forge"
    } else if has("fabric.mod.json") {
        "fabric"
    } else if has("quilt.mod.json") {
        "quilt"
    } else {
        ""
    }
}

/// Data folders named as before 1.21 (`recipes/`, `loot_tables/`) and none
/// as 1.21 names them: the game doesn't read them, and the carry upgrades
/// from 1.21.1 on. (Some jars keep a stale plural folder beside the new one.)
fn old_layout(names: &[String]) -> bool {
    let has = |kinds: [&str; 2]| {
        names.iter().any(|n| {
            let p: Vec<&str> = n.split('/').collect();
            p.len() > 3 && p[0] == "data" && kinds.contains(&p[2]) && n.ends_with(".json")
        })
    };
    has(["recipes", "loot_tables"]) && !has(["recipe", "loot_table"])
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Checks `jar` and carries it over into `out/carry/` (`out/check.json`,
/// `out/carry-report.json` beside it). The carry is the folder mod an
/// install copies.
pub fn check(jar_path: &Path, out: &Path) -> Result<Check, String> {
    check_with_resources(jar_path, out, None)
}

pub fn check_with_resources(jar_path: &Path, out: &Path, resources: Option<&Path>) -> Result<Check, String> {
    preflight(jar_path)?;
    let info = jar::read(jar_path)?;
    if !valid_mod_id(&info.mod_id) { return Err("Mod id must be non-reserved [a-z0-9_]{1,64}".into()); }
    let allowed: Option<BTreeSet<String>> = resources.map(|root| {
        let facts: serde_json::Value = crate::pipeline::read_json(&root.join("facts.json"))?;
        let defaults = facts["defaults"].as_object().ok_or("Prepared facts.json has no defaults")?;
        Ok::<_, String>(defaults.keys().cloned().collect())
    }).transpose()?;
    let file = fs::File::open(jar_path).map_err(|e| format!("{}: {e}", jar_path.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", jar_path.display()))?;
    let names: Vec<String> = zip
        .file_names()
        .map(|name| name.map(|name| name.into_owned()))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{}: {e}", jar_path.display()))?;
    let mut bytes = Vec::new();
    for n in names.iter().filter(|n| n.ends_with(".class")) {
        let mut b = Vec::new();
        if let Ok(mut e) = zip.by_name(n)
            && e.size() < 1 << 24
            && e.read_to_end(&mut b).is_ok()
        {
            bytes.push(b);
        }
    }
    // Intermediary names resolve against the built-in table, and against a
    // fetched mapping cached in this folder (`intermediary::update`) when
    // the run has one. Reading the jar itself never needs the network.
    let code = classes::census_with(bytes.iter().map(Vec::as_slice), &intermediary::map_for(out));
    let mut check = Check {
        mod_id: info.mod_id.clone(),
        name: info.name.clone(),
        version: info.version.clone(),
        loader: loader(&names).to_owned(),
        license: info.license.clone(),
        verdict: Verdict::No,
        summary: String::new(),
        ports: Vec::new(),
        skipped: Vec::new(),
        depends: info.depends.clone(),
        namespaces: Vec::new(),
        code,
    };
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let done = |check: Check| -> Result<Check, String> {
        crate::pipeline::write_json(&out.join("check.json"), &check)?;
        Ok(check)
    };
    if old_layout(&names) {
        check.summary = "Made for Minecraft 1.20 or older: Cryon ports mods made for 1.21 \
                         and later."
            .into();
        return done(check);
    }
    let alone = |m: &str| m == info.mod_id || m == "minecraft";
    let pack = carry::Pack {
        loaded: &alone,
        mods: &alone,
    };
    let report = match carry::carry(
        jar_path,
        &info,
        out,
        None,
        &|id| allowed.as_ref().is_none_or(|known| known.contains(id)),
        &pack,
        &BTreeMap::new(),
    ) {
        Ok(r) => r,
        Err(e) => {
            check.summary = format!("Can't be carried over: {e}");
            return done(check);
        }
    };
    check.namespaces = std::iter::once(info.mod_id.clone())
        .chain(report.namespaces.iter().cloned())
        .collect();
    let c = &info.census;
    let recipes = report
        .recipes_upgraded
        .saturating_sub(report.recipes_unregistered.len());
    let loot = c.loot_tables + report.loot_tables_added;
    let package = cryon_mods::Package::read(&out.join("carry"))?;
    let verified = cryon_mods::Content::read(&out.join("carry"), &package.manifest)?;
    let content = verified.files.len();
    // Fabric can register built-in packs from Java. Their files are real data,
    // but choosing or flattening them would guess at activation and overlays.
    let embedded_pack_files = names.iter().filter(|name| {
        (name.starts_with("resourcepacks/") || name.starts_with("datapacks/"))
            && (name.contains("/data/") || name.contains("/assets/"))
            && !name.ends_with('/')
    }).count();
    for (n, one, many) in [
        (report.blocks.len(), "block", "blocks"),
        (report.items, "item", "items"),
        (recipes, "recipe", "recipes"),
        (loot, "loot table", "loot tables"),
        (report.worldgen, "worldgen file", "worldgen files"),
        (c.textures, "texture", "textures"),
    ] {
        if n > 0 {
            check.ports.push(plural(n, one, many));
        }
    }
    if !report.blocks.is_empty() {
        check
            .ports
            .push("blocks take the closest vanilla block's shape, hardness and sounds".into());
    }
    let mut skipped = Vec::new();
    if !report.blocks_unknown.is_empty() {
        skipped.push(plural(
            report.blocks_unknown.len(),
            "block with no vanilla block to copy (left out)",
            "blocks with no vanilla block to copy (left out)",
        ));
    }
    if !report.recipes_unregistered.is_empty() {
        skipped.push(plural(
            report.recipes_unregistered.len(),
            "recipe for a left-out block",
            "recipes for left-out blocks",
        ));
    }
    if !report.recipes_kept.is_empty() {
        skipped.push(plural(
            report.recipes_kept.len(),
            "recipe of a type Cryon doesn't have",
            "recipes of types Cryon doesn't have",
        ));
    }
    if !report.unreadable.is_empty() {
        skipped.push(plural(
            report.unreadable.len(),
            "file the game can't read",
            "files the game can't read",
        ));
    }
    let wg = &report.worldgen_types;
    let dropped = wg.processors_dropped.values().sum::<usize>();
    if dropped > 0 {
        skipped.push(plural(
            dropped,
            "structure processor written in Java",
            "structure processors written in Java",
        ));
    }
    let unsupported = wg.unsupported.values().sum::<usize>();
    if unsupported > 0 {
        skipped.push(plural(
            unsupported,
            "worldgen file of a type written in Java",
            "worldgen files of types written in Java",
        ));
    }
    for (f, n) in &check.code.features {
        let mut line = f.describe(*n);
        let listed = if *f == classes::Feature::Events {
            Some(&check.code.events)
        } else if *f == classes::Feature::Mixin {
            Some(&check.code.mixin_targets)
        } else if *f == classes::Feature::Unread {
            Some(&check.code.unread_names)
        } else {
            None
        };
        if let Some(names) = listed {
            let shown: Vec<&str> = names.iter().take(3).map(String::as_str).collect();
            if !shown.is_empty() {
                line.push_str(&format!(" ({}", shown.join(", ")));
                if names.len() > 3 {
                    line.push_str(", ...");
                }
                line.push(')');
            }
        }
        skipped.push(line);
    }
    if info.census.classes > 0 {
        skipped.push(format!("{} Java classes are not executed; callbacks and behavior need a manual native port", info.census.classes));
    }
    if !report.blocks.is_empty() || report.items > 0 {
        skipped.push("Block/item templates approximate vanilla behavior; Java behavior parity is not verified".into());
    }
    if !check.depends.is_empty() {
        skipped.push(format!("Required dependencies are not executed or automatically resolved: {}", check.depends.join(", ")));
    }
    if !report.outside.is_empty() || !report.items_unknown.is_empty() {
        skipped.push(format!("{} out-of-namespace files and {} unknown items were not imported", report.outside.len(), report.items_unknown.len()));
    }
    if names.iter().any(|n| n.ends_with(".jar") || n.ends_with(".accesswidener") || n.contains("mixin")) {
        skipped.push("Nested jars, access wideners and mixins are not executed".into());
    }
    if embedded_pack_files > 0 {
        skipped.push(format!("{embedded_pack_files} embedded resource/data-pack files were not activated; pack selection and conditional overlays require a manual port"));
    }
    check.skipped = skipped;
    let behaviour = check.code.behaviour();
    (check.verdict, check.summary) = if content == 0 {
        let what = if embedded_pack_files > 0 {
            "its data is inside embedded packs that this importer does not activate; Java code and conditional overlays are not executed"
        } else if behaviour || check.code.classes > 0 {
            "everything it does is Java code, which Cryon doesn't run"
        } else {
            "it has no blocks, items, recipes or worldgen"
        };
        (Verdict::No, format!("Nothing to port: {what}."))
    } else if check.skipped.is_empty() {
        (
            Verdict::Works,
            format!(
                "Content only: {} port.",
                check.ports[..check.ports.len().min(3)].join(", ")
            ),
        )
    } else {
        (
            Verdict::Partly,
            format!(
                "{} port; not {}.",
                check.ports[..check.ports.len().min(2)].join(", "),
                check.skipped[..check.skipped.len().min(2)].join(", ")
            ),
        )
    };
    done(check)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn jar(dir: &Path, files: &[(&str, &[u8])]) -> std::path::PathBuf {
        let path = dir.join("m.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for (name, bytes) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    const TOML: &[u8] = br#"modLoader = "javafml"
license = "MIT"
[[mods]]
modId = "ruby"
version = "1.0"
displayName = "Ruby"
"#;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("port-pack-check-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn templates_and_code_are_explicitly_partial() {
        let dir = temp("content");
        let blockstate = br#"{"variants": {"": {"model": "ruby:block/ruby_block"}}}"#;
        let recipe = br#"{"type": "minecraft:crafting_shapeless", "ingredients": ["minecraft:red_dye"], "result": {"id": "ruby:ruby"}}"#;
        let plain = [
            ("META-INF/neoforge.mods.toml", TOML),
            ("assets/ruby/blockstates/ruby_block.json", blockstate),
            ("assets/ruby/models/item/ruby.json", b"{}"),
            ("data/ruby/recipe/ruby.json", recipe),
        ];
        let c = check(&jar(&dir, &plain), &dir.join("out")).unwrap();
        assert_eq!(c.verdict, Verdict::Partly, "{c:?}");
        assert!(c.skipped.iter().any(|s| s.contains("approximate")));
        assert_eq!(c.loader, "neoforge");
        assert_eq!(&c.ports[..3], ["1 block", "1 item", "1 recipe"]);
        assert!(dir.join("out/carry/cryon-mod.toml").is_file());
        assert!(dir.join("out/check.json").is_file());

        let tile = crate::classes::tests_support::class_file(
            "ruby/Tile",
            "net/minecraft/world/level/block/entity/BlockEntity",
        );
        let mut coded = plain.to_vec();
        coded.push(("ruby/Tile.class", &tile));
        let c = check(&jar(&dir, &coded), &dir.join("out")).unwrap();
        assert_eq!(c.verdict, Verdict::Partly);
        assert!(
            c.skipped[0].starts_with("1 block entity"),
            "{:?}",
            c.skipped
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn safe_single_import_is_disabled_and_never_overwrites() {
        let dir = temp("single-import");
        let source = jar(&dir, &[
            ("fabric.mod.json", br#"{"id":"sample","name":"Sample","version":"1","license":"MIT"}"#),
            ("data/sample/tags/item/test.json", br#"{"values":["minecraft:stone"]}"#),
        ]);
        let game = dir.join("game");
        let report = import_one(&source, &dir.join("work"), &game, None).unwrap();
        assert_eq!(report.verdict, Verdict::Works);
        let installed = game.join("cryon-mods/sample");
        let package = cryon_mods::Package::read(&installed).unwrap();
        assert_eq!(cryon_mods::Content::read(&installed, &package.manifest).unwrap().files.len(), 1);
        assert!(installed.join("port-report.json").is_file());
        assert!(!game.join("cryon-mods.txt").exists());
        fs::write(installed.join("sentinel"), "keep").unwrap();
        assert!(import_one(&source, &dir.join("second"), &game, None).is_err());
        assert_eq!(fs::read_to_string(installed.join("sentinel")).unwrap(), "keep");
        assert!(import_one(&source, &dir.join("work"), &game, None).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hostile_or_reserved_inputs_never_install() {
        let dir = temp("unsafe-import");
        let hostile = jar(&dir, &[("../escape", b"no")]);
        assert!(import_one(&hostile, &dir.join("bad"), &dir.join("game"), None).is_err());
        assert!(!dir.join("bad").exists());
        let reserved = jar(&dir, &[("fabric.mod.json", br#"{"id":"minecraft","version":"1"}"#)]);
        assert!(check(&reserved, &dir.join("reserved")).is_err());
        assert!(!dir.join("game/cryon-mods").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn prepared_templates_are_a_real_fence() {
        let dir = temp("fence");
        let resources = dir.join("resources");
        fs::create_dir_all(&resources).unwrap();
        fs::write(resources.join("facts.json"), r#"{"defaults":{"minecraft:dirt":0}}"#).unwrap();
        let source = jar(&dir, &[
            ("fabric.mod.json", br#"{"id":"sample","version":"1"}"#),
            ("assets/sample/blockstates/stone.json", br#"{"variants":{"":{"model":"sample:block/stone"}}}"#),
        ]);
        let report = check_with_resources(&source, &dir.join("out"), Some(&resources)).unwrap();
        assert!(report.skipped.iter().any(|s| s.contains("no vanilla block")));
        assert!(!dir.join("out/carry/data/sample/cryon/block/stone.json").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn recipes_for_left_out_blocks_are_left_out_too() {
        let dir = temp("left-out");
        let odd = br#"{"variants": {"wobble=1": {"model": "ruby:block/odd"}}}"#;
        let plain = br#"{"variants": {"": {"model": "ruby:block/ruby_block"}}}"#;
        let recipe = |result: &str| {
            format!(
                r#"{{"type": "minecraft:crafting_shapeless", "ingredients": ["minecraft:red_dye"], "result": {{"id": "{result}"}}}}"#
            )
        };
        let (odd_recipe, block_recipe) = (recipe("ruby:odd"), recipe("ruby:ruby_block"));
        let files = [
            ("META-INF/neoforge.mods.toml", TOML),
            ("assets/ruby/blockstates/odd.json", odd.as_slice()),
            ("assets/ruby/blockstates/ruby_block.json", plain.as_slice()),
            ("data/ruby/recipe/odd.json", odd_recipe.as_bytes()),
            ("data/ruby/recipe/ruby_block.json", block_recipe.as_bytes()),
        ];
        let c = check(&jar(&dir, &files), &dir.join("out")).unwrap();
        assert_eq!(c.verdict, Verdict::Partly);
        assert!(c.ports.contains(&"1 recipe".to_string()), "{:?}", c.ports);
        assert!(
            c.skipped
                .contains(&"1 recipe for a left-out block".to_string()),
            "{:?}",
            c.skipped
        );
        assert!(!dir.join("out/carry/data/ruby/recipe/odd.json").exists());
        assert!(dir.join("out/outside/data/ruby/recipe/odd.json").exists());
        assert!(
            dir.join("out/carry/data/ruby/recipe/ruby_block.json")
                .exists()
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn embedded_packs_are_reported_without_claiming_they_contain_only_code() {
        let dir = temp("embedded-pack");
        let source = jar(&dir, &[
            ("fabric.mod.json", br#"{"id":"terrain","version":"1","license":"MIT"}"#),
            ("resourcepacks/terrain/pack.mcmeta", br#"{"pack":{"pack_format":48,"description":"Terrain"}}"#),
            ("resourcepacks/terrain/data/terrain/worldgen/density_function/test.json", br#"{"type":"minecraft:constant","argument":1.0}"#),
        ]);
        let game = dir.join("game");
        let report = import_one(&source, &dir.join("out"), &game, None).unwrap();
        assert_eq!(report.verdict, Verdict::No);
        assert!(report.summary.contains("embedded packs"), "{}", report.summary);
        assert!(!report.summary.contains("everything it does is Java code"));
        assert!(report.skipped.iter().any(|s| s.contains("1 embedded resource/data-pack files")));
        assert!(!game.join("cryon-mods/terrain").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn code_only_and_old_layouts_dont_run() {
        let dir = temp("code");
        let tile = crate::classes::tests_support::class_file("ruby/Mixin", "java/lang/Object");
        let c = check(
            &jar(
                &dir,
                &[
                    ("META-INF/neoforge.mods.toml", TOML),
                    ("ruby/A.class", &tile),
                ],
            ),
            &dir.join("out"),
        )
        .unwrap();
        assert_eq!(c.verdict, Verdict::No);
        assert!(c.summary.starts_with("Nothing to port"), "{}", c.summary);
        let c = check(
            &jar(
                &dir,
                &[
                    ("META-INF/mods.toml", TOML),
                    ("data/ruby/recipes/ruby.json", b"{}"),
                ],
            ),
            &dir.join("out"),
        )
        .unwrap();
        assert_eq!(c.verdict, Verdict::No);
        assert!(c.summary.contains("1.20 or older"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
