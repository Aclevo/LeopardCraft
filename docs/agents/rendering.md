# Rendering lane

You own `crates/lc-shading` and `crates/lc-render`: materials and the node
shader graph, procedural textures, render-scene flattening, camera models, the
CPU path tracer and PNG/EXR output. Read `AGENTS.md`,
`docs/agents/README.md` and this brief before changing anything.

## Mission

Turn an already-evaluated scene into a deterministic image, and provide the
material evaluation the viewport and exporters reuse. The real-time viewport
preview and the offline path tracer are different quality levels of the same
material data; neither may become a second scene evaluator.

## Entry points

- `crates/lc-shading/src/lib.rs` — material library and public types.
- `graph.rs` — node shader graph, Principled surface and node evaluation.
- `noise.rs` — procedural noise; `texture.rs` — texture cache and image input.
- `crates/lc-render/src/scene.rs` — `RenderScene` flattening from
  `EvaluatedScene`.
- `camera.rs` — perspective, orthographic and panoramic rays.
- `tracer.rs` — CPU path tracer: NEE, GGX, transmission, depth of field,
  progressive accumulation.
- `output.rs`, `exr.rs` — ACES PNG and linear EXR writers.

## Invariants

- Consume `EvaluatedScene` only. Animation, modifiers, IK, morphs, skinning and
  visibility are already applied by `Document::evaluate`; never re-run them
  here.
- Determinism: per-pixel `Pcg32` seeds and ordered traversal. The same scene,
  time, seed and sample count produce the same image. No wall clock, thread
  scheduling, network or unseeded randomness in output.
- Keep all radiometric and material values finite. Degenerate rays, zero-area
  lights and coincident geometry return black/transparent rather than NaN or a
  panic.
- Scene units are metres. Shader angles are radians internally.
- Material and shader-graph changes stay serializable through the document
  types owned by the document lane; do not invent a parallel material format.
- The viewport is a material preview, not the offline path tracer. Do not claim
  visual parity between them.

## Interfaces and boundaries

You consume evaluated geometry, cameras, lights and materials from `lc-scene`
and `lc-shading`. `lc-ui` uses the material graph and viewport preview,
`lc-mcp` and `app` call the renderer for image output, and `lc-io` consumes
material values on export. Render output options and CLI/MCP parameters are
exposed through the automation lane; request signature changes there.

Do not edit `lc-scene` evaluation or serialization, `lc-io`, `lc-ui`, `lc-mcp`,
`app`, root documentation or `Cargo.toml`. If a render feature needs new scene
data, request it from the document lane first.

Changes to image output formats, colour transforms or material semantics are
user-visible contracts: report them so README and the parity matrix can be
updated by the integrator.

## Tests and check

Inline `#[cfg(test)] mod tests` beside the code. Cover deterministic output for
fixed seed/samples, PNG/EXR encoding, camera ray generation, light sampling,
transmission and degenerate scenes. Keep tests fast; the headless selftest in
`app` is the end-to-end integration check, not a lane unit test.

```bash
scripts/check-lane.sh rendering
```

The lane check covers `lc-shading` and `lc-render` formatting, clippy and tests.
It does not compile the editor or binary; the integrator runs the full gate.

## TODO mapping

Own the rendering and material items under `TODO.md` sections 6 (modifier and
subdivision parity where shading is involved) and the renderer limitations in
`README.md`. Parity-document updates are reported to the integrator; do not
claim Cheetah3D parity without a researched source.

## Handoff

Report files, commands and results, material/render contract changes, docs
owed, and any image or viewport behavior that was not visually verified.
