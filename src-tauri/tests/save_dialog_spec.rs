//! Dr.Record SPEC.md — Feature 3 acceptance tests: Save-with-Name / Delete Dialog.
//!
//! Source of truth: SPEC.md sections "Feature 3", "EDGE CASES — Feature 3",
//! "USER FLOWS" (first-run, misclicks, out-of-order, double-clicks, dead-ends),
//! and acceptance criteria 14-18. Derived from the SPEC alone.
//!
//! PROPOSED CONTRACT (minimal pure-logic surface the implementation must provide):
//! ```text
//! dr_record_lib::save_dialog::{
//!     default_filename, MAX_FILENAME_LEN, FilenameError, sanitize_filename,
//!     collision_exists, DialogStage, should_show_dialog, DialogKey, KeyAction,
//!     key_action, DialogSession, SaveOutcome, DeleteOutcome, CancelOutcome,
//!     delete_take, StartRequest, request_start_while_open, Recovery,
//!     recover_after_quit,
//! }
//! ```
//! Filesystem effects in these tests use isolated per-test scratch dirs
//! (std only). If the builder places this logic under different paths, update
//! the `use` lines below. The BEHAVIORAL assertions must stay as written.
//!
//! Assumptions (flagged for review):
//! C1. `MAX_FILENAME_LEN` is exposed by the implementation (filesystem limit);
//!     tests assert behavior AT the exposed limit, not a hardcoded number.
//! C2. Windows reserved stems are rejected with or without an extension
//!     (`CON`, `con.mp4`), matching Win32 filename semantics.
//! C3. Cancel keeps the recording under its default name and clears temps
//!     (one of the two SPEC-defined cancel/recovery outcomes).

use dr_record_lib::save_dialog::{
    collision_exists, default_filename, delete_take, key_action, recover_after_quit,
    request_start_while_open, sanitize_filename, should_show_dialog, CancelOutcome, DeleteOutcome,
    DialogKey, DialogSession, DialogStage, FilenameError, KeyAction, Recovery, SaveOutcome,
    StartRequest, MAX_FILENAME_LEN,
};
use std::fs;
use std::path::PathBuf;
use std::process;

// ─── helpers ─────────────────────────────────────────────────────────────────

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dr-record-spec3-{}-{}", name, process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir must be creatable");
    dir
}

fn finish(dir: &PathBuf) {
    let _ = fs::remove_dir_all(dir);
}

/// A staged (validated, pre-dialog) final file plus its temp artifacts.
fn staged_take(dir: &PathBuf, stem: &str) -> (PathBuf, Vec<PathBuf>) {
    let staged = dir.join(format!("{stem}.mp4"));
    fs::write(&staged, b"fake-final-bytes").unwrap();
    let temps = vec![
        dir.join(format!("{stem}_video.mp4")),
        dir.join(format!("{stem}_sys.wav")),
        dir.join(format!("{stem}_mic.wav")),
        dir.join(format!("{stem}.log")),
    ];
    for t in &temps {
        fs::write(t, b"temp-artifact").unwrap();
    }
    (staged, temps)
}

fn open_session(dir: &PathBuf, stem: &str) -> DialogSession {
    let (staged, _temps) = staged_take(dir, stem);
    DialogSession::open(
        default_filename("Screen", "2026-09-21_10-00-00"),
        staged,
        dir.clone(),
    )
}

// ─── F3.1 Post-stop modal with prefilled timestamp default ───────────────────

#[test]
fn prefilled_default_uses_timestamp_format() {
    assert_eq!(
        default_filename("Screen", "2026-09-21_10-00-00"),
        "DrRecord_Screen_2026-09-21_10-00-00"
    );
}

#[test]
fn dialog_appears_only_after_finalization_never_mid_recording() {
    assert!(!should_show_dialog(&DialogStage::Recording));
    assert!(should_show_dialog(&DialogStage::FinalizedValid));
    assert!(should_show_dialog(
        &DialogStage::FinalizedFailedWithArtifact
    ));
}

#[test]
fn dialog_opens_prefilled_with_default_name() {
    let dir = scratch("prefill");
    let session = open_session(&dir, "take1");
    assert_eq!(session.prefilled(), "DrRecord_Screen_2026-09-21_10-00-00");
    assert!(!session.is_resolved());
    finish(&dir);
}

// ─── F3.2 Validation / sanitization ──────────────────────────────────────────

#[test]
fn empty_name_is_rejected_and_dialog_stays_open() {
    let dir = scratch("empty");
    let mut session = open_session(&dir, "take1");
    assert_eq!(
        session.confirm_save(""),
        SaveOutcome::ValidationError(FilenameError::Empty)
    );
    assert!(
        !session.is_resolved(),
        "invalid input keeps the dialog open"
    );
    finish(&dir);
}

#[test]
fn whitespace_only_name_is_rejected() {
    for bad in ["   ", "\t", " \t ", "\n"] {
        assert!(
            matches!(sanitize_filename(bad), Err(FilenameError::Empty)),
            "{bad:?} must be rejected as empty"
        );
    }
}

#[test]
fn every_windows_illegal_character_is_rejected_and_named() {
    for c in ['<', '>', ':', '"', '/', '\\', '|', '?', '*'] {
        let name = format!("take{c}1");
        assert_eq!(
            sanitize_filename(&name),
            Err(FilenameError::IllegalChar(c)),
            "illegal char {c:?} must be named in the error"
        );
    }
}

#[test]
fn control_characters_are_rejected() {
    assert_eq!(
        sanitize_filename("take\x001"),
        Err(FilenameError::IllegalChar('\0'))
    );
    assert_eq!(
        sanitize_filename("take\n1"),
        Err(FilenameError::IllegalChar('\n'))
    );
    assert_eq!(
        sanitize_filename("take\x1F"),
        Err(FilenameError::IllegalChar('\x1F'))
    );
}

#[test]
fn reserved_names_rejected_case_insensitively_with_or_without_extension() {
    // Assumption C2: stems are checked, so "con.mp4" is reserved too.
    for reserved in [
        "CON", "con", "Con", "PRN", "prn", "AUX", "aux", "NUL", "nul", "COM1", "com4", "COM9",
        "LPT1", "lpt9", "LPT9",
    ] {
        assert!(
            matches!(
                sanitize_filename(reserved),
                Err(FilenameError::ReservedName(_))
            ),
            "{reserved:?} must be rejected"
        );
        let with_ext = format!("{reserved}.mp4");
        assert!(
            matches!(
                sanitize_filename(&with_ext),
                Err(FilenameError::ReservedName(_))
            ),
            "{with_ext:?} must be rejected"
        );
    }
}

#[test]
fn reserved_lookalikes_are_allowed() {
    for ok in [
        "CONSOLE",
        "COM10",
        "LPT10",
        "aux1.mp4",
        "my CON file",
        "PRN2",
    ] {
        assert!(sanitize_filename(ok).is_ok(), "{ok:?} must be allowed");
    }
}

#[test]
fn overlong_names_rejected_at_limit_boundary() {
    // Assumption C1: boundary is the implementation-exposed limit.
    assert!(MAX_FILENAME_LEN > 0);
    let at_max = "a".repeat(MAX_FILENAME_LEN);
    assert!(
        sanitize_filename(&at_max).is_ok(),
        "name at max length saves"
    );
    let over = "a".repeat(MAX_FILENAME_LEN + 1);
    assert!(
        matches!(sanitize_filename(&over), Err(FilenameError::TooLong { .. })),
        "name beyond the limit is rejected with a message"
    );
}

#[test]
fn rename_to_same_name_as_default_is_a_plain_save() {
    let dir = scratch("same-name");
    let mut session = open_session(&dir, "take1");
    let prefilled = session.prefilled().to_owned();
    let outcome = session.confirm_save(&prefilled);
    assert!(
        matches!(outcome, SaveOutcome::Saved(_)),
        "same-name save must succeed without rename fuss, got {outcome:?}"
    );
    finish(&dir);
}

// ─── F3.3 + edge: collision needs explicit overwrite confirm ─────────────────

#[test]
fn filename_collision_never_silently_overwrites() {
    let dir = scratch("collision");
    fs::write(dir.join("Take.mp4"), b"existing-precious-bytes").unwrap();
    assert!(collision_exists(&dir, "Take.mp4"));
    assert!(!collision_exists(&dir, "Take 2.mp4"));

    let mut session = open_session(&dir, "take1");
    let outcome = session.confirm_save("Take");
    assert!(
        matches!(outcome, SaveOutcome::NeedsOverwriteConfirm(_)),
        "collision must ask, got {outcome:?}"
    );
    assert_eq!(
        fs::read(dir.join("Take.mp4")).unwrap(),
        b"existing-precious-bytes",
        "original must be untouched until explicit confirm"
    );
    finish(&dir);
}

// ─── F3.3 Save persists under edited name; Delete removes final + temps ──────

#[test]
fn save_persists_file_and_clears_temps() {
    let dir = scratch("save-ok");
    let mut session = open_session(&dir, "take1");
    let outcome = session.confirm_save("My Talk");
    let saved = match outcome {
        SaveOutcome::Saved(p) => p,
        other => panic!("expected Saved, got {other:?}"),
    };
    assert_eq!(saved, dir.join("My Talk.mp4"));
    assert!(saved.exists(), "confirmation must name a real path");
    for temp in [
        "take1_video.mp4",
        "take1_sys.wav",
        "take1_mic.wav",
        "take1.log",
    ] {
        assert!(
            !dir.join(temp).exists(),
            "temp {temp} must be gone after save"
        );
    }
    finish(&dir);
}

#[test]
fn delete_locally_removes_final_and_all_temps() {
    let dir = scratch("delete-ok");
    let (staged, temps) = staged_take(&dir, "take1");
    let outcome = delete_take(&staged, &temps);
    let removed = match outcome {
        DeleteOutcome::Deleted {
            final_path,
            temps_removed,
        } => {
            assert_eq!(final_path, staged);
            temps_removed
        }
        other => panic!("expected Deleted, got {other:?}"),
    };
    assert_eq!(removed, temps.len());
    assert!(!staged.exists());
    for t in &temps {
        assert!(
            !t.exists(),
            "temp {} must be gone after delete",
            t.display()
        );
    }
    finish(&dir);
}

#[test]
fn delete_missing_final_reports_error_naming_the_piece() {
    let dir = scratch("delete-missing");
    let missing = dir.join("ghost.mp4");
    let outcome = delete_take(&missing, &[]);
    assert!(
        matches!(outcome, DeleteOutcome::MissingArtifact(_)),
        "must report, never claim a deletion that did not happen, got {outcome:?}"
    );
    finish(&dir);
}

// ─── F3.4 Keyboard and click handling ────────────────────────────────────────

#[test]
fn enter_confirms_save_and_esc_follows_cancel() {
    assert_eq!(key_action(DialogKey::Enter), KeyAction::Save);
    assert_eq!(key_action(DialogKey::Escape), KeyAction::Cancel);
}

#[test]
fn double_click_save_executes_exactly_once() {
    let dir = scratch("double-save");
    let mut session = open_session(&dir, "take1");
    let first = session.confirm_save("Double");
    assert!(matches!(first, SaveOutcome::Saved(_)));
    let second = session.confirm_save("Double");
    assert_eq!(second, SaveOutcome::AlreadyResolved);
    let dupes: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "mp4").unwrap_or(false))
        .collect();
    assert_eq!(
        dupes.len(),
        1,
        "no `name (1)` duplicates, no second dialog effect"
    );
    finish(&dir);
}

#[test]
fn double_click_delete_executes_exactly_once() {
    let dir = scratch("double-delete");
    let mut session = open_session(&dir, "take1");
    let first = session.confirm_delete();
    assert!(
        matches!(first, DeleteOutcome::Deleted { .. }),
        "got {first:?}"
    );
    let second = session.confirm_delete();
    assert_eq!(second, DeleteOutcome::AlreadyResolved);
    finish(&dir);
}

#[test]
fn hotkey_start_while_dialog_open_waits_for_resolution() {
    let dir = scratch("ooo-start");
    let session = open_session(&dir, "take1");
    assert_eq!(
        request_start_while_open(&session),
        StartRequest::BlockedUntilResolved
    );
    let mut session = session;
    let _ = session.confirm_save("Resolved");
    assert_eq!(request_start_while_open(&session), StartRequest::Allowed);
    finish(&dir);
}

#[test]
fn output_dir_with_trailing_slash_still_saves() {
    let dir = scratch("trailing-slash");
    let with_slash = PathBuf::from(format!("{}{}", dir.display(), std::path::MAIN_SEPARATOR));
    let (staged, _temps) = staged_take(&dir, "take1");
    let mut session = DialogSession::open("Default".to_string(), staged, with_slash);
    assert!(matches!(
        session.confirm_save("Talk"),
        SaveOutcome::Saved(_)
    ));
    assert!(dir.join("Talk.mp4").exists());
    finish(&dir);
}

// ─── F3.5 + cancel semantics: no orphan temp files, quit recovery ────────────

#[test]
fn cancel_keeps_default_name_and_leaves_no_temps() {
    // Assumption C3: cancel resolves to keep-default + inform (no orphans).
    let dir = scratch("cancel");
    let mut session = open_session(&dir, "take1");
    let kept = match session.cancel() {
        CancelOutcome::KeptDefault(p) => p,
        other => panic!("expected KeptDefault, got {other:?}"),
    };
    assert!(kept.exists(), "cancelled take must remain discoverable");
    for temp in [
        "take1_video.mp4",
        "take1_sys.wav",
        "take1_mic.wav",
        "take1.log",
    ] {
        assert!(!dir.join(temp).exists(), "temp {temp} must not be orphaned");
    }
    finish(&dir);
}

#[test]
fn final_file_missing_when_dialog_opens_is_an_error_state() {
    let dir = scratch("dialog-missing-final");
    let missing = dir.join("ghost.mp4");
    let outcome = delete_take(&missing, &[]);
    assert!(
        matches!(outcome, DeleteOutcome::MissingArtifact(_)),
        "dialog must offer discard-of-temps path, got {outcome:?}"
    );
}

#[test]
fn quit_with_pending_dialog_recovers_to_a_defined_state() {
    let dir = scratch("quit-recovery");
    let default_path = dir.join("DrRecord_Screen_2026-09-21_10-00-00.mp4");
    fs::write(&default_path, b"unresolved-take").unwrap();
    let recovery = recover_after_quit(true, true, default_path.clone());
    match recovery {
        Recovery::ReofferDialog => {}
        Recovery::KeptDefault(p) => assert_eq!(p, default_path),
    }
    let recovery_none = recover_after_quit(false, false, default_path.clone());
    assert!(
        matches!(
            recovery_none,
            Recovery::ReofferDialog | Recovery::KeptDefault(_)
        ),
        "even empty recovery must be defined, never a silent orphan"
    );
    finish(&dir);
}
