# LeopardCraft

LeopardCraft is a clean-room, offline-first 3D modelling, rendering and animation
application written in pure Rust. It is inspired by the focused, approachable
workflow of classic polygon modellers and by the native, crash-resistant
engineering ethos of the ArtCraft suite, but its architecture and implementation
are original.

- **Native:** Linux, macOS and Windows (eframe + wgpu).
- **Offline:** no telemetry, accounts, network services or cloud dependencies.
- **Deterministic:** headless scene evaluation, stable IDs and an undo/redo
  command history.
- **Tested:** geometry, shading, animation, scene evaluation, rendering and file
  I/O all have automated round-trip and regression tests.

## Screenshots

![The LeopardCraft editor with the built-in studio scene loaded](docs/images/editor.png)

The editor on first launch, captured from the running application: the shaded viewport with the studio
cube, sphere, torus and glass sphere, the transform gizmo, the `Properties` column, the `Objects` browser,
the material shelf and the full-width timeline. The status bar reads `Frame 0 · 13 objects · 7418 tris`.

## Quick start

Prebuilt Linux, macOS and Windows binaries for 0.1.0 are on the [releases page](https://github.com/Aclevo/LeopardCraft/releases/tag/v0.1.0); the steps below build from source.

```bash
cargo run -p leopardcraft
```

The editor opens with a ready-to-use studio scene: camera, three-point lighting,
ground, beveled cube, animated sphere and torus, glass sphere, checker panel and
six materials.

```bash
cargo test --workspace
cargo run -p leopardcraft -- --headless selftest
```

`selftest` checks deterministic evaluation, native serialization, PNG/EXR
output and OBJ/STL/GLB round-trips in a temporary directory.

## Workspace layout

| Crate | Responsibility |
| --- | --- |
| `lc-core` | math helpers, transforms, bounds, rays, PCG32, colour, tonemapping, stable hashing |
| `lc-geom` | n-gon mesh, primitives, splines, UV projection/relaxation, BVH, extrude/bevel/ring-cut/bridge, subdivision, BSP booleans |
| `lc-shading` | node material graph, Principled surface, procedural noise, texture cache |
| `lc-anim` | F-curves, animation tracks, playback, indexed skinning |
| `lc-scene` | scene graph, tags, layers, document format, deterministic evaluation, undo commands |
| `lc-render` | camera models, flattened render scene, BVH, CPU path tracer, PNG and EXR writers |
| `lc-io` | native `.lcscene`, OBJ/MTL, STL, glTF/GLB import and export |
| `lc-ui` | Cheetah-style fixed egui workspace, wgpu viewport, inspectors, material graph, UV view, timeline |
| `app` | `leopardcraft` native binary and headless CLI |

The native scene format is pretty-printed JSON with a format/version header. It is
validated and sanitized on load. Document signatures ignore allocator counters, so
save/load round-trips can be compared deterministically.

## Editor

The fixed workspace is laid out like a classic Mac modeller: a menu bar and
original vector-icon toolbar, a `3D view | UVs | Nodes` switcher above the
viewport, a Properties column over an `Objects | Takes | Layers | Console`
browser, a material-sphere shelf, a full-width timeline and a separate Render
Manager window.

- **3D Viewport** — wgpu shaded/wireframe display with a blue-grey gradient,
  light floor grid, white selected wireframe, light and camera helpers,
  orbit/pan/zoom navigation and a transform gizmo. Alt+drag orbits or pans.
- **Object Browser** — hierarchical scene tree with type icons, visibility,
  render and lock toggles, modifier rows, and a layer manager.
- **Properties / Tags** — icon tabs for object, material and render settings;
  object, camera, light, spline and primitive parameters; modifier stack; tags;
  skin binding; mesh-editing tools; spline control points.
- **Node Materials** — material shelf with shaded sphere previews and an
  editable light-canvas shader graph with Principled, noise, voronoi, ramp,
  math, texture, normal and emission nodes.
- **UV Editor** — planar/box/cylindrical/spherical projection, auto-unwrap,
  relaxation and draggable UV points over a light checker.
- **Timeline / F-Curves** — transport, frame ruler, key diamonds, orange
  playhead, scrubbing, key insertion and an expandable curve editor.
- **Render Manager** — capped job history with thumbnails and info, live
  progress, cancellation, and PNG/EXR export from background CPU path tracing.

### Mesh editing

Select a mesh, open **Properties / Tags → Mesh Editing** and press **Enter Mesh
Edit**. Primitives are converted to editable n-gon meshes first. Click points,
edges or polygons in the viewport; Shift-click extends the selection.

The panel provides:

- Move, rotate and scale selected components along world, local or average-normal
  axes.
- Region or individual extrude, inset, bevel, ring cut, weld selected points and
  bridge two selected edge loops.
- Assign the material selected in the Materials tab to selected polygons.
- Subdivision-surface smoothing through the non-destructive modifier stack.

### Splines, booleans and skinning

- Spline objects have linear, Bézier and smooth modes, editable control points,
  closed/open curves and a viewport preview with control-point markers.
- The Boolean modifier is created with **Add Boolean** in the modifier stack and
  chooses another mesh as Union, Subtract or Intersect operand.
- Mesh nodes can be bound to all Joint objects in the scene from the **Skinning**
  section. Binding creates inverse-bind matrices and starts every vertex fully
  weighted to the first joint; the evaluation path supports up to four weights.

### Viewport controls

| Input | Action |
| --- | --- |
| Left mouse | select object or mesh component; drag the object gizmo |
| Middle mouse drag | orbit |
| Shift + middle drag | pan |
| Right mouse drag | pan |
| Mouse wheel | zoom |
| `F` | frame selected |
| `Cmd/Ctrl+Z` | undo |
| `Cmd/Ctrl+Shift+Z` | redo |
| `Delete` | delete selected object |
| View menu | shading mode, grid, helpers |

## Headless CLI

```bash
# Path-traced PNG or EXR
leopardcraft --headless render \
  --scene scene.lcscene --out render.exr \
  --width 1280 --height 720 --samples 128 --seed 7

# Native scene or interchange export
leopardcraft --headless export --scene scene.lcscene --out model.glb

# Built-in deterministic smoke test
leopardcraft --headless selftest
```

Without `--scene` the CLI renders or exports the built-in studio. Render options
also include `--time` for animation frames. The path tracer uses deterministic
per-pixel seeds and progressive accumulation, so a given scene, time, seed and
sample count produce the same image.

## Rendering

The real-time viewport is a wgpu render pass into an offscreen texture that egui
composites with overlays. The offline renderer is a CPU path tracer with:

- perspective, orthographic and panoramic camera ray generation;
- environment/HDRI backgrounds and sun/point/area lights;
- next-event estimation, GGX specular, diffuse and dielectric transmission;
- reflections, refraction, shadows, depth of field and progressive accumulation;
- ACES tone mapping to PNG and linear half-float scanline EXR output.

## Interchange

| Format | Import | Export |
| --- | --- | --- |
| `.lcscene` | yes | yes |
| OBJ + MTL | yes | yes |
| STL (binary/ASCII) | yes | yes |
| glTF / GLB | yes | yes |

USDZ and Collada are not implemented. The native scene format is the only format
that preserves the complete document: hierarchy, tags, modifiers, animation,
skinning and shader graphs.

## Clean-room statement

This project does not contain, link to, inspect or reverse-engineer proprietary
code, binaries or file internals. The UI workflow is inspired by publicly
described polygon-modelling conventions. Geometry algorithms, the scene model,
shading graph, renderer and file parsers were designed from public graphics
mathematics and open format specifications, with original code.

## Status and limitations

LeopardCraft is an early 0.1 implementation. The engine and document model are
usable and covered by tests, but the editor is not yet a replacement for a mature
production modeller. Known limitations:

- Boolean and subdivision operations favour robustness over speed on large meshes.
- The viewport shading is a lightweight material preview, not the offline path
  tracer; the Render tab provides the final image.
- glTF export covers meshes, transforms and core PBR material values; advanced
  extensions, animations and skinning are not part of the interchange path yet.
- There is no external file-picker dependency; file paths are entered in the
  in-app dialog.
- GPU compute baking is not implemented; offline rendering is CPU-side.
- The 0.1 tree is native-first; a WASM/WebGPU application target is not wired up
  yet.
- Render export is single-frame. Animation playback is available in the editor,
  but there is no batch image-sequence render command yet.
- Skinning currently provides bind-to-joint setup and runtime evaluation; a
  weight-painting brush is not implemented.

## License

Licensed under either of:

- Apache License, Version 2.0 (`LICENSE-APACHE`)
- MIT License (`LICENSE-MIT`)

at your option.
