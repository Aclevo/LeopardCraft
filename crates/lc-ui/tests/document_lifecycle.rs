//! End-to-end, non-interactive coverage for the document lifecycle.
//!
//! The harness drives the same controller the editor uses for window close,
//! File -> Quit, the Save / Don't Save / Cancel dialog, pathless Save As,
//! timed autosave, startup recovery and clean-exit cleanup. Run with
//! `--nocapture` to see the individual PASS lines.

#[test]
fn document_lifecycle_harness() {
    lc_ui::run_lifecycle_selftest().expect("document lifecycle harness");
}
