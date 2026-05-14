//! Usage Source Inventory discovery for local AI Coding Agent artifacts.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Current Source State projection used by Derived Summary and outputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CurrentSourceState {
    /// A readable Custom Usage Source or default local usage artifact was discovered.
    Readable {
        /// Representative Usage Source path for this run.
        path: PathBuf,
        /// Whether the path came from `--usage-source`.
        is_custom: bool,
    },
    /// No readable local usage artifacts were discovered.
    Missing {
        /// Representative Usage Source path attempted for this run.
        path: PathBuf,
        /// Whether the path came from `--usage-source`.
        is_custom: bool,
    },
}

impl CurrentSourceState {
    /// Returns the representative Usage Source path regardless of readability.
    pub fn path(&self) -> &PathBuf {
        match self {
            Self::Readable { path, .. } | Self::Missing { path, .. } => path,
        }
    }

    /// Returns true when the run has a readable source state.
    pub fn is_readable(&self) -> bool {
        matches!(self, Self::Readable { .. })
    }

    /// Returns true when the state came from a Custom Usage Source.
    pub fn is_custom(&self) -> bool {
        match self {
            Self::Readable { is_custom, .. } | Self::Missing { is_custom, .. } => *is_custom,
        }
    }
}

/// Usage Source Inventory for one AI Usage run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageSourceInventory {
    /// Current Source State projection for reporting and output modules.
    pub current_source_state: CurrentSourceState,
    /// Roots attempted while building the inventory.
    pub roots: Vec<UsageSourceRoot>,
    /// Readable local artifacts discovered under the roots.
    pub artifacts: Vec<UsageSourceArtifact>,
}

/// A root path considered while building a Usage Source Inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageSourceRoot {
    /// Local root path considered for discovery.
    pub path: PathBuf,
    /// Domain origin of the root.
    pub kind: UsageSourceRootKind,
    /// Whether the root is a readable file or directory.
    pub is_readable: bool,
}

/// Domain origin of a Usage Source Inventory root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageSourceRootKind {
    /// Default Codex Sessions Directory.
    CodexSessionsDirectory,
    /// Default Pi sessions directory.
    PiSessionsDirectory,
    /// Default OpenCode data directory.
    OpenCodeDataDirectory,
    /// Custom Usage Source root for this run.
    CustomUsageSource,
}

/// A readable local artifact discovered in a Usage Source Inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageSourceArtifact {
    /// Local artifact path.
    pub path: PathBuf,
    /// Filesystem shape of the artifact.
    pub shape: UsageSourceArtifactShape,
    /// Root kind where the artifact was discovered.
    pub root_kind: UsageSourceRootKind,
    /// Root path where the artifact was discovered.
    pub root_path: PathBuf,
    /// Whether the artifact came from a Custom Usage Source.
    pub is_custom: bool,
}

/// Filesystem shape of a Usage Source artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageSourceArtifactShape {
    /// A line-delimited JSON session-like file.
    JsonLinesSessionFile,
    /// A JSON session or message file.
    JsonSessionFile,
    /// An OpenCode SQLite database file.
    OpenCodeDatabase,
}

/// Builds the Usage Source Inventory for the current run.
pub fn build_usage_source_inventory(
    custom_usage_source: Option<PathBuf>,
) -> io::Result<UsageSourceInventory> {
    if let Some(path) = custom_usage_source {
        return custom_usage_source_inventory(path);
    }

    default_usage_source_inventory()
}

/// Returns the default local Codex Sessions Directory.
pub fn default_codex_sessions_directory() -> PathBuf {
    env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|home_directory| PathBuf::from(home_directory).join(".codex"))
        })
        .unwrap_or_else(|| PathBuf::from(".codex"))
        .join("sessions")
}

/// Returns the default local Pi sessions directory.
pub fn default_pi_sessions_directory() -> PathBuf {
    env::var_os("PI_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|home_directory| PathBuf::from(home_directory).join(".pi"))
        })
        .unwrap_or_else(|| PathBuf::from(".pi"))
        .join("agent")
        .join("sessions")
}

/// Returns the default local OpenCode data directory.
pub fn default_opencode_sessions_directory() -> PathBuf {
    env::var_os("OPENCODE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("XDG_DATA_HOME")
                .map(|data_home_directory| PathBuf::from(data_home_directory).join("opencode"))
        })
        .or_else(|| {
            env::var_os("HOME").map(|home_directory| {
                PathBuf::from(home_directory)
                    .join(".local")
                    .join("share")
                    .join("opencode")
            })
        })
        .unwrap_or_else(|| PathBuf::from(".local").join("share").join("opencode"))
}

fn custom_usage_source_inventory(path: PathBuf) -> io::Result<UsageSourceInventory> {
    let root = usage_source_root(path.clone(), UsageSourceRootKind::CustomUsageSource);
    let artifacts = if root.is_readable {
        discover_artifacts(&root)?
    } else {
        Vec::new()
    };
    let current_source_state = if root.is_readable {
        CurrentSourceState::Readable {
            path,
            is_custom: true,
        }
    } else {
        CurrentSourceState::Missing {
            path,
            is_custom: true,
        }
    };

    Ok(UsageSourceInventory {
        current_source_state,
        roots: vec![root],
        artifacts,
    })
}

fn default_usage_source_inventory() -> io::Result<UsageSourceInventory> {
    let roots = vec![
        usage_source_root(
            default_codex_sessions_directory(),
            UsageSourceRootKind::CodexSessionsDirectory,
        ),
        usage_source_root(
            default_pi_sessions_directory(),
            UsageSourceRootKind::PiSessionsDirectory,
        ),
        usage_source_root(
            default_opencode_sessions_directory(),
            UsageSourceRootKind::OpenCodeDataDirectory,
        ),
    ];
    let mut artifacts = Vec::new();
    let mut discovered_paths = BTreeSet::new();

    for root in &roots {
        if !root.is_readable {
            continue;
        }
        for artifact in discover_artifacts(root)? {
            if discovered_paths.insert(artifact.path.clone()) {
                artifacts.push(artifact);
            }
        }
    }
    artifacts.sort_by(|left, right| left.path.cmp(&right.path));

    let representative_path = artifacts
        .first()
        .map(|artifact| artifact.root_path.clone())
        .unwrap_or_else(default_codex_sessions_directory);
    let current_source_state = if artifacts.is_empty() {
        CurrentSourceState::Missing {
            path: representative_path,
            is_custom: false,
        }
    } else {
        CurrentSourceState::Readable {
            path: representative_path,
            is_custom: false,
        }
    };

    Ok(UsageSourceInventory {
        current_source_state,
        roots,
        artifacts,
    })
}

fn usage_source_root(path: PathBuf, kind: UsageSourceRootKind) -> UsageSourceRoot {
    let is_readable = fs::metadata(&path)
        .map(|metadata| metadata.is_dir() || metadata.is_file())
        .unwrap_or(false);

    UsageSourceRoot {
        path,
        kind,
        is_readable,
    }
}

fn discover_artifacts(root: &UsageSourceRoot) -> io::Result<Vec<UsageSourceArtifact>> {
    let metadata = fs::metadata(&root.path)?;
    if metadata.is_file() {
        return Ok(artifact_from_path(&root.path, root).into_iter().collect());
    }

    let mut artifacts = WalkDir::new(&root.path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| artifact_from_path(&entry.into_path(), root))
        .collect::<Vec<_>>();
    artifacts.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(artifacts)
}

fn artifact_from_path(path: &Path, root: &UsageSourceRoot) -> Option<UsageSourceArtifact> {
    let shape = artifact_shape(path).or_else(|| custom_file_artifact_shape(path, root))?;
    Some(UsageSourceArtifact {
        path: path.to_path_buf(),
        shape,
        root_kind: root.kind,
        root_path: root.path.clone(),
        is_custom: root.kind == UsageSourceRootKind::CustomUsageSource,
    })
}

fn artifact_shape(path: &Path) -> Option<UsageSourceArtifactShape> {
    if path
        .file_name()
        .is_some_and(|file_name| file_name == "opencode.db")
    {
        return Some(UsageSourceArtifactShape::OpenCodeDatabase);
    }

    match path.extension().and_then(|extension| extension.to_str()) {
        Some("jsonl") => Some(UsageSourceArtifactShape::JsonLinesSessionFile),
        Some("json") if path_has_session_or_message_component(path) => {
            Some(UsageSourceArtifactShape::JsonSessionFile)
        }
        _ => None,
    }
}

fn custom_file_artifact_shape(
    path: &Path,
    root: &UsageSourceRoot,
) -> Option<UsageSourceArtifactShape> {
    if root.kind != UsageSourceRootKind::CustomUsageSource || path != root.path {
        return None;
    }

    match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => Some(UsageSourceArtifactShape::JsonSessionFile),
        _ => None,
    }
}

fn path_has_session_or_message_component(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "session" || component.as_os_str() == "message")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_usage_source_replaces_default_discovery() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        fs::write(temporary_directory.path().join("session.jsonl"), "{}").expect("session file");
        let inventory =
            build_usage_source_inventory(Some(temporary_directory.path().to_path_buf()))
                .expect("usage source inventory");

        assert_eq!(inventory.artifacts.len(), 1);
        assert_eq!(
            inventory.current_source_state.path(),
            temporary_directory.path()
        );
        assert!(inventory.current_source_state.is_readable());
        assert!(inventory.current_source_state.is_custom());
        assert_eq!(
            inventory.artifacts[0].root_kind,
            UsageSourceRootKind::CustomUsageSource
        );
    }

    #[test]
    fn missing_custom_usage_source_is_distinct_from_readable_source() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let missing_path = temporary_directory.path().join("missing");
        let inventory = build_usage_source_inventory(Some(missing_path.clone()))
            .expect("usage source inventory");

        assert_eq!(
            inventory.current_source_state,
            CurrentSourceState::Missing {
                path: missing_path,
                is_custom: true
            }
        );
        assert!(inventory.artifacts.is_empty());
    }

    #[test]
    fn default_inventory_includes_all_readable_local_agent_artifacts() {
        let temporary_home = tempfile::tempdir().expect("temporary home");
        let codex_home = temporary_home.path().join("codex-home");
        let codex_sessions_directory = codex_home.join("sessions");
        let pi_home = temporary_home.path().join("pi-home");
        let pi_sessions_directory = pi_home.join("agent").join("sessions");
        let opencode_home = temporary_home.path().join("opencode-home");
        fs::create_dir_all(&codex_sessions_directory).expect("Codex sessions directory");
        fs::create_dir_all(&pi_sessions_directory).expect("Pi sessions directory");
        fs::create_dir_all(&opencode_home).expect("OpenCode data directory");
        fs::write(codex_sessions_directory.join("codex.jsonl"), "{}").expect("Codex file");
        fs::write(pi_sessions_directory.join("pi.jsonl"), "{}").expect("Pi file");
        fs::write(opencode_home.join("opencode.db"), "").expect("OpenCode database");

        unsafe {
            env::set_var("CODEX_HOME", &codex_home);
            env::set_var("PI_HOME", &pi_home);
            env::set_var("OPENCODE_HOME", &opencode_home);
        }

        let inventory = build_usage_source_inventory(None).expect("usage source inventory");

        assert!(inventory.current_source_state.is_readable());
        assert_eq!(inventory.artifacts.len(), 3);
        assert!(
            inventory.artifacts.iter().any(|artifact| {
                artifact.root_kind == UsageSourceRootKind::CodexSessionsDirectory
            })
        );
        assert!(
            inventory
                .artifacts
                .iter()
                .any(|artifact| { artifact.root_kind == UsageSourceRootKind::PiSessionsDirectory })
        );
        assert!(
            inventory.artifacts.iter().any(|artifact| {
                artifact.root_kind == UsageSourceRootKind::OpenCodeDataDirectory
            })
        );
    }
}
