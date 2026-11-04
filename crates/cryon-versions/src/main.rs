//! `cryon-versions`: ask a version store what its versions disagree about.
//!
//! ```text
//! cryon-versions [--store DIR] list                        every prepared version and what it carries
//! cryon-versions [--store DIR] negotiate PROTOCOL [VERSION]  which version answers a handshake
//! cryon-versions [--store DIR] id REGISTRY ENTRY           what one entry is numbered in each version
//! cryon-versions [--store DIR] packets OLD NEW             packet ids added, removed and renumbered
//! cryon-versions [--store DIR] registry OLD NEW [REGISTRY] protocol ids that moved between two versions
//! cryon-versions [--store DIR] matrix [REFERENCE]      how far every stored version sits from one
//! cryon-versions [--store DIR] bundle OUT [--registries=a,b]  write every version into one data file
//! ```
//!
//! `--store` defaults to `$CRYON_VERSIONS`, then `~/code/cryon-parity/versions`, which is where
//! `tools/version_port.py prepare` writes.

use std::path::{Path, PathBuf};

use cryon_versions::{
    Negotiation, ProfileBundle, VersionCatalog, VersionProfile, packet_drift, registry_drift,
    shared_registries,
};

const USAGE: &str =
    "usage: cryon-versions [--store DIR] <list|negotiate|id|packets|registry|matrix|bundle> ...";

fn main() {
    if let Err(error) = run(std::env::args().skip(1).collect()) {
        eprintln!("cryon-versions: {error}");
        std::process::exit(1);
    }
}

fn run(arguments: Vec<String>) -> Result<(), String> {
    let mut store: Option<PathBuf> = None;
    let mut rest = Vec::new();
    let mut iterator = arguments.into_iter();
    while let Some(argument) = iterator.next() {
        match argument.as_str() {
            "--store" => {
                store = Some(PathBuf::from(
                    iterator.next().ok_or("--store wants a path")?,
                ))
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => rest.push(argument),
        }
    }
    if rest.is_empty() {
        return Err(USAGE.to_owned());
    }
    let command = rest[0].clone();
    if command == "help" {
        println!("{USAGE}");
        return Ok(());
    }
    let store = store.unwrap_or_else(default_store);
    let (catalog, problems) =
        VersionCatalog::open_reporting(&store).map_err(|error| error.to_string())?;
    for (name, error) in &problems {
        eprintln!("cryon-versions: {name} is in the store but not in this build: {error}");
    }
    if catalog.is_empty() {
        return Err(format!(
            "{} has no prepared versions; run tools/version_port.py prepare VERSION",
            store.display()
        ));
    }
    let arguments = &rest[1..];
    match command.as_str() {
        "list" => list(&catalog),
        "negotiate" => negotiate(&catalog, arguments),
        "id" => identify(&catalog, arguments),
        "packets" => packets(&catalog, arguments),
        "registry" => registry(&catalog, arguments),
        "matrix" => matrix(&catalog, arguments),
        "bundle" => bundle(&catalog, &store, arguments),
        other => Err(format!("unknown command {other}\n{USAGE}")),
    }
}

fn default_store() -> PathBuf {
    if let Some(path) = std::env::var_os("CRYON_VERSIONS") {
        return PathBuf::from(path);
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("code").join("cryon-parity").join("versions")
}

fn version<'a>(
    catalog: &'a VersionCatalog,
    arguments: &'a [String],
    index: usize,
    what: &str,
) -> Result<&'a VersionProfile, String> {
    let id = arguments
        .get(index)
        .ok_or_else(|| format!("this command wants {what} version"))?;
    catalog.get(id).ok_or_else(|| {
        format!(
            "the store has no version {id}; it has: {}",
            catalog.ids().collect::<Vec<_>>().join(", ")
        )
    })
}

fn count(catalog: &VersionCatalog) -> String {
    format!(
        "{} version{}",
        catalog.len(),
        if catalog.len() == 1 { "" } else { "s" }
    )
}

fn list(catalog: &VersionCatalog) -> Result<(), String> {
    println!("{} in this build:", count(catalog));
    for profile in catalog.profiles() {
        println!("  {}", profile.summary());
        let blocks = profile
            .registry("minecraft:block")
            .map_or(0, |registry| registry.entries.len());
        let items = profile
            .registry("minecraft:item")
            .map_or(0, |registry| registry.entries.len());
        println!(
            "    packs resource {} data {}, java {}, {} block ids, {} item ids",
            profile.facts().resource_pack,
            profile.facts().data_pack,
            profile.facts().java_version,
            blocks,
            items
        );
    }
    println!(
        "newest release: {}",
        catalog
            .newest_release()
            .map(VersionProfile::id)
            .unwrap_or("(none)")
    );
    println!(
        "newest of any kind: {}",
        catalog.newest().map(VersionProfile::id).unwrap_or("(none)")
    );
    Ok(())
}

fn negotiate(catalog: &VersionCatalog, arguments: &[String]) -> Result<(), String> {
    let text = arguments
        .first()
        .ok_or("negotiate wants a protocol number")?;
    let protocol = text
        .parse::<i32>()
        .map_err(|_| format!("{text} is not a protocol number"))?;
    let chosen = match arguments.get(1) {
        Some(id) => catalog.negotiate_with_id(protocol, id),
        None => catalog.negotiate(protocol),
    };
    match chosen {
        Ok(profile) => {
            println!("protocol {protocol} -> {}", profile.summary());
            Ok(())
        }
        Err(Negotiation::Ambiguous { protocol, versions }) => Err(format!(
            "protocol {protocol} is answered by more than one release ({}); name the one the client sent: cryon-versions negotiate {protocol} VERSION",
            versions.join(", ")
        )),
        Err(error) => Err(error.to_string()),
    }
}

fn identify(catalog: &VersionCatalog, arguments: &[String]) -> Result<(), String> {
    let registry = arguments
        .first()
        .ok_or("id wants a registry and an entry")?;
    let entry = qualified(arguments.get(1).ok_or("id wants a registry and an entry")?);
    println!("{registry} {entry}:");
    let mut known = 0;
    for profile in catalog.profiles() {
        match profile.entry_id(registry, &entry) {
            Some(id) => {
                known += 1;
                println!("  {:<20} id {id}", profile.id());
            }
            None => println!("  {:<20} not in this version", profile.id()),
        }
    }
    if known == 0 {
        return Err(format!("no version in the store has {registry} {entry}"));
    }
    Ok(())
}

fn packets(catalog: &VersionCatalog, arguments: &[String]) -> Result<(), String> {
    let (from, to) = (
        version(catalog, arguments, 0, "an older")?,
        version(catalog, arguments, 1, "a newer")?,
    );
    println!("packets {} -> {}", from.id(), to.id());
    let (mut added, mut removed, mut renumbered) = (0, 0, 0);
    for drift in packet_drift(from, to) {
        if drift.added.is_empty() && drift.removed.is_empty() && drift.renumbered.is_empty() {
            continue;
        }
        println!(
            "  {}/{}: +{} -{} {} renumbered, {} unchanged",
            drift.state.key(),
            drift.direction.key(),
            drift.added.len(),
            drift.removed.len(),
            drift.renumbered.len(),
            drift.kept
        );
        print_entries("added", &drift.added);
        print_entries("removed", &drift.removed);
        print_shifts(&drift.renumbered);
        added += drift.added.len();
        removed += drift.removed.len();
        renumbered += drift.renumbered.len();
    }
    println!("  {added} added, {removed} removed, {renumbered} renumbered across every flow");
    if added + removed + renumbered == 0 {
        println!("  every packet id is the same in both versions: one decoder serves both");
    }
    Ok(())
}

fn print_entries(label: &str, entries: &[cryon_versions::Entry]) {
    for entry in entries.iter().take(20) {
        println!("    {label:<8} {} id {}", entry.name, entry.id);
    }
    if entries.len() > 20 {
        println!("    {label:<8} ... {} more", entries.len() - 20);
    }
}

fn print_shifts(shifts: &[cryon_versions::Shift]) {
    for shift in shifts.iter().take(20) {
        println!("    renamed  {} {} -> {}", shift.name, shift.from, shift.to);
    }
    if shifts.len() > 20 {
        println!("    renamed  ... {} more", shifts.len() - 20);
    }
}

fn registry(catalog: &VersionCatalog, arguments: &[String]) -> Result<(), String> {
    let (from, to) = (
        version(catalog, arguments, 0, "an older")?,
        version(catalog, arguments, 1, "a newer")?,
    );
    let names = match arguments.get(2) {
        Some(name) => vec![qualified(name)],
        None => shared_registries(from, to),
    };
    let mut moved = 0;
    for name in names {
        let Some(drift) = registry_drift(from, to, &name) else {
            continue;
        };
        if drift.is_still_translatable() {
            continue;
        }
        moved += 1;
        println!(
            "  {name}: +{} -{} {} shifted, {} in the same place",
            drift.added.len(),
            drift.removed.len(),
            drift.shifted.len(),
            drift.kept
        );
        print_shifts(&drift.shifted);
    }
    println!(
        "{} -> {}: {moved} of {} registries moved their ids",
        from.id(),
        to.id(),
        shared_registries(from, to).len()
    );
    Ok(())
}

/// One row per version: how far its ids sit from one reference, which is the work a build has to do per
/// version beyond carrying it.
fn matrix(catalog: &VersionCatalog, arguments: &[String]) -> Result<(), String> {
    let reference = match arguments.first() {
        Some(id) => id.clone(),
        None => catalog
            .newest_release()
            .or_else(|| catalog.newest())
            .map(VersionProfile::id)
            .ok_or("the catalog is empty")?
            .to_owned(),
    };
    let to = catalog.get(&reference).ok_or_else(|| {
        format!(
            "the store has no version {reference}; it has: {}",
            catalog.ids().collect::<Vec<_>>().join(", ")
        )
    })?;
    println!(
        "every version measured against {reference} (protocol {}, data version {})",
        to.protocol(),
        to.data_version()
    );
    println!(
        "  {:<18} {:>11} {:>7}   {:>5} {:>5} {:>6}   {:>5} {:>6}",
        "version", "protocol", "data", "+pkt", "-pkt", "renum", "shared", "moved"
    );
    for profile in catalog.profiles() {
        let flows = packet_drift(profile, to);
        let (added, removed, renumbered) = (
            flows.iter().map(|flow| flow.added.len()).sum::<usize>(),
            flows.iter().map(|flow| flow.removed.len()).sum::<usize>(),
            flows
                .iter()
                .map(|flow| flow.renumbered.len())
                .sum::<usize>(),
        );
        let shared = shared_registries(profile, to);
        let moved = shared
            .iter()
            .filter(|registry| {
                registry_drift(profile, to, registry)
                    .is_some_and(|drift| !drift.is_still_translatable())
            })
            .count();
        let same = if profile.id() == reference {
            " <- the reference"
        } else {
            ""
        };
        let (a, b, c) = if profile.has_packets() && to.has_packets() {
            (
                added.to_string(),
                removed.to_string(),
                renumbered.to_string(),
            )
        } else {
            ("-".to_owned(), "-".to_owned(), "-".to_owned())
        };
        let (d, e) = if profile.has_registries() && to.has_registries() {
            (shared.len().to_string(), moved.to_string())
        } else {
            ("-".to_owned(), "-".to_owned())
        };
        println!(
            "  {:<18} {:>11} {:>7}   {:>5} {:>5} {:>6}   {:>5} {:>6}{same}",
            profile.id(),
            profile.protocol(),
            profile.data_version(),
            a,
            b,
            c,
            d,
            e
        );
    }
    println!(
        "a version whose row is all zeros needs no id translation at all: it can share every table."
    );
    println!(
        "a - means that version's reports hold no such table, so the gap is unknown rather than zero."
    );
    Ok(())
}

fn bundle(catalog: &VersionCatalog, store: &Path, arguments: &[String]) -> Result<(), String> {
    let out = arguments.first().ok_or("bundle wants an output file")?;
    let keep = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--registries="))
        .map(|list| list.split(',').map(qualified).collect::<Vec<_>>());
    if let Some(keep) = &keep {
        let unknown = keep
            .iter()
            .filter(|want| {
                !catalog
                    .profiles()
                    .any(|profile| profile.registries().get(want).is_some())
            })
            .cloned()
            .collect::<Vec<_>>();
        if !unknown.is_empty() {
            return Err(format!(
                "no version in the store has a registry named {}; the bundle would quietly carry everything else",
                unknown.join(", ")
            ));
        }
    }
    let bundle = ProfileBundle::with_registries(catalog, keep.as_deref());
    bundle
        .save(Path::new(out))
        .map_err(|error| error.to_string())?;
    let registries = bundle
        .versions
        .first()
        .map(|version| version.registries.len())
        .unwrap_or(0);
    println!(
        "{} KB, {} versions, {registries} registries each -> {}",
        bundle.json_len() / 1024,
        bundle.len(),
        Path::new(out).display()
    );
    let read = bundle.catalog().map_err(|error| error.to_string())?;
    println!(
        "read back from the file: {}",
        read.protocols()
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("source: {}", store.display());
    Ok(())
}

/// `stone` and `minecraft:stone` name the same entry; the reports always spell it out.
fn qualified(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}
