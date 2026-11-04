//! `cryon-port-pack`: port a Java modpack to Cryon (`docs/modpacks.md`).
//!
//! - `import <pack> <work>`: a `.mrpack`, CurseForge zip or instance folder
//!   to `<work>/pack.json`.
//! - `inventory <work> [--offline]`: Modrinth metadata (cached in
//!   `<work>/modrinth.json`), the jars in the instance or `<work>/jars/`,
//!   classes, tiers and dependency layers in `<work>/inventory.json` and
//!   `inventory.csv`.
//! - `fetch <work> [--from <folder>] [<mod id>...]`: the jars of the
//!   portable mods (content, data tier; or the ones named) into
//!   `<work>/jars/`: copied from the player's folder, or downloaded from the
//!   pack's own links or Modrinth's CDN, hash checked. What only CurseForge
//!   has needs a key and is listed in `<work>/fetch.json`.
//! - `carry <work> [--resources <prepared>] [<mod id>...]`: the content
//!   mods' data and assets into `<work>/mods/<id>/carry/`, a folder mod for
//!   local use. With the NeoForge oracle's `<work>/oracle/facts.json`
//!   (`tools/neoforge_oracle.py`), blocks and items come from its facts;
//!   `--resources` limits templates to the blocks Cryon's tables have.
//! - `report <work>`: the coverage report, per mod ported, partial, not yet
//!   or not needed with the reason, in `<work>/coverage.json` and
//!   `coverage.md`.
//! - `check <jar> <out> [--offline] [--mc <version>]`: one mod jar on its
//!   own, as the launcher's mod browser installs it: carried over into
//!   `<out>/carry/`, its classes read (Fabric's intermediary names resolved
//!   against the built-in table, or the full one fetched to
//!   `<out>/intermediary.json` unless `--offline`), and the verdict (Works,
//!   Partly, Doesn't run) in `<out>/check.json`.
//! - `install <work> <game>`: copy every `<work>/mods/<id>/carry` into
//!   `<game>/cryon-mods/<id>` and turn it on in `<game>/cryon-mods.txt`, so the
//!   port is a mod the game loads next start.
//! - `port <pack> <work> [--from <folder>] [--resources <prepared>]`: every
//!   step for the portable mods, as the launcher's "Import modpack" runs it.
//!   A single mod jar, resource pack or data pack is an accepted `<pack>`.
//! - `upgrade-datapack <pack> <out> | --world <world>`: a 26.2 data pack
//!   (zip or folder) with its world generation rewritten in 26.3's formats
//!   (`docs/datapack-upgrade.md`), into the folder `<out>`, or into
//!   `<world>/datapacks/<pack name>-26.3`.
//!
//! The work folder belongs to the user: jars and carried-over files stay
//! there and are never uploaded or committed.
use cryon_port_pack::{fetch, inventory::Record, pipeline, upgrade};
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

const USAGE: &str = "usage: cryon-port-pack import <pack> <work> | inventory <work> [--offline] | fetch <work> [--from <folder>] [<mod id>...] | carry <work> [--resources <prepared>] [<mod id>...] | report <work> | check <jar> <out> [--offline] [--mc <version>] | install <work> <game> | port <pack> <work> [--from <folder>] [--resources <prepared>] [--install <game>] | upgrade-datapack <pack> (<out> | --world <world>)";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let result = (|| {
        let from = option(&mut args, "--from")?;
        let resources = option(&mut args, "--resources")?;
        let install = option(&mut args, "--install")?;
        let world = option(&mut args, "--world")?;
        let mc = option(&mut args, "--mc")?.map(|p| p.to_string_lossy().into_owned());
        let offline = take_flag(&mut args, "--offline");
        let path = |i: usize| args.get(i).map(Path::new).ok_or(USAGE.to_owned());
        let rest = || args.get(2..).unwrap_or_default().to_vec();
        let log = &|line: String| println!("{line}");
        match args.first().map(String::as_str) {
            Some("import") => pipeline::import(path(1)?, path(2)?, log).map(drop),
            Some("inventory") => pipeline::inventory(path(1)?, offline, log).map(drop),
            Some("fetch") => {
                let work = path(1)?;
                let records: Vec<Record> = pipeline::read_json(&work.join("inventory.json"))?;
                let only = rest();
                let want = |r: &Record| {
                    if only.is_empty() {
                        pipeline::portable(r)
                    } else {
                        only.contains(&r.key)
                    }
                };
                fetch::fetch(work, &records, from.as_deref(), &want, log).map(drop)
            }
            Some("carry") => {
                pipeline::carry(path(1)?, resources.as_deref(), &rest(), log).map(drop)
            }
            Some("report") => {
                let work = path(1)?;
                pipeline::report(work, log)?;
                println!("{}", work.join("coverage.md").display());
                Ok(())
            }
            Some("import-one") => {
                let game = install.as_deref().ok_or("import-one requires --install <game>")?;
                let c = cryon_port_pack::check::import_one(path(1)?, path(2)?, game, resources.as_deref())?;
                println!("{}: {}. {}", c.mod_id, c.verdict.label(), c.summary);
                println!("Report: {}", path(2)?.join("check.json").display());
                println!("Imports are disabled until explicitly enabled; Java bytecode never runs.");
                Ok(())
            }
            Some("check") => {
                let jar = path(1)?;
                let out = path(2)?;
                // Fabric jars name the game classes they extend in
                // intermediary. The built-in table answers the common ones
                // offline; a fetched full mapping, cached in the folder like
                // modrinth.json, answers every class of a newer game.
                if !offline
                    && !out
                        .join(cryon_port_pack::intermediary::CACHE_FILE)
                        .is_file()
                {
                    let mc = mc.as_deref().unwrap_or("1.21.1");
                    match cryon_port_pack::intermediary::update(out, mc) {
                        Ok(n) => println!("intermediary: {n} class names cached for {mc}"),
                        Err(e) => eprintln!(
                            "warning: intermediary fetch failed ({e}); \
                             using the built-in table"
                        ),
                    }
                }
                let c = cryon_port_pack::check::check_with_resources(jar, out, resources.as_deref())?;
                println!("{}: {}. {}", c.mod_id, c.verdict.label(), c.summary);
                for p in &c.ports {
                    println!("  ports: {p}");
                }
                for s in &c.skipped {
                    println!("  skipped: {s}");
                }
                Ok(())
            }
            Some("install") => {
                let n = pipeline::install(path(1)?, path(2)?, log)?;
                println!(
                    "{n} mod(s) installed and turned on in {}",
                    path(2)?.display()
                );
                Ok(())
            }
            Some("port") => {
                let work = path(2)?;
                pipeline::import(path(1)?, work, log)?;
                pipeline::inventory(work, offline, log)?;
                pipeline::port(work, resources.as_deref(), from.as_deref(), log)?;
                println!("{}", work.join("coverage.md").display());
                if let Some(game) = &install {
                    pipeline::install(work, game, log)?;
                }
                Ok(())
            }
            Some("upgrade-datapack") => {
                let pack = path(1)?;
                let out = match &world {
                    Some(world) => {
                        let name = pack.file_stem().unwrap_or_default().to_string_lossy();
                        world.join("datapacks").join(format!("{name}-26.3"))
                    }
                    None => path(2)?.to_path_buf(),
                };
                let report = upgrade::upgrade_into(pack, &out)?;
                for line in report.lines() {
                    println!("{line}");
                }
                println!("{}", out.display());
                Ok(())
            }
            _ => Err(USAGE.to_owned()),
        }
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Removes `--name <value>` from `args`.
fn option(args: &mut Vec<String>, name: &str) -> Result<Option<PathBuf>, String> {
    match args.iter().position(|a| a == name) {
        Some(i) if i + 1 < args.len() => {
            let value = args.remove(i + 1);
            args.remove(i);
            Ok(Some(PathBuf::from(value)))
        }
        Some(_) => Err(USAGE.to_owned()),
        None => Ok(None),
    }
}

fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    let before = args.len();
    args.retain(|a| a != name);
    args.len() != before
}
