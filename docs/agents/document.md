# Document lane

You own `crates/lc-anim` and `crates/lc-scene`: the document model, animation
registry, evaluation, undo, tags, IK and the native scene format. Read
`AGENTS.md`, `docs/agents/README.md` and this brief before changing anything.

## Mission

Keep `Document` the authoritative, deterministic model of a scene and the
single source of evaluation and serialization. Every editor and MCP action is a
command in the shared history; every `.lcscene` file keeps loading.

## Entry points

- `crates/lc-scene/src/document.rs` — `Document`, settings, layers, save/open
  entry points.
- `scene.rs` — `Scene`, `Node`, `NodeKind`, `NodeId`, tags, skins, morph sets.
- `geometry.rs` — `Primitive`, `Modifier`, shape application.
- `eval.rs` — authoritative `Document::evaluate` and `EvaluatedScene`.
- `undo.rs` — command vocabulary and `History`.
- `format.rs` — validation, sanitization and serde defaults for `.lcscene`.
- `ik.rs` — IK chains, joint limits, CCD and baking.
- `camera.rs`, `light.rs`, `demo.rs` — node payloads and the studio scene.
- `crates/lc-anim/src/lib.rs` — F-curves, clips, channels, playback, skinning.

## Invariants

- Node IDs are stable `u64`s and are never renumbered by ordinary edits.
  `Document::signature()` ignores the allocator and stays the semantic
  dirty-check.
- Animation and joint limits, IK, tags, visibility, geometry, modifiers, morphs
  and skinning all run in `Document::evaluate`, in that order. It is
  deterministic and never touches GPU, clock or network. Renderer and UI must
  not duplicate it.
- Undo is command-based. One user action is one `Command` and one history
  entry; multi-object actions are all-or-nothing. Apply through the existing
  command types rather than mutating `Document` from callers.
- Every new serialized field uses `#[serde(default)]` and gets a legacy-loading
  test plus a round-trip test. Old `.lcscene` files must keep loading. Sanitize
  NaN/infinite values and impossible skin/morph data at load boundaries.
- Units: metres; stored rotations are normalized quaternions; UI/API angles are
  degrees and math is radians.
- `AnimChannel` is the only registry of animatable properties, their scope and
  component count. Recording and the curve editor resolve channels through it.
- Writes go through the existing atomic save helpers and recovery paths. Never
  write over a user target directly.

## Interfaces and boundaries

You own the contracts that every other lane consumes: node/tag/skin/morph types,
`Primitive`/`Modifier`, `EvaluatedScene`, the undo command vocabulary, the
native schema and `AnimChannel`. A change to any of them is a contract change:
land it first, keep it backward compatible, name the consumers and what they
must adapt, and add the tests below.

Do not edit `lc-geom` algorithms, `lc-shading`, `lc-render`, `lc-io`, `lc-ui`,
`lc-mcp`, `app`, root documentation or `Cargo.toml`. Ask the owning lane through
a handoff. Do not expose editor-only helpers in the document model.

## Tests and check

Inline `#[cfg(test)] mod tests` beside the code. Cover command undo/redo,
determinism, evaluation isolation, sanitization, serde defaults, native
round-trip, old-format loading, and rejected invalid data. For a serialized
schema change also run the interchange lane; for an `AnimChannel` change also
run the editor lane.

```bash
scripts/check-lane.sh document
```

The lane check covers `lc-anim` and `lc-scene` formatting, clippy and tests. It
does not compile `lc-ui`, `lc-io` or `lc-mcp`; the integrator runs the full gate
after merging a contract change.

## TODO mapping

Own `TODO.md` sections 1 (animation core), 2 (binding), 2b (IK), 4 (morph
targets), serialized-format fields, and the data model behind modifiers and
tags. UI workflows belong to the editor lane; interchange format work belongs
to its lane.

## Handoff

Report files, commands and results, the exact contract change and its consumer
impact, documentation owed, and any load/evaluation behavior left unverified.
