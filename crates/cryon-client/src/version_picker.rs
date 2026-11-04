//! The installs on this device, and which one to run without asking.
//!
//! There used to be a screen here that listed them, for a phone with no desktop
//! launcher to hand the choice to. That screen is gone: the launcher is a Java
//! activity now, and it picks the version and the instance before the engine is
//! started, so an engine-side list would be the same question asked twice. What is
//! left is the part the game still needs — finding the prepared folders and
//! choosing between them when nobody said which one.

use std::path::{Path, PathBuf};

/// One prepared install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Install {
    pub path: PathBuf,
    /// The folder's name: what whoever made it called it.
    pub label: String,
}

/// An install is a folder holding the prepared index — the same test the game
/// uses before it starts loading, so nothing is offered that cannot be read.
fn is_install(path: &Path) -> bool {
    path.join("cryon-resources.json").is_file()
}

/// The installs present, newest name first.
///
/// A `versions` folder beside the one requested is the layout a mobile install
/// uses (`files/versions/26.3`), so a phone can hold several engine builds.
/// `.local` is the layout a source checkout uses, so this works during development.
pub fn scan(looked_in: &Path) -> Vec<Install> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(parent) = looked_in.parent() {
        candidates.push(parent.join("versions"));
        candidates.push(parent.to_path_buf());
    }
    if let Ok(dir) = std::env::current_dir() {
        candidates.push(dir.join("versions"));
        candidates.push(dir.join(".local"));
    }
    let mut found: Vec<Install> = Vec::new();
    for dir in candidates {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() || !is_install(&path) {
                continue;
            }
            if found.iter().any(|i| i.path == path) {
                continue;
            }
            let label = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            found.push(Install { path, label });
        }
    }
    found.sort_by(|a, b| b.label.cmp(&a.label));
    found
}

/// The install to use without asking anyone.
///
/// When there is exactly one there is nothing to ask. With several, a
/// `version.txt` beside them names the one to run — which is what the Java launcher
/// writes when PLAY is pressed — and the newest name wins if that file is missing or
/// stale.
///
/// Nothing here opens a window. A phone runs one activity and one event loop, and a
/// second loop makes android-activity drop the activity, so a screen that stopped to
/// ask would load its data into a window that no longer exists.
pub fn resolve(looked_in: &Path) -> Option<PathBuf> {
    let installs = scan(looked_in);
    match installs.len() {
        0 => None,
        1 => Some(installs[0].path.clone()),
        _ => {
            let wanted = looked_in
                .parent()
                .map(|p| p.join("versions").join("version.txt"))
                .and_then(|f| std::fs::read_to_string(f).ok())
                .map(|name| name.trim().to_owned());
            let chosen = wanted
                .as_deref()
                .and_then(|name| installs.iter().find(|i| i.label == name))
                .or_else(|| installs.first());
            chosen.map(|i| {
                if let Some(name) = &wanted {
                    log::info!("Cryon: version {name} chosen by version.txt");
                }
                i.path.clone()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("cryon-picker-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("versions")).unwrap();
        root
    }

    fn prepared(parent: &Path, name: &str) -> PathBuf {
        let dir = parent.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cryon-resources.json"), "{}").unwrap();
        dir
    }

    /// A folder that is not prepared is not an install: the game would offer a
    /// half-copied version and then fail on it.
    #[test]
    fn only_a_folder_with_the_prepared_index_counts() {
        let root = scratch("one");
        let good = prepared(&root.join("versions"), "26.3");
        std::fs::create_dir_all(root.join("versions/not-a-version")).unwrap();

        let found = scan(&root.join("resources"));
        assert_eq!(found.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["26.3"]);
        assert_eq!(resolve(&root.join("resources")), Some(good));
        std::fs::remove_dir_all(&root).ok();
    }

    /// With several installs, the launcher's `version.txt` wins over the newest
    /// name, so the version PLAY showed is the version that runs.
    #[test]
    fn version_txt_names_the_one_to_run() {
        let root = scratch("two");
        prepared(&root.join("versions"), "26.3");
        let older = prepared(&root.join("versions"), "26.2");
        std::fs::write(root.join("versions").join("version.txt"), "26.2\n").unwrap();

        assert_eq!(resolve(&root.join("resources")), Some(older));
        std::fs::remove_dir_all(&root).ok();
    }

    /// Nothing prepared anywhere is not the same as one badly named install: the
    /// caller has to say where it looked.
    #[test]
    fn nothing_on_the_device_is_no_choice() {
        let root = scratch("none");
        assert!(scan(&root.join("resources")).is_empty());
        assert_eq!(resolve(&root.join("resources")), None);
        std::fs::remove_dir_all(&root).ok();
    }
}
