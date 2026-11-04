//! The steps on a work folder, as the command line and the launcher run
//! them. Each one reads what the earlier steps wrote and says what it did
//! through `log`, one line at a time.
use crate::{carry, fetch, import, inventory, modrinth, oracle, report};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

/// Where a step's lines go: the terminal, or the launcher's log.
pub type Log<'a> = &'a dyn Fn(String);

pub fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Step 1: the pack at `pack` to `<work>/pack.json`.
pub fn import(source: &Path, work: &Path, log: Log) -> Result<import::Pack, String> {
    let pack = import::import(source)?;
    fs::create_dir_all(work).map_err(|e| format!("{}: {e}", work.display()))?;
    // A bad resource-pack entry must not stop the pack's mods from importing.
    match crate::resourcepacks::stage(source, work, &pack).and_then(|r| r.save(work)) {
        Ok(()) => {}
        Err(e) => log(format!("resource packs not staged: {e}")),
    }
    write_json(&work.join("pack.json"), &pack)?;
    let mut kinds = BTreeMap::<&str, usize>::new();
    for f in &pack.files {
        *kinds.entry(f.kind()).or_default() += 1;
    }
    log(format!(
        "{} ({}): Minecraft {}, {} {}; files {kinds:?}; overrides {:?}",
        pack.name, pack.format, pack.minecraft, pack.loader, pack.loader_version, pack.overrides
    ));
    Ok(pack)
}

/// Step 2: `<work>/inventory.json` and `inventory.csv`. Online, Modrinth is
/// asked about the files it has not answered for yet; when it cannot be
/// reached, the cached answers and the reviewed table classify alone.
pub fn inventory(work: &Path, offline: bool, log: Log) -> Result<Vec<inventory::Record>, String> {
    let pack: import::Pack = read_json(&work.join("pack.json"))?;
    let cache_path = work.join("modrinth.json");
    let mut cache = modrinth::Cache::load(&cache_path);
    if !offline {
        let hashes: Vec<String> = pack.files.iter().filter_map(|f| f.sha1.clone()).collect();
        match cache.fetch(&hashes) {
            Ok(()) => write_json(&cache_path, &cache)?,
            Err(e) => log(format!("Modrinth not reached, classifying offline: {e}")),
        }
    }
    let records = inventory::build(&pack, &cache, work);
    write_json(&work.join("inventory.json"), &records)?;
    fs::write(work.join("inventory.csv"), inventory::csv(&records))
        .map_err(|e| format!("inventory.csv: {e}"))?;
    let jars = records.iter().filter(|r| r.jar.is_some()).count();
    log(format!("{} files, {jars} jars read", records.len()));
    for (title, field) in [
        (
            "class",
            (|r: &inventory::Record| r.class.as_str()) as fn(&inventory::Record) -> &str,
        ),
        ("tier", |r| r.tier.as_str()),
        ("confidence", |r| r.confidence.as_str()),
        ("source", |r| r.source.as_str()),
    ] {
        log(format!("{title}: {:?}", inventory::counts(&records, field)));
    }
    let layers = records.iter().map(|r| r.layer).max().map_or(0, |m| m + 1);
    let per_layer: Vec<usize> = (0..layers)
        .map(|l| records.iter().filter(|r| r.layer == l).count())
        .collect();
    log(format!("layers: {per_layer:?}"));
    let missing: usize = records.iter().map(|r| r.missing.len()).sum();
    if missing > 0 {
        log(format!(
            "{missing} required dependencies are not in the pack (see inventory.json)"
        ));
    }
    Ok(records)
}

/// The vanilla blocks a prepared resource root has (`facts.json`'s defaults).
fn vanilla_blocks(resources: &Path) -> Result<BTreeSet<String>, String> {
    let facts: serde_json::Value = read_json(&resources.join("facts.json"))?;
    let defaults = facts["defaults"]
        .as_object()
        .ok_or("facts.json: no defaults")?;
    Ok(defaults.keys().cloned().collect())
}

/// Step 4: the content mods with a jar (or the ones in `only`) carried over
/// into `<work>/mods/<id>/carry/`. Returns how many were.
pub fn carry(
    work: &Path,
    resources: Option<&Path>,
    only: &[String],
    log: Log,
) -> Result<usize, String> {
    let records: Vec<inventory::Record> = read_json(&work.join("inventory.json"))?;
    let facts_path = work.join("oracle/facts.json");
    let facts = if facts_path.exists() {
        let mut f = oracle::Facts::read(&facts_path)?;
        if let Some(r) = resources {
            f.vanilla_items = read_json(&r.join("items.json"))
                .inspect_err(|e| log(format!("items patched without the game's tables: {e}")))
                .unwrap_or_default();
        }
        log(format!(
            "oracle facts: {} blocks, {} items",
            f.blocks.len(),
            f.items.len()
        ));
        Some(f)
    } else {
        None
    };
    // Without the prepared tables, templates are not limited to what the game has.
    let vanilla = resources.and_then(|r| {
        vanilla_blocks(r)
            .inspect_err(|e| log(format!("templates not limited to the game's blocks: {e}")))
            .ok()
    });
    let allowed = |b: &str| vanilla.as_ref().is_none_or(|v| v.contains(b));
    let conventions = crate::worldgen::loader_biome_tags(work);
    let chosen = |r: &inventory::Record| {
        if only.is_empty() {
            r.class == "content"
        } else {
            only.contains(&r.key)
        }
    };
    // The mods ported along (carried now or before), for the conditions of
    // the mods' worldgen modifiers.
    let ported: BTreeSet<&str> = records
        .iter()
        .filter(|r| {
            r.jar.is_some() && (chosen(r) || work.join("mods").join(&r.key).join("carry").is_dir())
        })
        .filter_map(|r| Some(r.info.as_ref()?.mod_id.as_str()))
        .chain(["minecraft"])
        .collect();
    let loaded = |m: &str| ported.contains(m);
    // Every mod of the pack, ported or not: a namespace one of them has as
    // its id is never another mod's.
    let in_pack: BTreeSet<&str> = records
        .iter()
        .flat_map(|r| {
            [
                Some(r.key.as_str()),
                r.info.as_ref().map(|i| i.mod_id.as_str()),
            ]
        })
        .flatten()
        .collect();
    // A namespace two carried mods both claim (a library's they both use, such as YUNG's API's
    // `yungsapi`, whose own jar isn't read) is neither's: either would refuse the other in the
    // game. Its files stay outside both.
    let claims = records.iter().filter(|r| chosen(r)).filter_map(|r| {
        let (jar, info) = (r.jar.as_ref()?, r.info.as_ref()?);
        let owned = carry::namespaces_claimed(jar, &info.mod_id, &|m| in_pack.contains(m)).ok()?;
        Some((r.key.as_str(), owned))
    });
    let shared = shared_namespaces(claims, log);
    let in_pack = |m: &str| in_pack.contains(m) || shared.contains(m);
    let pack = carry::Pack {
        loaded: &loaded,
        mods: &in_pack,
    };
    let mut done = 0;
    for r in &records {
        let chosen = chosen(r);
        let (Some(jar), Some(info)) = (&r.jar, &r.info) else {
            if chosen && !only.is_empty() {
                log(format!(
                    "{}: no jar (put it in {}/jars/)",
                    r.key,
                    work.display()
                ));
            }
            continue;
        };
        if !chosen {
            continue;
        }
        // The oracle's facts for the mods it ran; a mod it did not load (a
        // library missing) keeps the guesses from its blockstates.
        let prefix = format!("{}:", info.mod_id);
        let seen = |f: &&oracle::Facts| {
            f.blocks
                .keys()
                .chain(f.items.keys())
                .any(|k| k.starts_with(&prefix))
        };
        match carry::carry(
            jar,
            info,
            &work.join("mods").join(&r.key),
            facts.as_ref().filter(seen),
            &allowed,
            &pack,
            &conventions,
        ) {
            Ok(report) => {
                done += 1;
                log(format!(
                    "{}: {} files, {} recipes upgraded ({} kept), {} loot tables upgraded ({} added), {} item definitions added, {} blocks ({} left out, {} with differences), {} items ({} left out), {} convention tags, {} outside",
                    r.key,
                    report.copied,
                    report.recipes_upgraded,
                    report.recipes_kept.len(),
                    report.loot_tables_upgraded,
                    report.loot_tables_added,
                    report.item_definitions_added,
                    report.blocks.len(),
                    report.blocks_unknown.len(),
                    report.differences.len(),
                    report.items,
                    report.items_unknown.len(),
                    report.convention_tags,
                    report.outside.len()
                ));
            }
            Err(e) => log(format!("{}: {e}", r.key)),
        }
    }
    log(format!(
        "{done} mods carried over into {}",
        work.join("mods").display()
    ));
    Ok(done)
}

/// Which carried-over mods (`mods/<key>/carry` folders) the game loads together, in the order
/// given: as the game does, the first of two owning one namespace stays on. Returns the ones on,
/// and a line for each one left off.
pub fn loadable(mods: &[std::path::PathBuf]) -> (Vec<std::path::PathBuf>, Vec<String>) {
    let mut on: Vec<(std::path::PathBuf, String, Vec<String>)> = Vec::new();
    let mut off = Vec::new();
    for path in mods {
        let manifest = fs::read_to_string(path.join(cryon_mods::manifest::MANIFEST))
            .map_err(|e| e.to_string())
            .and_then(|t| cryon_mods::Manifest::parse(&t));
        let m = match manifest {
            Ok(m) => m,
            Err(e) => {
                off.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        let owned: Vec<String> = std::iter::once(m.id.clone())
            .chain(m.namespaces.iter().cloned())
            .collect();
        let clash = on
            .iter()
            .find_map(|(_, id, ns)| cryon_mods::content::clash((id, ns), (&m.id, &owned)));
        match clash {
            Some(why) => off.push(format!("{} not loaded: {why}", m.id)),
            None => on.push((path.clone(), m.id, owned)),
        }
    }
    (on.into_iter().map(|(p, _, _)| p).collect(), off)
}

/// The namespaces more than one mod claims, from each mod's (key, claimed namespaces).
fn shared_namespaces<'a>(
    claims: impl Iterator<Item = (&'a str, Vec<String>)>,
    log: Log,
) -> BTreeSet<String> {
    let mut by_namespace: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for (key, owned) in claims {
        for ns in owned {
            by_namespace.entry(ns).or_default().push(key);
        }
    }
    by_namespace
        .into_iter()
        .filter(|(_, mods)| mods.len() > 1)
        .map(|(ns, mods)| {
            log(format!(
                "namespace {ns} is claimed by {}: left to none of them",
                mods.join(", ")
            ));
            ns
        })
        .collect()
}

/// The pack's title as the report heads it.
pub fn title(pack: &import::Pack) -> String {
    format!(
        "{} ({} {}, Minecraft {})",
        pack.name, pack.loader, pack.loader_version, pack.minecraft
    )
}

/// Step 7: `<work>/coverage.json` and `coverage.md`.
pub fn report(work: &Path, log: Log) -> Result<Vec<report::Row>, String> {
    let pack: import::Pack = read_json(&work.join("pack.json"))?;
    let records: Vec<inventory::Record> = read_json(&work.join("inventory.json"))?;
    let rows = report::build(&records, work);
    write_json(&work.join("coverage.json"), &rows)?;
    let title = title(&pack);
    fs::write(work.join("coverage.md"), report::markdown(&title, &rows))
        .map_err(|e| format!("coverage.md: {e}"))?;
    log(format!(
        "{title}: {} files: {:?}",
        rows.len(),
        report::counts(&rows)
    ));
    Ok(rows)
}

/// What a player's machine can port today without an agent: content mods
/// of the data tier (carry-over only).
pub fn portable(r: &inventory::Record) -> bool {
    r.class == "content" && r.tier == "data"
}

/// Everything after the import, for the portable mods: their jars (`fetch`,
/// from `from` when the player has them), the inventory again with the jars
/// read, the carry-over and the report. `resources` is the prepared
/// version the ports will play on.
pub fn port(
    work: &Path,
    resources: Option<&Path>,
    from: Option<&Path>,
    log: Log,
) -> Result<(fetch::Fetched, Vec<report::Row>), String> {
    let records = match read_json::<Vec<inventory::Record>>(&work.join("inventory.json")) {
        Ok(r) => r,
        Err(_) => inventory(work, false, log)?,
    };
    crate::resourcepacks::fetch(work, log)?;
    // A single jar the player pointed at, rather than a pack they installed.
    let single = read_json::<import::Pack>(&work.join("pack.json"))
        .map(|pack| pack.format == "single")
        .unwrap_or(false);
    // The same rule for the fetch and the carry: a jar the fetch leaves out is a
    // mod the carry then reports as having no jar.
    let take = |r: &inventory::Record| portable(r) || (single && r.class == "content");
    let fetched = fetch::fetch(work, &records, from, &take, log)?;
    // The jars tell classes and tiers better than metadata alone.
    let records = inventory(work, true, log)?;
    let only: Vec<String> = records
        .iter()
        .filter(|r| take(r) && r.jar.is_some())
        .map(|r| r.key.clone())
        .collect();
    if only.is_empty() {
        log("no portable mod has its jar: nothing to carry over".into());
    } else {
        if single {
            // One file the player chose. A modpack run leaves a mod with behaviour of
            // its own out, because half a tech mod is worse than none; a person who
            // pointed at one jar wants what can be taken, plus the truth about the rest.
            for key in &only {
                if let Some(r) = records.iter().find(|r| &r.key == key) {
                    if r.tier != "data" {
                        log(format!(
                            "{key} has behaviour of its own ({}): its blocks, items, recipes, loot, tags, models and textures carry over; its code does not, until a native mod is written for it",
                            r.tier
                        ));
                    }
                }
            }
        }
        carry(work, resources, &only, log)?;
    }
    Ok((fetched, report(work, log)?))
}

/// The folder and the switch file the game reads, beside its `options.txt`
/// (`cryon-client-render/src/mod_list.rs`).
pub const MODS_DIR: &str = "cryon-mods";
pub const LIST_FILE: &str = "cryon-mods.txt";
/// Where the game looks for resource packs, and the key that says which are on
/// (`options_file.rs:198`: `resourcePacks:["vanilla","file/x.zip"]`).
pub const PACKS_DIR: &str = "resourcepacks";
pub const PACKS_KEY: &str = "resourcePacks";

/// Step 6, install: what the work folder holds becomes the game folder's own.
/// Each `<work>/mods/<id>/carry` is copied to `<game>/cryon-mods/<id>` and turned
/// on in `cryon-mods.txt`; each `<work>/resourcepacks/<file>` is copied to
/// `<game>/resourcepacks/` and named in `options.txt`. A ported jar and a dropped
/// pack both end up something the game loads on the next start, which is what the
/// player means by "where is it".
pub fn install(work: &Path, game: &Path, log: Log) -> Result<usize, String> {
    let mut done = 0;
    let src = work.join("mods");
    if src.is_dir() {
        let out = game.join(MODS_DIR);
        fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        let mut ids: Vec<String> = Vec::new();
        for entry in fs::read_dir(&src)
            .map_err(|e| format!("{}: {e}", src.display()))?
            .flatten()
        {
            let carried = entry.path().join("carry");
            if !carried.join(cryon_mods::manifest::MANIFEST).is_file() {
                continue;
            }
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let target = out.join(&id);
            copy_tree(&carried, &target)?;
            log(format!("installed {id} → {}", target.display()));
            ids.push(id);
        }
        if !ids.is_empty() {
            enable(game, &ids, log)?;
            done += ids.len();
        }
    }
    let staged = work.join(PACKS_DIR);
    if staged.is_dir() {
        let out = game.join(PACKS_DIR);
        fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        for entry in fs::read_dir(&staged)
            .map_err(|e| format!("{}: {e}", staged.display()))?
            .flatten()
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            fs::copy(&path, out.join(&name)).map_err(|e| format!("{name}: {e}"))?;
            let id = format!("file/{name}");
            enable_pack(game, &id, log)?;
            log(format!("installed pack {id}"));
            done += 1;
        }
    }
    if done == 0 {
        return Err("nothing was carried over, so there is nothing to install".into());
    }
    Ok(done)
}

/// Add one pack to `resourcePacks` in `options.txt`, keeping the order the game
/// reads (bottom first) and never listing a pack twice.
fn enable_pack(game: &Path, id: &str, log: Log) -> Result<(), String> {
    let path = game.join("options.txt");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let entry = format!("{PACKS_KEY}:");
    let want = format!("\"{id}\"");
    if let Some(line) = lines.iter_mut().find(|l| l.starts_with(&entry)) {
        let list = &line[entry.len()..];
        if list.contains(&want) {
            return Ok(());
        }
        let joined = match list.strip_suffix(']') {
            // `["vanilla"]` becomes `["vanilla","file/x.zip"]`.
            Some(inner) if inner.ends_with('[') => format!("{inner}{want}]"),
            Some(inner) => format!("{inner},{want}]"),
            None => format!("[{want}]"),
        };
        *line = format!("{entry}{joined}");
    } else {
        lines.push(format!("{entry}[\"vanilla\",{want}]"));
    }
    fs::write(&path, format!("{}\n", lines.join("\n")))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    log(format!("enabled {id} in {}", path.display()));
    Ok(())
}

/// A recursive copy. The carried folder is small JSON and NBT, and the game reads
/// it back as a folder mod, so nothing here needs to be clever.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in fs::read_dir(from)
        .map_err(|e| format!("{}: {e}", from.display()))?
        .flatten()
    {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
    }
    Ok(())
}

/// `mod.<id>:true` lines, keeping whatever the file already said. Every mod is off
/// until the player turns it on, so a port that is installed is also switched on —
/// otherwise the honest answer to "where is my mod" is a settings screen.
fn enable(game: &Path, ids: &[String], log: Log) -> Result<(), String> {
    let path = game.join(LIST_FILE);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = existing
        .lines()
        .map(str::to_owned)
        .filter(|l| !l.is_empty())
        .collect();
    let mut added = 0;
    for id in ids {
        let key = format!("mod.{id}");
        if lines
            .iter()
            .any(|l| l.split(':').next().unwrap_or("") == key)
        {
            continue;
        }
        lines.push(format!("{key}:true"));
        added += 1;
    }
    lines.sort();
    let text = format!("{}\n", lines.join("\n"));
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    log(format!("turned on {added} mod(s) in {}", path.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_resources_without_items_json_still_carry() {
        let root = std::env::temp_dir().join(format!("port-pack-no-items-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (work, resources) = (root.join("work"), root.join("resources"));
        fs::create_dir_all(work.join("oracle")).unwrap();
        fs::create_dir_all(&resources).unwrap();
        fs::write(work.join("inventory.json"), "[]").unwrap();
        fs::write(
            work.join("oracle/facts.json"),
            r#"{"blocks": {}, "items": {}}"#,
        )
        .unwrap();
        let logged = std::cell::RefCell::new(Vec::new());
        let log = |line: String| logged.borrow_mut().push(line);
        assert_eq!(carry(&work, Some(&resources), &[], &log), Ok(0));
        let logged = logged.borrow();
        assert!(
            logged
                .iter()
                .any(|l| l.contains("without the game's tables")),
            "{logged:?}"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_installed_carrier_is_a_mod_the_game_finds() {
        // `content_root` looks for folders with a manifest beside `options.txt`
        // (`cryon-mods::find`) and then reads the package, so a test that only
        // checks the files exist proves nothing. This walks the same two calls.
        let root = std::env::temp_dir().join(format!("port-pack-install-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (work, game) = (root.join("work"), root.join("game"));
        let carried = work.join("mods").join("demo").join("carry");
        fs::create_dir_all(carried.join("data").join("demo_content")).unwrap();
        fs::write(
            carried.join(cryon_mods::manifest::MANIFEST),
            "id = \"demo\"\nname = \"Demo\"\nversion = \"1\"\napi = \"0.2\"\nside = \"both\"\ncapabilities = [\"registries\"]\nnamespaces = [\"demo_content\"]\n",
        )
        .unwrap();
        fs::write(carried.join("data/demo_content/thing.txt"), "x").unwrap();
        let logged = std::cell::RefCell::new(Vec::new());
        let log = |line: String| logged.borrow_mut().push(line);
        assert_eq!(install(&work, &game, &log), Ok(1));
        assert_eq!(
            fs::read_to_string(game.join(LIST_FILE)).unwrap().trim(),
            "mod.demo:true"
        );

        let found = cryon_mods::find(&game.join(MODS_DIR));
        assert_eq!(found.len(), 1, "the game would find no mod: {found:?}");
        let package = cryon_mods::Package::read(&found[0]).expect("manifest does not parse");
        assert_eq!(package.manifest.id, "demo");
        assert!(
            package
                .manifest
                .capabilities
                .contains(&cryon_mods::Capability::Registries)
        );
        // Installing twice must not duplicate the switch line.
        install(&work, &game, &log).unwrap();
        assert_eq!(
            fs::read_to_string(game.join(LIST_FILE))
                .unwrap()
                .matches("mod.demo")
                .count(),
            1
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_staged_pack_lands_where_the_game_reads_packs() {
        let root = std::env::temp_dir().join(format!("port-pack-pack-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (work, game) = (root.join("work"), root.join("game"));
        fs::create_dir_all(work.join(PACKS_DIR)).unwrap();
        fs::create_dir_all(&game).unwrap();
        fs::write(work.join(PACKS_DIR).join("Faithful.zip"), b"zip").unwrap();
        fs::write(
            game.join("options.txt"),
            "resourcePacks:[\"vanilla\"]\nlang:en_us\n",
        )
        .unwrap();
        let logged = std::cell::RefCell::new(Vec::new());
        let log = |line: String| logged.borrow_mut().push(line);
        assert_eq!(install(&work, &game, &log), Ok(1));
        assert!(game.join(PACKS_DIR).join("Faithful.zip").is_file());
        let options = fs::read_to_string(game.join("options.txt")).unwrap();
        assert!(
            options.contains(r#"resourcePacks:["vanilla","file/Faithful.zip"]"#),
            "{options}"
        );
        // The game's other lines are untouched, and installing twice is not twice.
        assert!(options.contains("lang:en_us"));
        install(&work, &game, &log).unwrap();
        let again = fs::read_to_string(game.join("options.txt")).unwrap();
        assert_eq!(again.matches("file/Faithful.zip").count(), 1, "{again}");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_namespace_two_mods_claim_is_neither_s() {
        let claims = [
            ("betterdungeons", vec!["yungsapi".to_owned()]),
            ("betterfortresses", vec!["yungsapi".to_owned()]),
            (
                "towns_and_towers",
                vec!["kaisyn".to_owned(), "towns_and_towers".to_owned()],
            ),
        ];
        let shared = shared_namespaces(claims.into_iter(), &|_| {});
        assert_eq!(shared.into_iter().collect::<Vec<_>>(), ["yungsapi"]);
    }

    #[test]
    fn the_first_of_two_mods_owning_a_namespace_loads() {
        let root = std::env::temp_dir().join(format!("port-pack-loadable-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let carry = |key: &str, namespaces: &str| {
            let dir = root.join(key).join("carry");
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("cryon-mod.toml"),
                format!(
                    "id = \"{key}\"\nname = \"{key}\"\nversion = \"1\"\napi = \"0.2\"\n\
                     capabilities = [\"registries\"]\nnamespaces = [{namespaces}]\n"
                ),
            )
            .unwrap();
            dir
        };
        let mods = [
            carry("betterdungeons", "\"yungsapi\""),
            carry("betterfortresses", "\"yungsapi\""),
            carry("ruby", ""),
        ];
        let (on, off) = loadable(&mods);
        assert_eq!(on, [mods[0].clone(), mods[2].clone()]);
        assert_eq!(off.len(), 1);
        assert!(off[0].starts_with("betterfortresses not loaded"), "{off:?}");
        fs::remove_dir_all(&root).unwrap();
    }
}
