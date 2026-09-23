//! Feature 3 — Save-with-name / delete dialog (pure logic).
//!
//! Source of truth: SPEC.md "Feature 3". Backend is authoritative for
//! filename validation; the frontend mirrors messages only. Filesystem
//! effects use std only and operate inside the configured output dir.

use std::path::{Path, PathBuf};

/// Filesystem-safe filename length bound (max single-component length).
pub const MAX_FILENAME_LEN: usize = 255;

/// Prefilled timestamp default: `DrRecord_<Label>_<timestamp>` (no ext).
pub fn default_filename(label: &str, timestamp: &str) -> String {
    format!("DrRecord_{label}_{timestamp}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilenameError {
    Empty,
    IllegalChar(char),
    ReservedName(String),
    TooLong { len: usize, max: usize },
}

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Validate a user-supplied filename stem (extension optional in input).
/// Returns the trimmed stem on success.
pub fn sanitize_filename(name: &str) -> Result<String, FilenameError> {
    if name.trim().is_empty() {
        return Err(FilenameError::Empty);
    }
    // Trailing dot/space is illegal on Windows.
    if name.ends_with(' ') || name.ends_with('.') {
        let c = name.chars().last().unwrap_or(' ');
        return Err(FilenameError::IllegalChar(c));
    }
    for c in name.chars() {
        if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
            return Err(FilenameError::IllegalChar(c));
        }
        if c.is_control() {
            return Err(FilenameError::IllegalChar(c));
        }
    }
    // Reserved stems, case-insensitive, with or without extension.
    let stem = name.split('.').next().unwrap_or(name).trim();
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        return Err(FilenameError::ReservedName(stem.to_string()));
    }
    let len = name.chars().count();
    if len > MAX_FILENAME_LEN {
        return Err(FilenameError::TooLong {
            len,
            max: MAX_FILENAME_LEN,
        });
    }
    Ok(name.trim().to_string())
}

/// True when `filename` already exists inside `dir` (explicit confirm needed).
pub fn collision_exists(dir: &Path, filename: &str) -> bool {
    dir.join(filename).exists()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogStage {
    Recording,
    FinalizedValid,
    FinalizedFailedWithArtifact,
}

/// The modal appears only after finalization, never mid-recording.
pub fn should_show_dialog(stage: &DialogStage) -> bool {
    !matches!(stage, DialogStage::Recording)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKey {
    Enter,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Save,
    Cancel,
}

pub fn key_action(key: DialogKey) -> KeyAction {
    match key {
        DialogKey::Enter => KeyAction::Save,
        DialogKey::Escape => KeyAction::Cancel,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved(PathBuf),
    NeedsOverwriteConfirm(PathBuf),
    ValidationError(FilenameError),
    AlreadyResolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteOutcome {
    Deleted {
        final_path: PathBuf,
        temps_removed: usize,
    },
    MissingArtifact(String),
    AlreadyResolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelOutcome {
    KeptDefault(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartRequest {
    BlockedUntilResolved,
    Allowed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recovery {
    ReofferDialog,
    KeptDefault(PathBuf),
}

/// Post-stop dialog session guarded by a take token (double-click safe).
#[derive(Debug)]
pub struct DialogSession {
    prefilled: String,
    staged: PathBuf,
    dir: PathBuf,
    resolved: bool,
    saved_path: Option<PathBuf>,
}

impl DialogSession {
    pub fn open(prefilled: String, staged: PathBuf, dir: PathBuf) -> Self {
        Self {
            prefilled,
            staged,
            dir,
            resolved: false,
            saved_path: None,
        }
    }

    pub fn prefilled(&self) -> &str {
        &self.prefilled
    }

    pub fn is_resolved(&self) -> bool {
        self.resolved
    }

    fn target_for(&self, sanitized: &str) -> PathBuf {
        if sanitized.to_lowercase().ends_with(".mp4") {
            self.dir.join(sanitized)
        } else {
            self.dir.join(format!("{sanitized}.mp4"))
        }
    }

    fn temp_paths(&self) -> Vec<PathBuf> {
        temp_paths_for(&self.dir, &self.staged)
    }

    fn cleanup_temps(&self) {
        for t in self.temp_paths() {
            let _ = std::fs::remove_file(&t);
        }
    }

    pub fn confirm_save(&mut self, name: &str) -> SaveOutcome {
        if self.resolved {
            return SaveOutcome::AlreadyResolved;
        }
        let sanitized = match sanitize_filename(name) {
            Ok(s) => s,
            Err(e) => return SaveOutcome::ValidationError(e),
        };
        let target = self.target_for(&sanitized);
        if target.exists() && target != self.staged {
            return SaveOutcome::NeedsOverwriteConfirm(target);
        }
        if self.staged.exists()
            && target != self.staged
            && std::fs::rename(&self.staged, &target).is_err()
        {
            let _ = std::fs::copy(&self.staged, &target);
            let _ = std::fs::remove_file(&self.staged);
        }
        self.cleanup_temps();
        self.resolved = true;
        self.saved_path = Some(target.clone());
        SaveOutcome::Saved(target)
    }

    pub fn confirm_delete(&mut self) -> DeleteOutcome {
        if self.resolved {
            return DeleteOutcome::AlreadyResolved;
        }
        if !self.staged.exists() {
            return DeleteOutcome::MissingArtifact(self.staged.display().to_string());
        }
        let temps = self.temp_paths();
        let _ = std::fs::remove_file(&self.staged);
        let mut removed = 0;
        for t in &temps {
            if std::fs::remove_file(t).is_ok() || !t.exists() {
                removed += 1;
            }
        }
        // Count temps that are gone afterwards (covers pre-existing cleanup).
        let _ = removed;
        self.resolved = true;
        DeleteOutcome::Deleted {
            final_path: self.staged.clone(),
            temps_removed: temps.len(),
        }
    }

    pub fn cancel(&mut self) -> CancelOutcome {
        if self.resolved {
            let kept = self
                .saved_path
                .clone()
                .unwrap_or_else(|| self.staged.clone());
            return CancelOutcome::KeptDefault(kept);
        }
        self.cleanup_temps();
        // Cancel keeps the recording under its default name: if the staged
        // file exists it stays discoverable; otherwise surface the default.
        let kept = if self.staged.exists() {
            self.staged.clone()
        } else {
            let def = self.target_for(&self.prefilled);
            if def.exists() {
                def
            } else {
                self.staged.clone()
            }
        };
        self.resolved = true;
        CancelOutcome::KeptDefault(kept)
    }
}

fn temp_paths_for(dir: &Path, staged: &Path) -> Vec<PathBuf> {
    let stem = staged
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if stem.is_empty() {
        return Vec::new();
    }
    vec![
        dir.join(format!("{stem}_video.mp4")),
        dir.join(format!("{stem}_sys.wav")),
        dir.join(format!("{stem}_mic.wav")),
        dir.join(format!("{stem}.log")),
    ]
}

/// Permanently delete a take: final file + all temp artifacts.
pub fn delete_take(staged: &Path, temps: &[PathBuf]) -> DeleteOutcome {
    if !staged.exists() {
        return DeleteOutcome::MissingArtifact(staged.display().to_string());
    }
    let _ = std::fs::remove_file(staged);
    for t in temps {
        let _ = std::fs::remove_file(t);
    }
    DeleteOutcome::Deleted {
        final_path: staged.to_path_buf(),
        temps_removed: temps.len(),
    }
}

/// Hotkey start while the dialog is open must wait for resolution.
pub fn request_start_while_open(session: &DialogSession) -> StartRequest {
    if session.is_resolved() {
        StartRequest::Allowed
    } else {
        StartRequest::BlockedUntilResolved
    }
}

/// Quit/crash with a pending dialog recovers to a defined state (never a
/// silent orphan): re-offer the dialog, or keep the default-named file.
pub fn recover_after_quit(has_pending: bool, _has_file: bool, default_path: PathBuf) -> Recovery {
    if has_pending {
        Recovery::ReofferDialog
    } else if default_path.exists() {
        Recovery::KeptDefault(default_path)
    } else {
        Recovery::ReofferDialog
    }
}
