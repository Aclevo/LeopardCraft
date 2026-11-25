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

Prebuilt Linux (AppImage, .deb, .rpm, tarball), macOS (Apple silicon and Intel) and Windows builds for 0.3.0 are on the [releases page](https://github.com/Aclevo/LeopardCraft/releases/tag/v0.3.0); the steps below build from source.

```bash
cargo run -p leopardcraft
```

The editor opens with a ready-to-use studio scene: camera, three-point lighting,
ground, beveled cube, animated sphere and torus, glass sphere, checker panel and
six materials. New scenes can add box, sphere, capsule, cylinder, cone, disc,
torus and plane primitives with live parameters.

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
| `lc-mcp` | Model Context Protocol stdio server for scene inspection, editing, save/export and rendering |
| `lc-ui` | Cheetah-style fixed egui workspace, wgpu viewport, inspectors, material graph, UV view, timeline |
| `app` | `leopardcraft` native binary and headless CLI |
| `scripts` | Full verification gate and the cwd-independent MCP launcher |

AI coding agents should start with [`AGENTS.md`](AGENTS.md) for the architecture
map, task recipes and conventions; `scripts/check.sh` runs the full workspace
gate, including a real MCP handshake smoke test. The repository is split into
six agent lanes with scoped briefs under [`docs/agents/`](docs/agents/README.md);
`scripts/check-lane.sh <lane>` runs one lane's fast gate, `.claude/agents/`
exposes the same lanes as Claude Code project subagents, and `.agents/` provides
a Codebuff CLI coordinator plus the six lanes. The project-scoped
`.mcp.json` exposes the MCP server to compatible clients.

The native scene format is pretty-printed JSON with a format/version header. It is
validated and sanitized on load. Document signatures ignore allocator counters, so
save/load round-trips can be compared deterministically.

## Editor

The fixed workspace is laid out like a classic Mac modeller: a menu bar and
original vector-icon toolbar, a `3D view | UVs | Nodes` switcher above the
viewport, a Properties column over an `Objects | Morphs | Takes | Layers |
Console` browser, a material-sphere shelf, a full-width timeline and a separate
Render Manager window.

- **3D Viewport** — wgpu shaded/wireframe display with a blue-grey gradient,
  light floor grid, white selected wireframe, light and camera helpers,
  orbit/pan/zoom navigation and a transform gizmo. Alt+drag orbits or pans.
- **Multi-object selection** — Shift/Cmd-click adds or removes objects in the
  viewport or Object Browser; drag in the viewport to box-select (Shift/Cmd
  adds). The last clicked object stays the inspector's primary. The gizmo
  moves, rotates or scales the whole selection around a shared pivot, and
  Delete, Duplicate, visibility, render and lock changes apply to the whole
  selection as one undo step. Group Selection and Selected Flags are in the
  Objects menu; the Object Browser context menu can also reparent or unparent
  the selection.
  Convert to Editable Mesh, Add Subdivision, Add Boolean Subtract, every UV
  projection operation and Key Transforms also apply to the whole selection as
  one undo step. Context-menu Frame Selected keeps the existing selection and
  frames all of it rather than collapsing to the right-clicked row; mesh-only
  menu and toolbar actions are disabled when the selection contains no mesh,
  and UV projection requires every selected mesh to be editable. Selected Flags
  are enabled per flag only while some selected object still needs that change,
  and Key Transforms only while some selected transform is not already keyed at
  the current frame.
  Component modes accept any selection made only of meshes; entering a mode
  converts every selected primitive in one undo step, so primitives and
  editable meshes can be edited together. Select All Components and Deselect
  All Components act on every selected mesh, and the topology tools either
  apply to every selected mesh with one undo step or are disabled/refused when
  any selected mesh lacks the required component selection. Mixed selections
  containing lights, cameras or other non-mesh objects are refused.
- **Object Browser** — hierarchical scene tree with type icons, visibility,
  render and lock toggles, modifier rows, and a layer manager. Its **Morphs**
  tab lists every mesh's morph targets under the owning object with weight and
  On/Off state; clicking a row selects that object without copying the data.
- **Properties / Tags** — icon tabs for object, material and render settings;
  object, camera, light, spline and primitive parameters; modifier stack; tags;
  skin binding and weight painting; mesh-editing tools; spline control points.
  Modifier, skin and direct UV-point controls edit the primary object only.
- **Node Materials** — material shelf with shaded sphere previews and an
  editable light-canvas shader graph with Principled BSDF and noise, voronoi,
  checker, color-ramp, image-texture, RGB, value and bump nodes; the graph also
  evaluates gradient, texture-coordinate, geometry, fresnel, normal-map,
  mapping, math and mix nodes stored in a material. Material roughness and
  metallic honour material-scope animation keys during evaluation.
- **UV Editor** — planar/box/cylindrical/spherical projection, auto-unwrap and
  relaxation apply to every selected editable mesh. Edges marked as UV seams
  with mesh-edge **Toggle Seam** (Mesh Editing Edge mode, `Selection -> Toggle
  Seam` or the Selection menu) turn green in the 3D view and are drawn green
  over the UV wireframe; auto-unwrap never crosses a seam, so seams are how a
  closed or smoothly curved surface is cut into non-overlapping islands, and
  relaxation runs per chart so it never averages the two sides of a seam onto
  one UV point.
  The UV view selects several points (shift/command-click adds or removes one;
  **Select All**, **Select None** and **Island** cover the rest) and authors
  seams itself: select two adjacent UV points on a mesh edge and press **Toggle
  Seam**, or use **All Seams**, **Seam Loop** and **Clear Seams** (also under
  `Tools -> UV Editing`). Each of those seam edits is one undo step.
  **Toggle Pinned UV** pins every selected point yellow; unwrap, relax and
  normalize keep pinned points exactly where they are while everything else
  re-solves, and pins are saved with the document. Direct UV-point dragging
  moves every selected point on the primary editable mesh.
  Chart controls move, rotate and scale the charts touched by the selection.
  **Pack Charts** shelf-packs the selection (or every chart when nothing is
  selected) with a padding field, optional 90° chart rotation and bottom-left,
  top-left or centred alignment; a chart that contains pinned points is left
  exactly as authored and the status line counts it. The slot operations
  **Weld**, **Split**, **Align U/V**, **Distribute U/V**, **Flip U/V**, **Rotate
  90°** and the numeric U/V entry each edit only UV coordinates or the shared
  UV-slot topology, never geometry, and each is one undo step.
  A **UV0/UV1/…** selector switches the active set; **Add UV Set** duplicates
  the current layout for a lightmap or secondary texture and **Remove UV Set**
  drops a non-zero set. Projections author the active set; Unwrap, Relax and
  Normalize stay UV0 authoring tools and refuse on another set. Every set shares
  the same slot indices, so weld and split apply to all sets at once. An **Image
  Texture** node has its own **UV set** field, and the CPU renderer interpolates
  every set at the hit, so a texture or lightmap can sample UV0 or any authored
  extra set. The
  **Checker** overlay draws a denser UV checker and **Distortion** tints faces
  by their UV-to-surface area stretch; the footer reports mean/max distortion
  and how many faces exceed 2×. Both overlays are view state and never dirty
  the document.
- **Timeline / F-Curves** — transport, frame ruler, key diamonds, orange
  playhead, scrubbing, key insertion and an expandable curve editor. The
  timeline's **Range** menu edits playback start and end frames plus loop mode;
  the F-curve header exposes the same controls. Bounds stay finite and ordered
  (`0..=end`) and never dirty the document or history. Click a key to select it,
  drag it to a new frame, right-click it or use Delete Key to remove it. Selected keys can be set per key to Constant, Linear, Smooth or
  Spline from the F-curve toolbar or the **Animation -> Tangent -> Type** menu;
  the same menu's Left/Right Tangent entries set one tangent side. Selected
  Spline keys expose draggable Bezier tangent handles for scalar and
  morph-weight curves; Smooth re-derives automatic tangents, while a dragged
  Spline tangent becomes a hand-authored slope that survives later edits,
  native save/load and undo. Interpolation and Tangent Type changes apply to
  scalar object, tag/material and morph-weight curves together with quaternion
  rotation keys, each as one undoable selection edit. Rotation keys use
  Constant hold, Linear slerp, and Smooth/Spline cubic quaternion sampling;
  their per-side tangent types choose flat, secant, or Catmull-Rom controls,
  but they have no scalar handles to drag.
  Selected keys can be retimed by a factor with **Scale Keys**, around the
  earliest selected key; **Retime** in the Takes tab retimes the selected
  object's active-take clip around 0s. Times are not frame-snapped, and a
  collision within 0.001s or an out-of-range result rejects the whole edit.
  Scalar, tag/material and morph-weight keys move with quaternion rotation keys;
  hand-authored tangent slopes are rescaled by their incident segment duration
  and automatic tangents are re-derived. Each retime is one undo step and
  round-trips through native `.lcscene` files. Rotation is a four-component
  quaternion channel:
  Constant keys hold, Linear keys use shortest-path slerp, and Smooth/Spline
  keys normalise a component-wise cubic, so sign-opposed quaternions cannot
  flip or pop. One channel registry declares every animatable property, its scope
  (object, tag or material), applicability and component count; keys address a
  target, a registry channel and one component, and recording and the curve
  editor both resolve channels through that address. The curve editor's target
  selector switches between the primary object, each Copy Location tag and the
  selected material. Rows are grouped under their target object, tag or
  material and generated from the registry, including channels that have no
  keys yet; other animated targets appear as further groups with their keyed
  channels, and clicking one of their rows focuses it. Select a row and use
  `Key Channel` to record it. A **Filter** selector beside the target switches the list, the
  drawn curves and key picking between all registry channels, channels that own
  a selected key, and transforms only. A mesh with Morph targets also gets one
  **Morph Weight** row per
  named target, backed by the same curve/key machinery and clamped to the target
  range. The Takes tab is the document take manager: it lists every named take
  with its fps, creates/renames/duplicates/deletes/activates takes, and applies
  a per-take fps. The active take owns the node, tag and material clips that the
  rest of the editor sees, and switching takes restores that clip set. Each
  take action is one undo step, and changing the active take's fps retimes the
  playback start/end range from the document's frame bounds; finite fps clamps
  to 1–240 and non-finite values are refused. Old `.lcscene` files without a
  take list load into one default take.
  weight range during evaluation. Tag keys are stored in a per-tag clip and
  material keys in the material clip. Shift/command-click or drag a marquee over the curve view to
  select multiple scalar or quaternion keys; dragging one selected key moves the
  whole selection while preserving spacing. Copy/paste with the header buttons
  or Ctrl/Cmd+C and Ctrl/Cmd+V works across objects, tags and materials: keys
  keep their offsets from the earliest copied key, paste starts at the playhead,
  and colliding keys are overwritten. Selection, multi-key drag, paste and
  overwrite each collapse to one undo step through the same animation commands
  as single-key editing.
- **Render Manager** — capped job history with thumbnails and info, live
  progress, cancellation, and PNG/EXR export from background CPU path tracing.

### Document safety

- The title bar marks unsaved changes with `*`.
- New, Open and Quit ask before discarding unsaved work; the window close button
  follows the same path.
- Saves and exports are atomic: a flushed temporary file in the target
  directory replaces the target, so an interrupted write cannot truncate an
  existing scene or export.
- Native `.lcscene` saves use that atomic replace without a backup file. After
  the rename, the parent directory is flushed so the new entry is durable;
  platforms without directory syncing degrade to a no-op. A crash can leave
  only a hidden temporary sibling; the next save, opening the scene from any
  directory, or app launch in the scene's directory removes it. Recovery and
  directory-flush failures are warnings shown in the status bar and Console,
  and do not block startup, opening or saving.
- OBJ/MTL pairs commit transactionally. If a crash leaves transaction siblings
  behind, the next export to that pair — or app launch in the export directory
  or the opened scene's directory — restores the previous pair or keeps the
  fully installed pair, then removes the temporary and backup files.
- A failed write — empty target, missing directory, or I/O error — keeps the
  document dirty, returns any pending close/open confirmation, reports the
  error, and leaves no partial target or temporary file behind.
- While a scene has unsaved changes the editor writes a recovery snapshot every
  15 seconds to `$XDG_STATE_HOME/leopardcraft` (default
  `~/.local/state/leopardcraft`); `LEOPARDCRAFT_RECOVERY_DIR` overrides the
  location. The next launch offers to recover the snapshot. Saves, explicit
  discards and clean exits remove it.

### Mesh editing

Select one or more meshes, open **Properties / Tags → Mesh Editing** and press
**Enter Mesh Edit**. Primitives are converted to editable n-gon meshes first
(all selected primitives in one undo step). Click points, edges or polygons in
the viewport; Shift-click extends the selection. Component modes and Select
All/Deselect All Components cover every selected mesh, but require a mesh-only
selection. Topology tools validate the component selection on every selected
mesh before applying one combined undo step; they are disabled or refuse with a
status message rather than modifying only part of the selection. Ring Cut is
only offered when the cut plane strictly crosses geometry on every selected
mesh, Weld requires two distinct valid points on every mesh, and Bridge Loops is
only offered when every selected mesh has exactly two valid closed edge loops of
matching length. Each handler reuses its menu's predicate before touching
geometry, and Apply Transform is enabled only when the current mode, axis and
value would actually move components on every selected mesh.

Four pick-driven tools take viewport clicks instead of acting on the component
selection: choose a tool from the Mesh Editing **Click tool** selector or
`Tools -> Ring Cut/Weld/Bridge/Insert Point Tool`. Ring Cut cuts the plane
perpendicular to the clicked edge through its midpoint. Weld takes two point
clicks and merges them at the **To** target, the midpoint or the first clicked
point. Bridge takes one point on each of two open borders and bridges those
loops. Insert Point splits a clicked polygon into triangles around a new point,
or splits a clicked edge in both polygons that use it. Each click is one undo
step, ESC drops a pending first click, and the Scalpel and Fill Hole click tools
are mutually exclusive with these four.

The panel provides:

- Move, rotate and scale selected components on every selected mesh along world,
  local or average-normal axes.
- Region or individual extrude, inset, bevel, ring cut, weld selected points and
  bridge two selected edge loops.
- Toggle edge creases on the selected edges in Edge mode, or set a fractional
  **Weight** from 0 to 1 with **Set Crease Weight**. Creased edges draw blue in
  every selection mode, and subdivision blends the smooth and sharp rules by
  that weight, so 0.5 is exactly halfway between an uncreased and a fully
  creased edge.
- Toggle UV seams on the selected edges in Edge mode (also `Selection -> Toggle
  Seam`); seam edges draw green in the 3D view and over the UV wireframe, and
  automatic unwrap cuts charts along them.
- Fill an open hole from one selected border point in point, edge or polygon
  mode (**Fill Hole**, also `Tools -> Fill Hole`), or turn on the **Click tool**
  (also `Tools -> Fill Hole Tool`) and click any point on an open border; the
  tool stays active for further holes. The new polygon winds with its
  neighbours, creases, seams and UV pins are kept, and the panel reports how
  many open holes the selection touches. A hole whose border encloses a separate
  mesh island is refused with a clear status instead of covering the island;
  fill or remove the island first.
- Pin UV points from the UV view (**Toggle Pinned UV**); every selected point
  is pinned or unpinned as one undo step, pinned points draw yellow, and
  unwrap, relax and normalize leave them where they are.
- Select two adjacent UV points in the UV view and use **Toggle Seam** to mark
  that mesh edge as a seam; **All Seams**, **Seam Loop**, **Clear Seams** and
  the UV chart/slot tools live in the UV panel and `Tools -> UV Editing`.
- Add a second UV set from the UV panel for lightmaps or secondary textures;
  the set selector switches which coordinates the view edits, projections and
  new UV operations target, and all sets survive native save/load and glTF
  export as `TEXCOORD_0`, `TEXCOORD_1`, …. OBJ writes UV0 only and records the
  skipped sets in an export comment.
- Split the selected polygons into a new object (**Split**, also `Tools ->
  Split`). The copy keeps the source's transform, tags, material slots, UVs,
  creases, seams and pins; the source mesh is unchanged and one undo removes the
  copy.
- Clone selected polygons or points (**Clone**, also `Tools -> Clone`) in point
  or polygon mode. The **Clone** section sets Copies, Holes, Direction, Scale,
  Rotation, position/scale/rotation variation and Keep ratio; copies are
  transformed around the selection centroid with the last copy at the end of
  the Direction vector, and an empty selection clones every point or polygon.
  Variation uses a fixed deterministic stream, so repeated Apply gives the same
  result; cloned faces keep UVs, material slots and interior creases/seams, and
  one undo restores the exact prior mesh.
- Subdivide only the selected polygons (**Subdivide**, also `Tools -> Subdivide`)
  with a level count and Catmull-Clark, Stam-Loop or Linear scheme. The patch
  boundary is pinned to the original edges, bordering unselected faces receive
  the new collinear boundary points, and unselected vertices keep their exact
  positions, so the result is watertight with no crack at the selection border.
  One undo restores the exact prior topology.
- Convert the selected polygons to triangles (**Triangulate**, also `Tools ->
  Triangulate`). The **Split** selector chooses **Shortest Diagonal** (ear-clip
  along the shortest valid diagonal, the default) or **Quad Pair** (split a quad
  along its shorter diagonal); unselected polygons stay n-gons, corner UVs and
  material slots are reused, creases, seams and UV pins survive, and one undo
  step reverts the split.
- Cut one polygon between two boundary picks (**Scalpel**, also `Tools ->
  Scalpel`) in point or edge mode. Two selected edges of one polygon cut at
  their midpoints, and two selected points of one polygon cut between them. The
  cut replaces that polygon with the two faces on either side, inserts a real
  point for an edge pick, splits the picked edge for the neighbouring face as
  well so the mesh stays watertight, carries a seam or crease weight onto both
  halves of the split edge, and keeps corner UVs and the material slot. A cut
  that would leave a polygon with fewer than three corners is refused, and one
  undo step restores the prior mesh. The **Click tool** checkbox next to
  **Scalpel** (also `Tools -> Scalpel Tool`) instead cuts from viewport clicks:
  a click on an edge or point starts a cut, a second click on an edge or point
  of a polygon that holds the start splits it, and the second click's point
  becomes the next cut's start, so cuts chain until **Esc** resets the pending
  cut. The face under the pointer decides which polygon splits when the two
  picks share an edge, a pick that shares no polygon with the start is refused
  without losing the start, and each completed cut is one undo step.
- Reverse the winding of the selected polygons (**Flip Normal**, also `Tools ->
  Flip Normal`) so their normals, shading and exported faces point the other
  way. Only selected faces change, the selection survives so the command flips
  them back, and one undo step restores the exact prior winding.
- Collapse the selected points into one vertex at their centroid (**Collapse**,
  also `Tools -> Collapse`) in point mode. Faces keep the vertices they do not
  collapse, a face that loses an edge becomes a smaller polygon, pinched faces
  are removed, the selection follows the merged vertex and one undo restores the
  exact prior topology.
- Build a face from existing points (**Create Polygon**, also `Tools -> Create
  Polygon`): in point mode select the boundary points of an open region and the
  new polygon reuses those indices in adjacency order, with the point's UV and
  pin and the bordering material slot. No point or existing face changes, the
  new face becomes the selection and one undo removes it.
- Slide the selected points toward their neighbours (**Point Slide**, also
  `Tools -> Point Slide`) in point mode: each point moves the Fraction of the way
  to the centroid of the points its edges connect it to (a point with exactly two
  neighbours reaches the segment midpoint at 1), only positions change, and one
  undo restores the exact prior positions.
- Weld the selected points that sit within a tolerance (**Optimize**, also
  `Tools -> Optimize`) in point mode: the Tolerance field decides what counts as
  one vertex, faces are rebuilt around the survivors, pinched faces are removed,
  the selection follows the surviving points and one undo restores the exact
  prior topology.
- Detach the selected polygons from their neighbours (**Disconnect**, also
  `Tools -> Disconnect`) without moving anything: the points they share with
  unselected faces are duplicated, UVs, creases, seams, pins and material slots
  follow their faces, the selection stays on the detached island and one undo
  restores the exact prior topology.
- Close the hole that a ring of selected polygons borders (**Cover**, also
  `Tools -> Cover`): each completely ringed open boundary becomes one new
  polygon reusing the border points and their order, with the bordering material
  slot and its own UVs, so the mesh is watertight again. The new faces become
  the selection and one undo removes them.
- Build a wall from the selected polygons (**Shell**, also `Tools -> Shell`) with
  a thickness field, the documented **Inside** side switch and the Angle and
  Quantize properties; the panel reports how many polygons the region covers and
  one undo step removes the wall.
- Assign the material selected in the Materials tab to selected polygons.
- Subdivision-surface smoothing through the non-destructive modifier stack.

### Splines, booleans, skinning, IK and morph targets

- Spline objects have linear, Bézier and smooth modes, editable control points,
  closed/open curves and a viewport preview with control-point markers.
- The modifier stack also includes **Twist**, **Shear**, **Bulge** and
  **Spherify** deformers with per-modifier axis, angle, distance, radius and
  strength controls, plus **Displacement** (normal offset from a deterministic
  noise field: amplitude, frequency, seed), **Formula** (one expression per axis
  in `x`, `y` and `z` using `+ - * / ^`, parentheses, `pi`/`e` and a fixed
  function set), **Magnet** (signed strength and radius falloff around the
  object centre), **Laplacian Smooth** (iterations, strength and a Keep border
  switch), **Polygon Reduction** (collapse the shortest triangle edge until the
  face count reaches a ratio) and **Wrap** (rotate the object around another
  mesh's centre line by an angle spread along an axis). The **Subdivision
  Surface** modifier offers Catmull-Clark,
  Stam-Loop and Linear schemes, defaults to two iterations, and honours edge
  creases under the two smooth schemes, including fractional crease weights.
  Stam-Loop keeps a quad cage quad and splits each triangle into three quads,
  then uses Catmull-Clark masks on quad rings, Loop masks on triangle rings and
  the published mixed mask where the two meet, so a mixed cage never gets
  triangulated; Linear subdivision increases density without smoothing the
  surface, keeps the mesh watertight and ignores crease sharpness.
- The Boolean modifier is created with **Add Boolean** in the modifier stack and
  chooses a mesh as Union, Subtract or Intersect operand; **Add Wrap** picks the
  mesh it wraps around. Add Modifier enables only entries whose geometry input
  exists: mesh modifiers on meshes, Sweep on splines, and Lathe everywhere;
  Boolean and Wrap additionally require a valid mesh target, and a Formula with
  a malformed expression is disabled until it parses. Adding and removing a
  modifier each commit one edit.
- Mesh nodes can be bound from the **Skinning** section to a chosen set of Joint
  objects using the All/None controls and per-joint checklist. Binding creates
  inverse-bind matrices for just those joints. With **Automatic proximity
  weights** enabled, each vertex receives up to **Max influences** normalized
  weights from its distance to the selected joints' bone segments; **Dropoff**
  and **Strength** control how sharply distant joints lose influence. With it
  off, vertices start unweighted and stay at rest until painted. Rebinding keeps
  every existing positive weight whose joint is still selected, auto-weights
  only vertices with no carried influence, and leaves uncovered vertices
  unweighted rather than falling back to the root joint. Binding and rebinding
  require a non-empty primary mesh and at least one chosen joint; a rebind that
  would reproduce the current skin is refused without adding a history entry.
- A Joint can carry an **Inverse Kinematics** chain in its inspector. The tip
  joint's origin is the end effector and **Chain length** counts movable Joint
  ancestors above it. Choose a target and optional pole, then tune iterations,
  tolerance and CCD damping. Exactly two movable joints use a closed-form
  two-bone solve whose pole chooses the bend plane; all other chain lengths
  (including one-joint chains) use damped CCD. Bones never stretch: unreachable
  targets clamp to the chain's reach and degenerate chains leave the pose finite.
  The same Joint inspector has optional **Joint Rotation Limits** in local XYZ
  degrees; they clamp direct joint posing and both IK paths, and a zero-width
  axis locks that axis. **Bake IK take** keys every solved frame from playback
  start to end into the existing rotation F-curves in one undo step (capped at
  10,000 frames per bake), and skinned meshes deform through the solved joints
  during playback. Set **Spline** to a Spline object and the chain follows
  that guide curve instead of the point target: each bone is aimed along the
  curve, bones keep their length, a guide shorter than the chain clamps to its
  end, joint limits still clamp, and a non-spline Spline target falls back to
  the point target. **Stiffness** blends each solved joint back toward its rest
  rotation: 1 is a full solve and 0.5 lands halfway, for point, pole and spline
  chains alike. The Tags section also offers the clean-room **IK Point** (pull
  the origin toward a target by weight), **IK Angle** (keep the local +X axis
  inside a cone around the direction to a target), **Orient** (blend toward the
  target's world rotation) and **Parent** (follow the target's position and
  rotation) tags, each with Target and Weight/Cone rows. The broader constraint
  system remains open.
- A bound editable mesh with no modifier stack can enter **Weight Paint** mode
  from the viewport brush icon or the Skinning section. Choose a joint and paint
  Add, Subtract, Replace or Smooth strokes with radius, strength and
  constant/linear/smooth falloff. Vertices show a heat map for the active joint,
  weights are auto-normalized and pruned to four influences after every stamp,
  and one undo step covers a whole stroke. The active joint can also be chosen
  by clicking its viewport handle; unbound joints are not pickable, and an empty
  viewport click leaves the chosen joint in place. Heat-diffusion weighting
  remains open.
- Editable meshes have a **Morph Targets** inspector. Record the rest base, move
  vertices, capture the pose as a named target, then blend targets with 0–1
  weights, update the rest base in place (targets keep their posed shapes) or
  restore the base; each target's weight can also be recorded and
  edited as a Morph Weight F-curve row. The browser's **Morphs** tab lists the
  same targets scene-wide with weight and On/Off state and selects the owning
  object. Targets are additive and one **Edit Morph** undo step covers capture,
  rename, weight drags and removal; loading validates topology, finite deltas
  and weights.

### Viewport controls

| Input | Action |
| --- | --- |
| Left mouse | select an object or mesh component; Shift/Cmd-click toggles multi-selection; drag empty space to box-select; drag the gizmo to transform the selection |
| Left mouse on a skin joint handle | make that bound joint the active joint for the Skinning panel, heat map and weight brush |
| Left mouse in Weight Paint | paint the active joint; drag for a continuous stroke |
| Middle mouse drag | orbit |
| Shift + middle drag | pan |
| Right mouse drag | pan |
| Mouse wheel | zoom |
| `F` | frame selected |
| `Cmd/Ctrl+Z` | undo |
| `Cmd/Ctrl+Shift+Z` | redo |
| `Delete` | delete selected objects |
| `Cmd/Ctrl+D` | duplicate selected objects |
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

# Non-interactive document lifecycle harness (dirty close/quit, Save As,
# write-failure recovery, timed autosave, snapshot recovery)
leopardcraft --headless lifecycle
```

Without `--scene` the CLI renders or exports the built-in studio. Render options
also include `--time` for animation frames. The path tracer uses deterministic
per-pixel seeds and progressive accumulation, so a given scene, time, seed and
sample count produce the same image.

## MCP server

`leopardcraft --mcp` runs an offline Model Context Protocol server over stdio, so
an MCP-compatible agent or editor can inspect and edit a LeopardCraft document,
save or export it, and render previews. It loads the built-in studio by default;
add `--scene <path>` to open a document first.

Typical client configuration:

```json
{
  "mcpServers": {
    "leopardcraft": {
      "command": "/absolute/path/to/LeopardCraft/scripts/mcp-server.sh",
      "args": ["--scene", "/path/to/scene.lcscene"]
    }
  }
}
```

The committed `.mcp.json` uses `${CLAUDE_PROJECT_DIR:-.}/scripts/mcp-server.sh`.
Claude Code runs project-scoped stdio servers with the repository root as their
working directory and sets `CLAUDE_PROJECT_DIR` in the server environment; the
fallback keeps the config valid while it loads. The wrapper then resolves its own
location, changes to the repository root and pins the workspace manifest and
`leopardcraft` package. Claude Code asks for approval the first time it sees the
project-scoped server; until then it reports `⏸ Pending approval`. Other clients
can use the absolute wrapper path shown above, which does not depend on the
client's working directory. The wrapper builds a missing or stale binary on first
use, so `cargo` must be on `PATH`; `LEOPARDCRAFT_MCP_BIN` overrides the binary for
packaging or alternate builds. The launcher is a Bash script, matching the
repository's other Unix tooling (Git Bash on Windows).

The server implements both MCP protocol generations: the `2025-06-18`
`initialize` handshake and the handshake-free `2026-07-28` revision, including
`server/discover`. Exposed tools:

- **Inspect** — `get_scene`, `list_objects`, `get_object`, `get_animation`,
  `get_mesh`, `get_material`, `list_materials`.
- **Edit** — `create_primitive` (all eight primitives with parameters and local
  transform), `set_transform`, `rename_object`, `delete_object`,
  `set_object_properties` (visibility, render, lock and material assignment),
  `add_modifier`, `update_modifier`, `remove_modifier`, `move_modifier`,
  `create_material`, `update_material`, `set_animation_key`,
  `remove_animation_key`, `create_take`, `activate_take`, `update_take`,
  `delete_take`, `edit_mesh`, `edit_uv`, `undo`, `redo` and `select_objects`
  (object selection in an attached editor). Modifier tools
  accept the same snake_case objects `get_object` reports, check the target and
  parameters before editing, and record one undo entry per call. Material tools
  accept the complete shader graph `get_material` returns (nodes, links and
  `next_id`), validate node ids, links and finite parameters, and also record
  one undo entry per call. Animation tools key object channels in the active
  take, use the same translation/rotation/scale/FOV/light/morph vocabulary
  evaluation uses, and edit takes with one undo entry per call. Mesh tools take
  an explicit per-call point, face, UV-slot or chart selection; `edit_mesh`
  transforms components and runs direct face/point operations, while `edit_uv`
  projects, unwraps, relaxes, normalises, packs and transforms charts, edits UV
  sets, coordinates, seams and pins, and welds/splits/aligns/distributes/flips/
  rotates slots. A primitive is converted to an editable mesh in the same undo
  entry, and validation happens before any command.
- **Files and output** — `save_scene`, `open_scene`, `new_scene`, `export_scene`
  (OBJ, STL, glTF/GLB or native) and `render_image` (PNG or EXR with optional
  size, sample, seed and time overrides).

The resources `leopardcraft://scene` and `leopardcraft://scene/summary` expose
the full document JSON and a compact summary. Object arguments accept a numeric
id or an exact name. Edits go through the editor's undo commands, so `undo`
reverses one tool call at a time, and the server tracks the document's path and
dirty state across saves. Mesh edits use explicit component indices supplied
with each request instead of a persistent editor selection, so the same payload
works headlessly. Without `--attach` the server is a separate headless process
with its own document; `--attach` points it at a running editor session instead.

### Attaching to a running editor

**Tools -> Live Agent Bridge** in the editor starts an opt-in loopback endpoint
and prints its port in the status bar; the menu item shows a check mark while it
is on, and turning it off (or closing the editor) closes the endpoint and removes
its registry entry. Nothing is opened by default, and the listener binds
`127.0.0.1` only.

The endpoint is HTTP/JSON at `POST /command` and its trust model is deliberately
small: a 32-byte token generated per session from the operating system entropy
source, which every request must send as `Authorization: Bearer <token>`, and an
`Origin` allow-list that accepts only loopback origins (or no `Origin` at all, as
native clients send). The token is published in a `0600` registry next to the
port and the editor's pid — `$XDG_RUNTIME_DIR/leopardcraft/editors.json` by
default — so several open editors stay addressable. The file is atomic-written
and `LEOPARDCRAFT_BRIDGE_REGISTRY` moves it.

`leopardcraft --mcp --attach` (also with `--mcp-http`) finds that registry and
attaches. The server then routes every tool call, `undo`/`redo`, the scene
resources and `select_objects` to the editor's live document and selection; each
edit is one entry in the editor's own undo history, and `get_scene` gains a
`selection` field. The editor's UI thread answers one request at a time, so a
busy or dialog-blocked editor answers `503 editor busy` instead of racing the
document, and a refused request changes nothing. The server's private document
stays as the fallback for a session that is not attached.

`LEOPARDCRAFT_EDITOR_ENDPOINT` (e.g. `http://127.0.0.1:47011`) plus
`LEOPARDCRAFT_EDITOR_TOKEN` pin one editor directly and win over the registry;
without them, `--attach` refuses to guess when more than one editor is running.

### HTTP transport, subscriptions and prompts

stdio is the default transport. `leopardcraft --mcp-http [--addr <host:port>]`
serves the same protocol over the MCP Streamable HTTP transport instead: `POST
/mcp` carries one JSON-RPC message per request (a notification is answered with
`202 Accepted` and no body) and `GET /mcp` with `Accept: text/event-stream`
opens the server's Server-Sent Events stream. The `initialize` response also
carries an `Mcp-Session-Id`, every later request has to repeat it, a missing
header is answered with `400`, an unknown or terminated id with `404` (so the
client initializes again), and `DELETE /mcp` ends a session. Reconnecting an
event stream with a live id keeps receiving the document's resource updates,
because subscriptions belong to the server rather than the connection. It
prints the endpoint URL to stdout when it starts and binds only loopback
addresses (`127.0.0.1:8765` by default), so enabling it never exposes the
project to the network; a non-loopback address is refused at startup.

Clients can call `resources/subscribe` and `resources/unsubscribe` for
`leopardcraft://scene` and `leopardcraft://scene/summary`. Every tool call that
changes the document then produces `notifications/resources/updated` for the
subscribed URIs, written to the event stream on the HTTP transport or as an
extra newline-delimited protocol line on stdio. Three prompt templates are
available through `prompts/list` and `prompts/get`: `scene_overview`,
`object_report` (argument `object`, an id or exact name) and `render_check`.
Each prompt is filled from the live document, so its text describes the scene as
it stands when the prompt is requested.

`app/tests/mcp_smoke.rs` launches the real binary from a temporary directory,
completes `initialize` and `tools/list`, runs representative tools, compares the
advertised inventory with `AGENTS.md`, and launches the `.mcp.json` command with
the project-root variable expanded to an absolute path. `scripts/check.sh` runs
that smoke test explicitly. The test acts as the MCP client. The exact command
was also health-checked successfully by Claude Code 2.1.327 through a disposable
local-scope entry; Claude Code discovers the project entry from the repository
root and a subdirectory, and requires its normal first-use approval before
connecting it. A third-party host is not part of the automated check.

## Rendering

The real-time viewport is a wgpu render pass into an offscreen texture that egui
composites with overlays. The offline renderer is a CPU path tracer with:

- perspective, orthographic and panoramic camera ray generation;
- environment/HDRI backgrounds and directional/sun, point, area and spot
  lights (point and spot radii sample as spherical soft-shadow lights, and spot
  cones have an inner/outer angle ramp);
- next-event estimation, GGX specular, diffuse and dielectric transmission;
- reflections, refraction, soft shadows, depth of field and progressive
  accumulation;
- ACES tone mapping to PNG and linear half-float scanline EXR output.

## Interchange

| Format | Import | Export |
| --- | --- | --- |
| `.lcscene` | yes | yes |
| OBJ + MTL | yes | yes |
| STL (binary/ASCII) | yes | yes |
| glTF / GLB | yes | yes |

glTF/GLB export writes bind-pose skinned meshes with `JOINTS_0`/`WEIGHTS_0` and
inverse-bind matrices, named morph targets with position/normal deltas, and a
merged animation named after the active document take, sampled on that take's
fps, for node translation/rotation/scale and morph weights; the take name and
fps are also written to the animation and root `extras`.
It samples all channels on one seconds-based grid and preserves uniform
CONSTANT/LINEAR channels; all-Smooth/Spline scalar channels become sampled
CUBICSPLINE, while mixed channels and smooth rotations are sampled LINEAR. glTF
import reads meshes, materials, transforms, a mesh's morph base, target names,
offsets and default weights, cameras (perspective and orthographic, with field
of view and clip planes), every `KHR_lights_punctual` light type (colour,
intensity, spot cone angles converted from radians to degrees, and `range` as
the light's falloff radius), `skins` (joint nodes, inverse-bind matrices and
four-influence `JOINTS_0`/`WEIGHTS_0`) and `animations`. Animation channels for
translation, rotation, scale and morph weights keep STEP, LINEAR and
CUBICSPLINE timing (cubic tangents become hand-authored Spline slopes; cubic
rotation becomes Smooth), the first animation fills the active take and further
animations become additional takes, and each take's frame range comes from the
clip duration at its fps. A camera `aspectRatio` hint is still rewritten and
reported; if a file carries one, the editor shows the report **before** the
scene is replaced and the Open/Import buttons wait for confirmation. Tags,
constraints and material animations are not exported.
USDZ and Collada are not implemented. The native
scene format remains the only format that preserves the complete document:
hierarchy, tags, modifiers, animation, skinning, morph targets and shader
graphs.

## Clean-room statement

This project does not contain, link to, inspect or reverse-engineer proprietary
code, binaries or file internals. The UI workflow is inspired by publicly
described polygon-modelling conventions. Geometry algorithms, the scene model,
shading graph, renderer and file parsers were designed from public graphics
mathematics and open format specifications, with original code.

## Status and limitations

LeopardCraft is an early 0.2 implementation. The engine and document model are
usable and covered by tests, but the editor is not yet a replacement for a mature
production modeller. The researched Cheetah3D feature matrix is in
[`docs/cheetah3d-parity.md`](docs/cheetah3d-parity.md). Known limitations:

- Boolean and subdivision operations favour robustness over speed on large meshes.
- The viewport shading is a lightweight material preview, not the offline path
  tracer; the Render tab provides the final image.
- glTF export covers meshes, transforms, core PBR values, bind-pose skins,
  named morph targets and object transform/morph-weight animation; import now
  reads skins and animation channels too. Advanced extensions, material
  animation and tags/constraints remain unsupported.
- There is no external file-picker dependency; file paths are entered in the
  in-app dialog.
- The MCP server is a separate headless process over stdio, or over the opt-in
  loopback Streamable HTTP transport. It edits its own in-memory document and
  writes files, but does not attach to a running editor window.
- GPU compute baking is not implemented; offline rendering is CPU-side.
- The 0.2 tree is native-first; a WASM/WebGPU application target is not wired up
  yet.
- Render export is single-frame. Animation playback is available in the editor,
  but there is no batch image-sequence render command yet.
- Skinning provides chosen-joint bind/rebind, deterministic normalized proximity
  auto-weights (Dropoff, Strength and up to four influences), a four-influence
  weight-paint brush (add/subtract/replace/smooth), active-joint heat map and
  per-stroke undo. Existing painted weights survive a rebind while their joints
  remain selected. IK chains drive the same joints and deformed meshes, but
  heat-diffusion weighting and the broader constraint system are not
  implemented; the IK Point/Angle/Orient/Parent tags and the chain stiffness are
  clean-room approximations rather than Cheetah3D's IK Constraint tag, whose
  documented stiffness of one means the joint does not bend; Spline IK follows a
  guide curve as a chain of chords and has no twist control.
- glTF skins are skipped when a modifier stack changes the bind-mesh topology,
  and morph targets are skipped whenever a modifier stack must be baked or the
  morph base no longer matches the exported rest mesh.
- Morph targets and their weight curves are authored, evaluated, exposed as
  native F-curve rows and exported to glTF. **Update Base** re-records the rest
  shape while keeping each target's absolute posed vertices; the manual does
  not define how targets follow a re-recorded base, so the rebase is a
  clean-room choice. The manual also does not document a numeric morph-weight
  range or morph-specific interpolation, so the 0–1 clamp and the shared
  Constant/Linear/Smooth/Spline key interpolation are clean-room choices rather
  than confirmed parity.
- Older documents that animate rotation as three Euler curves still load and
  evaluate; new recordings write the quaternion rotation channel. Those legacy
  curves are read as degrees, matching the inspector and the built-in scene.
  Release 0.1.x recorded them through a helper that stored radians, so rotation
  keys saved by those versions animated far too slowly and need re-recording;
  the loader cannot tell the two conventions apart.
- The F-curve editor exposes registry channels for the primary object, a Copy
  Location tag or the selected material. Tag targeting is only offered for tags
  that declare an animation channel. The **Filter** selector only changes what
  the editor lists, draws and picks; it never edits the document. Rotation keys
  do not expose scalar tangent handles.
- Additional UV sets are authorable, editable, saved and exported as glTF
  `TEXCOORD_1`…, and an Image Texture node can sample any of them; the CPU
  renderer interpolates every set at the hit. Lightmaps are therefore an extra
  UV set plus a texture node bound to it, not a dedicated lightmap slot.
- Animation keys address a target, registry channel and component. Files saved
  before this model used flat property names; the loader maps them to
  owner-relative addresses and keeps legacy Euler rotation curves in a
  compatibility field that still samples in degrees. glTF export writes the
  active take as node translation, rotation, scale and morph-weight channels;
  an addressed channel with no glTF destination is skipped and named in an
  export report instead of being written onto another node.

## License

Licensed under either of:

- Apache License, Version 2.0 (`LICENSE-APACHE`)
- MIT License (`LICENSE-MIT`)

at your option.
