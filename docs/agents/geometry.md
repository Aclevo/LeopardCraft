# Geometry lane

You own `crates/lc-core` and `crates/lc-geom`: the pure, headless mathematics
and geometry that every other crate builds on. Read `AGENTS.md`,
`docs/agents/README.md` and this brief before changing anything.

## Mission

Implement and test meshes, primitives, splines, UV algorithms, BVH, topology
operations, subdivision, CSG and core math helpers. Work here must be usable by
the renderer, exporters, MCP tools and the editor without a GPU, document or
clock.

## Entry points

- `crates/lc-geom/src/lib.rs` — re-exports; keep the public surface tidy.
- `mesh.rs` — n-gon mesh, corners, UV slots, edges, creases, seams, pins.
- `ops.rs` — extrusion, inset, bevel, ring cut, weld, bridge, fill, split,
  shell, collapse, triangulate, flip, cover, disconnect, point slide, optimize,
  deformers.
- `uv.rs` — projections (UV0 and any set), automatic unwrap, seam-aware
  charts, relaxation, chart transform/packing, UV slot operations and
  distortion measurement.
- `subdiv.rs` — Catmull-Clark, Stam-Loop (the published quad/triangle hybrid) and linear subdivision.
- `csg.rs` — BSP booleans; `bvh.rs` — ray acceleration.
- `primitives.rs`, `spline.rs` — parametric builders and curves.
- `crates/lc-core/src/lib.rs` — transforms, AABB, rays, PCG32, colour,
  tone mapping, stable hashing.

## Invariants

- Geometry is n-gon and corner/UV-slot based. Preserve winding, material slots,
  UV slots, creases, seams and pinned UVs unless the operation's contract says
  otherwise. Creases and seams are sorted canonical point pairs; normalize them
  after topology changes.
- `Mesh::uvs` is UV0 and `Mesh::extra_uvs` holds additional sets as parallel
  coordinate arrays over the same slot indices. Any operation that adds, removes
  or rebuilds slots must keep every set the same length (weld/split remap them,
  subdivision carries each set through the same UV0 rules, `sync_uv_sets`
  repairs a damaged length). Pins and seams belong to the shared slot/point
  topology, not to one set.
- Keep every result finite. Degenerate topology and zero-length vectors return a
  sensible value or `None`, never NaN or a panic.
- Determinism: iterate ordered containers or sort before hashing/rendering; use
  `Pcg32` with explicit seeds. No `HashMap` iteration order, wall clock, thread
  scheduling or network in outputs.
- Scene units are metres; math is radians. Conversion to degrees happens at the
  UI/API edge, not inside geometry.
- Geometry functions do not know about `Document`, undo, tags or rendering.
  Do not add evaluation or serialization logic here.

## Interfaces and boundaries

`lc-scene` owns the `Primitive` and `Modifier` enums and applies your functions
during evaluation; `lc-ui` calls the modelling tools; `lc-io` and `lc-render`
consume the mesh. Keep new APIs additive and pure. If a consumer needs a new
operation, add it here with tests and report the signature so the consumer lane
can wire it.

Do not edit `lc-scene`, `lc-anim`, `lc-shading`, `lc-render`, `lc-io`, `lc-ui`,
`lc-mcp`, `app`, root documentation, `Cargo.toml` or scripts. Request those
changes through a handoff instead.

Changes to `lc-core` public types are contract-sensitive because every lane
links them; call them out explicitly.

## Tests and check

Put tests in an inline `#[cfg(test)] mod tests` beside the code, following the
nearest module. Cover edge cases: empty/degenerate meshes, invalid indices,
pinned/seam/crease preservation, determinism, and exact prior topology after an
undoable mesh operation. Native round-trip tests belong to the document and
interchange lanes.

```bash
scripts/check-lane.sh geometry
```

The lane check runs formatting, clippy with `-D warnings` and tests for
`lc-core` and `lc-geom`. It does not compile dependents; the integrator runs the
full gate after merging.

## TODO mapping

Own the geometry side of `TODO.md` sections 3 (UV algorithms), 6 (modifiers and
subdivision) and 6b (polygon tools), plus new primitives. The editor owns the
UI half; coordinate through a contract-first sequence rather than editing
`lc-ui`.

## Handoff

Report changed files, the exact lane command and result, any new public API and
its intended consumers, documentation owed, and work you could not verify
headless.
