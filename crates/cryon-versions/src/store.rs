use std::fmt;
use std::path::{Path, PathBuf};

/// What can go wrong reading a version store.
#[derive(Debug)]
pub enum StoreError {
    /// A file could not be read.
    Io { path: PathBuf, message: String },
    /// A file was not JSON.
    Json { path: PathBuf, message: String },
    /// A version was asked for but the store has not prepared it.
    Unknown { store: PathBuf, version: String },
    /// A prepared version has no data generator reports to read.
    NoReports { version: String, path: PathBuf },
    /// Two profiles for one version id.
    DuplicateVersion { version: String },
    /// Two versions claim one protocol number, which negotiation resolves by the version id the client
    /// names rather than by refusing to hold both.
    AmbiguousProtocol {
        protocol: i32,
        versions: Vec<String>,
    },
}

impl StoreError {
    /// Wraps an I/O failure on a known path.
    pub(crate) fn io(path: &Path, error: &std::io::Error) -> Self {
        StoreError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    }

    /// Wraps a JSON failure on a known path.
    pub(crate) fn json(path: &Path, error: &serde_json::Error) -> Self {
        StoreError::Json {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io { path, message } => write!(f, "{}: {message}", path.display()),
            StoreError::Json { path, message } => write!(f, "{}: {message}", path.display()),
            StoreError::Unknown { store, version } => {
                write!(
                    f,
                    "{store:?} has no prepared version {version}; run tools/version_port.py prepare {version}"
                )
            }
            StoreError::NoReports { version, path } => {
                write!(
                    f,
                    "{version} has no data generator reports at {}; run prepare again",
                    path.display()
                )
            }
            StoreError::DuplicateVersion { version } => {
                write!(f, "version {version} is in the catalog twice")
            }
            StoreError::AmbiguousProtocol { protocol, versions } => write!(
                f,
                "protocol {protocol} is answered by {} versions ({}); name the version to pick one",
                versions.len(),
                versions.join(", ")
            ),
        }
    }
}

impl std::error::Error for StoreError {}

/// Where a prepared version's files are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// The version id as Mojang names it.
    pub version: String,
    /// Its data generator reports, which hold `packets.json` and `registries.json`.
    pub reports: PathBuf,
    /// Whether this is the version the repository pins (whose files come from `.local/` rather than a
    /// download), which is why their reports sit elsewhere.
    pub pinned: bool,
}

/// Reads a version directory's `prepared.json`, which `tools/version_port.py prepare` writes.
///
/// The reports path is taken from that record rather than assumed: the pinned version's sit in the
/// repository's `.local/`, a downloaded version's inside the store.
pub fn read_prepared(directory: &Path) -> Result<Prepared, StoreError> {
    let record = directory.join("prepared.json");
    let text = std::fs::read_to_string(&record).map_err(|error| StoreError::io(&record, &error))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| StoreError::json(&record, &error))?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let recorded = value
        .get("paths")
        .and_then(|paths| paths.get("reports"))
        .and_then(serde_json::Value::as_str);
    let reports = match recorded {
        Some(path) => PathBuf::from(path),
        None => directory.join("datagen").join("reports"),
    };
    let reports = if reports.is_absolute() {
        reports
    } else {
        directory.join(reports)
    };
    if !reports.is_dir() {
        return Err(StoreError::NoReports {
            version,
            path: reports,
        });
    }
    Ok(Prepared {
        version,
        reports,
        pinned: value.get("pinned").and_then(serde_json::Value::as_bool) == Some(true),
    })
}

/// The JSON of a file, as a borrowed-to-owned value.
pub(crate) fn read_json(path: &Path) -> Result<serde_json::Value, StoreError> {
    let text = std::fs::read_to_string(path).map_err(|error| StoreError::io(path, &error))?;
    serde_json::from_str(&text).map_err(|error| StoreError::json(path, &error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/store")
    }

    #[test]
    fn a_reported_path_is_used_as_written() {
        let prepared = read_prepared(&store().join("0.1-test")).unwrap();
        assert_eq!(prepared.version, "0.1-test");
        assert!(prepared.pinned);
        assert!(prepared.reports.is_dir(), "{:?}", prepared.reports);
    }

    #[test]
    fn a_version_without_reports_is_named_not_guessed() {
        let error = read_prepared(&store().join("0.2-nodata")).unwrap_err();
        assert!(matches!(error, StoreError::NoReports { .. }), "{error:?}");
        assert!(error.to_string().contains("0.2-nodata"), "{error}");
    }

    #[test]
    fn a_missing_record_is_an_io_error() {
        assert!(matches!(
            read_prepared(&store().join("nope")),
            Err(StoreError::Io { .. })
        ));
    }
}
