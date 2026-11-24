# Interchange lane

You own `crates/lc-io`: native `.lcscene` dispatch, OBJ/MTL, STL and glTF/GLB
import and export. Read `AGENTS.md`, `docs/agents/README.md` and this brief
before changing anything.

## Mission

Move LeopardCraft documents in and out of common formats without silently
losing authored data, and keep every importer defensive. The native format is
the only complete representation; interchange formats are lossy by nature and
must say so.

## Entry points

- `lib.rs` — extension dispatch, `import`, `import_with_report`, `export`.
- `native.rs` — thin wrapper over `lc-scene`'s native format.
- `obj.rs` — OBJ/MTL pairs, transactional writes.
- `stl.rs` — binary and ASCII STL.
- `gltf.rs` — glTF 2.0 JSON and GLB parse/write, skins, morph targets, node
  animation, `ImportReport`.
- `common.rs` — shared path/atomic-write helpers.

## Invariants

- Never silently drop authored data. Anything the target format cannot carry is
  reported in `ImportReport`/the status line, and a lossy import is never
  presented as complete.
- Importers reject malformed data with a message and never panic on bad
  indices, truncation, NaN/infinite values or unsupported extensions. Delegate
  final sanitization to the document loader.
- Writes are atomic and transactional using the existing helpers. A failed
  export leaves no partial target or temporary file behind. OBJ/MTL pairs
  commit together.
- Do not change the native `.lcscene` schema here; the document lane owns it.
  Do not duplicate evaluation, undo or node-ID allocation.
- Determinism: write containers in stable order and avoid `HashMap` iteration
  order in output. No network, clock or unseeded randomness.
- Units and conventions follow the format: glTF is metres and radians, OBJ/STL
  are as authored, native is metres and normalized quaternions.

## Interfaces and boundaries

You consume `Document`, `lc-geom`, `lc-anim` and material data. `app`, `lc-mcp`
and `lc-ui` call your import/export entry points; automation owns their CLI and
MCP parameters. A new `Document` field that must round-trip requires the
document lane's schema change first, including its legacy-load test.

Do not edit `lc-scene`, `lc-geom`, `lc-anim`, `lc-shading`, `lc-render`, `lc-ui`,
`lc-mcp`, `app`, root documentation or `Cargo.toml`. Request the API you need.

## Tests and check

Inline `#[cfg(test)] mod tests` beside the code, using `tempfile` as the
existing tests do. Cover import/export round-trips for every format, malformed
and truncated input, skipped-data reports, accessor bounds, skins/morphs where
supported, and orphaned temporary-file recovery.

```bash
scripts/check-lane.sh interchange
```

The lane check covers `lc-io` formatting, clippy and tests. It does not compile
`lc-ui`, `lc-mcp` or `app`; the integrator runs the full gate. A native-schema
change also requires the document lane check.

## TODO mapping

Own `TODO.md` section 5 (glTF animation and skinning interchange) and the
interchange gaps listed in `README.md`: glTF import of skins/animation, USDZ,
Collada and the exact semantic differences in exported interpolation.

## Handoff

Report files, commands and results, any format behavior or lossy-report change,
documentation owed, and round-trips you could not verify.
