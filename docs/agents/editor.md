# Editor lane

You own `crates/lc-ui`: the eframe/wgpu editor, viewport, panels, timeline,
material graph, selection and lifecycle. Read `AGENTS.md`,
`docs/agents/README.md` and this brief before changing anything.

## Mission

Expose the engine's data model through a consistent, crash-resistant editor.
The UI is a client of `Document` commands and evaluated snapshots; it is never a
second source of scene truth.

## Entry points

- `app.rs` — `LeopardApp`, the single UI state hub, shared types and core
  state/evaluation helpers.
- `scene_edit.rs` — object creation, selection, grouping, reparenting and
  deletion workflows.
- `documents.rs` — new/open/import/save workflows, file dialogs, pending
  actions and autosave.
- `render_jobs.rs` — background render queue, polling, start and save.
- `deform.rs` — modifier stack, skin binding, weight painting and morphs.
- `mesh_edit.rs` — editable-mesh, component selection, UV and mesh tools.
- `animation.rs` — timeline targets, F-curve keys, clipboard and retiming.
- `dialogs.rs` — menu-driven file, guard, recovery and about dialogs.
- `viewport_ui.rs` — viewport frame, navigation, picking, gizmo and overlays.
- `panels.rs`, `panels_more.rs` — Properties, Tags, Materials, mesh editing,
  skinning, splines, render settings.
- `shell.rs` — menu bar, toolbar, keyboard shortcuts.
- `viewport.rs` — wgpu offscreen viewport and overlays.
- `selection.rs` — shared selection predicates.
- `selection_tests.rs` plus `selection_tests/` — shared UI test harness and the
  scene-edit, viewport, mesh, deform, animation and document workflow tests.
- `lifecycle.rs`, `recovery.rs`, `lifecycle_harness.rs` — open/save/close,
  autosave and recovery.
- `theme.rs`, `icons.rs` — visual chrome.

## Invariants

- Every document mutation goes through the existing undo commands. One user
  action is one history entry; multi-object actions apply all-or-nothing.
  Never mutate `Document` directly from a panel callback.
- Do not duplicate `Document::evaluate` or re-derive animation, modifiers, IK,
  morphs or skinning in the UI. Read `EvaluatedScene`.
- Selection predicates live in `selection.rs` and are shared by menus, toolbar
  buttons and context menus. A handler rechecks its predicate before touching
  geometry so disabled UI and actual behavior cannot diverge.
- Keep `LeopardApp` the only UI state owner. Do not scatter document or
  selection state into panels.
- File writes use the existing atomic save/export helpers and recovery paths.
  A failed write keeps the document dirty and reports the error; it leaves no
  partial target or temporary file.
- Keep the fixed workspace layout and existing control conventions; user-visible
  control changes must be reported for README updates.
- No new dependencies without the integrator's approval.

## Interfaces and boundaries

You consume every lower crate through their public APIs. You need geometry
operations from the geometry lane, document/command/animation APIs from the
document lane, material/render APIs from the rendering lane, and import/export
from interchange. Ask for an API change; never patch another crate to make a
panel compile.

`lc-ui` stays one lane. `app.rs` is the choke point; do not split panels across
parallel editor agents. Keep action logic in the existing modules rather than
creating a parallel UI framework.

Do not edit `lc-scene`, `lc-anim`, `lc-geom`, `lc-shading`, `lc-render`,
`lc-io`, `lc-mcp`, `app`, root documentation or `Cargo.toml`.

## Tests and check

Put UI behavior tests in `selection_tests.rs` (and the nearest module's inline
tests for smaller helpers). A UI action that mutates the document needs an undo
test. Cover disabled/refused actions, mixed selections, multi-object all-or-
nothing behavior and lifecycle write failures.

```bash
scripts/check-lane.sh editor
```

The lane check covers `lc-ui` formatting, clippy and tests. It does not provide
a display or GPU, so it cannot prove visual appearance. Say what was not
visually verified; launch the editor separately only when asked and with a
display available.

## TODO mapping

Own the UI halves of `TODO.md` sections 1-4 and 6b, plus controls and panels.
Geometry algorithms belong to the geometry lane, persistence and evaluation to
the document lane. Coordinate feature slices in that order.

## Handoff

Report files, commands and results, the exact user-visible behavior changed,
documentation owed (README controls), and any GUI appearance or interaction that
was not verified.
