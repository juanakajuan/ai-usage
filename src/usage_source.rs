//! Usage Source discovery for local AI Coding Agent session files.

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

/// Result of resolving the Usage Source for a single run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UsageSourceResolution {
    /// A readable AI Coding Agent sessions directory or custom path exists.
    Readable {
        /// Path used as the authoritative Usage Source for this run.
        path: PathBuf,
        /// Whether the path came from `--usage-source`.
        is_custom: bool,
    },
    /// No readable local usage source was found.
    Missing {
        /// Path that was attempted.
        path: PathBuf,
        /// Whether the path came from `--usage-source`.
        is_custom: bool,
    },
}

impl UsageSourceResolution {
    /// Returns the resolved path regardless of readability.
    pub fn path(&self) -> &PathBuf {
        match self {
            Self::Readable { path, .. } | Self::Missing { path, .. } => path,
        }
    }

    /// Returns true when a readable source exists but contains no usage.
    pub fn is_readable(&self) -> bool {
        matches!(self, Self::Readable { .. })
    }
}

/// Resolves the Usage Source for the current run.
pub fn resolve_usage_source(
    custom_usage_source: Option<PathBuf>,
) -> io::Result<UsageSourceResolution> {
    if let Some(path) = custom_usage_source {
        return Ok(resolve_path(path, true));
    }

    let codex_sessions_directory = default_codex_sessions_directory();
    let codex_resolution = resolve_path(codex_sessions_directory, false);
    if codex_resolution.is_readable() {
        return Ok(codex_resolution);
    }

    let pi_sessions_directory = default_pi_sessions_directory();
    let pi_resolution = resolve_path(pi_sessions_directory, false);
    if pi_resolution.is_readable() {
        return Ok(pi_resolution);
    }

    let opencode_sessions_directory = default_opencode_sessions_directory();
    let opencode_resolution = resolve_path(opencode_sessions_directory, false);
    if opencode_resolution.is_readable() {
        return Ok(opencode_resolution);
    }

    Ok(codex_resolution)
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

/// Returns the default local Pi Sessions Directory.
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

/// Returns the default local OpenCode Sessions Directory.
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

fn resolve_path(path: PathBuf, is_custom: bool) -> UsageSourceResolution {
    match fs::metadata(&path) {
        Ok(metadata) if metadata.is_dir() || metadata.is_file() => {
            UsageSourceResolution::Readable { path, is_custom }
        }
        _ => UsageSourceResolution::Missing { path, is_custom },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_usage_source_replaces_default_discovery() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let resolution = resolve_usage_source(Some(temporary_directory.path().to_path_buf()))
            .expect("usage source resolution");

        assert_eq!(
            resolution,
            UsageSourceResolution::Readable {
                path: temporary_directory.path().to_path_buf(),
                is_custom: true
            }
        );
    }

    #[test]
    fn missing_usage_source_is_distinct_from_readable_source() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let missing_path = temporary_directory.path().join("missing");
        let resolution =
            resolve_usage_source(Some(missing_path.clone())).expect("usage source resolution");

        assert_eq!(
            resolution,
            UsageSourceResolution::Missing {
                path: missing_path,
                is_custom: true
            }
        );
    }
}
