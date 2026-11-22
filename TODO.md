# LeopardCraft — next improvements

To-do list for the five agreed workstreams. Boxes stay unchecked until an item's
full scope lands. Each item names the code that has to change so the list can be
worked top to bottom without re-deriving the plan.

**Progress:** wave 1 implemented per-key F-curve selection, frame-snapped
dragging, deletion with a single undo step, and per-key interpolation in the
editor (Animation Core → Editor). Wave 2 added the channel registry
(`AnimChannel` with object/tag/material scope and component counts), a
dedicated four-component quaternion rotation track with shortest-path slerp,
STEP and normalised cubic sampling, registry-driven transform recording,
quaternion key move/delete/interpolation in the F-curve editor, plus evaluated
Copy Location weight and material roughness/metallic channels.

Wave 3 made the registry the editor's row source for every scope. The F-curve
editor can target the primary object, an individual Copy Location tag or the
selected material; its rows, labels and component counts come from
`AnimChannel`, and empty channels are listed so they can be keyed. `Key Channel`
records the selected row at the playhead. Tag-scope keys live in per-tag clips
(`Node::tag_animations`, with a dedicated undo command) and material keys in
`Material::animation`; both support per-key select, drag, interpolation and
delete as one undo step. Material recording reads base graph values so a new key
is not overwritten by the previous animated value.

Wave 4 added multi-key editing. Shift/command-click and marquee drags select
scalar and quaternion keys; dragging any selected key moves the whole selection
and preserves spacing. Copy/paste uses registry channels, so keys move between
objects, tags or materials with time offsets preserved from the earliest copied
key. Paste starts at the playhead, overwrites colliding keys, and is one undo
step alongside box-selected drag and multi-delete. The pre-change failure runs
for tag/material rows and the new box/copy tests were captured as runtime
assertions, not compile errors.

**Still open in Animation Core:** scale-in-time, key filtering, tangent-handle
dragging, document-level takes and per-take fps remain. Rotation tangent handles
are not exposed; Smooth quaternion tangents are derived automatically.

## 1. Animation Core

Make animation data general enough to drive more than the nine transform channels,
and make the curve editor behave like a real key editor.

**Data model**

- [ ] Replace the flat `AnimProperty` enum (`crates/lc-anim/src/lib.rs`) with a
      channel address that names target + property + component, so material
      colour, modifier parameters and morph weights can be animated without
      growing the enum. Keep old `.lcscene` files loadable with a custom
      `Deserialize` that maps the current snake_case variant names.
- [x] Add quaternion rotation channels with shortest-path interpolation and a
      deterministic Euler/quaternion conversion at key time.
      `AnimationClip::apply_transform` now prefers the quaternion track and only
      falls back to legacy Euler curves for old documents.
- [ ] Make per-key tangents authoritative. `Curve::update_tangents` overwrites
      stored handles on every insert/remove, so hand-edited tangents cannot
      survive an edit.
- [ ] Move takes to document level instead of per node; the Takes tab currently
      states takes are stored per object.
- [x] Define one property registry used by Key Transforms, the curve editor and
      the evaluator, so a new animatable property is registered once. The
      registry also declares channel scope and applicability (object, camera,
      light, tag, material).

**Editor**

- [x] Select, box-select, move, copy/paste and delete individual keys, each as
      one undo command through the existing animation dispatch. Marquee bounds
      match both frame and displayed value; additive selection uses shift or
      command.
- [ ] Scale selected keys in time as one undo command.
- [ ] Drag Bézier tangent handles in the F-curve editor. Interpolation and
      deletion are per key now, but tangent handles remain implicit.
- [x] Per-key interpolation controls in the curve editor. Object, tag and
      material scalar keys can be selected, dragged, interpolation-cycled and
      deleted independently.
- [x] Expose every registry channel for the active object, tag or material as
      selectable F-curve rows with `Key Channel` recording.
- [ ] Key filter (all / selected / transform only).
- [ ] Edit playback range, per-take fps and loop mode; `Playback` currently has
      one global range and fps.
- [ ] Group the channel list by target object in the timeline.

**Evaluation and tests**

- [ ] Sample each channel once per frame into a scratch buffer instead of
      re-querying the clip channel-by-channel.
- [ ] Tests: old-document deserialisation, rotation continuity across ±180°,
      key-edit undo, deterministic sampling.

## 2. Weight Painting

Replace "bind everything to the first joint" with an authorable weight workflow.
Evaluation already supports four influences (`skin_positions_indexed`,
`crates/lc-scene/src/eval.rs`); nothing authors or validates those weights.

- [ ] Add a weight brush mode to the viewport that paints `Skin::weights`
      (`crates/lc-scene/src/scene.rs`) for the selected joint, with radius,
      strength and constant/linear/smooth falloff.
- [ ] Brush modes add / subtract / replace / smooth, with 4-influence pruning and
      normalisation after each stroke.
- [ ] Joint picking from the viewport or a joint list in the Skinning panel, plus
      a heat-map overlay for the active joint.
- [ ] One undo command per stroke that coalesces the whole drag, following the
      existing all-or-nothing `SetShapeCmd` pattern.
- [ ] Sanitise skins on load in `crates/lc-scene/src/format.rs`: finite
      non-negative weights, indices in range, lengths matching the vertex count,
      weights normalised.
- [ ] Bind workflow: bind to selected joints only, preserve unchanged weights on
      rebind, and offer proximity auto-bind as an alternative to
      `bind_all_to_first`.
- [ ] Tests: falloff determinism, normalisation and pruning, stroke undo, format
      round-trip.

## 3. UV Workflow

`automatic_unwrap` in `crates/lc-geom/src/uv.rs` already charts by normal angle
and shelf-packs the result; `set_uv_point` is the only direct edit. Build a real
UV toolset on top of that.

- [ ] Explicit seam edges: mark/unmark from selected mesh edges in the 3D and UV
      views, persist them on the mesh, and feed them to unwrap instead of relying
      only on the normal-angle threshold.
- [ ] Chart-aware relaxation. `relax_uvs` averages per point and re-fits the whole
      mesh; relaxation should run per chart with seams and chart borders pinned.
- [ ] Chart selection and chart move/rotate/scale, plus packing controls
      (padding, rotation, alignment) over the existing shelf packer.
- [ ] UV editing operations: island selection, weld/split UV vertices,
      align/distribute, flip, rotate 90°, numeric UV entry.
- [ ] Multiple UV sets per mesh (UV0/UV1) with a selector, usable by texture and
      lightmap slots.
- [ ] Viewport UV checker overlay and per-face distortion stats so unwrap quality
      is visible.
- [ ] Tests: seam-driven unwrap determinism, non-overlapping chart packing,
      per-chart relax stability, multi-set round-trip, UV undo.

## 4. Morph Targets

There is no morph storage anywhere in the tree today; this is new data, not a
rework.

- [ ] Data model: `MorphTarget { name, deltas: Vec<Vec3> }` on mesh nodes (or a
      deform component), with the delta count tied to the base vertex count.
- [ ] Document format and load-time validation in `crates/lc-scene/src/format.rs`,
      including stable signature coverage.
- [ ] One documented evaluation order in `crates/lc-scene/src/eval.rs`:
      primitives → modifiers → morph deltas → skinning → node transform.
- [ ] UI: target list with weight sliders, add/remove/rename, "capture current
      shape" from an edited mesh, and a clear refusal when a target no longer
      matches the vertex count.
- [ ] Undo commands for add/remove/rename and weight edits.
- [ ] Animatable morph weights once the Animation Core property registry lands.
- [ ] Tests: weighted delta application and clamping, determinism, format
      round-trip, undo, mismatched-length refusal.

## 5. glTF Animation and Skinning Interchange

Import and export are static today (`crates/lc-io/src/gltf.rs`): meshes,
transforms and core PBR values only. Close the loop with the native document.

- [ ] Export `skins`: inverse bind matrices as a MAT4 accessor, joint node
      indices, and `skin` on skinned mesh nodes, mapping `Skin::joints` to glTF
      node indices.
- [ ] Export `animations`: samplers and channels for translation/rotation/scale.
      Rotation depends on the quaternion channels above. Map smooth curves to
      CUBICSPLINE, or bake to sampled LINEAR where tangents do not fit the glTF
      in/out layout.
- [ ] Export morph targets once the morph model exists: `targets` on primitives
      and `weights` on nodes.
- [ ] Import `skins`, `animations` and morph targets into `Skin`,
      `AnimationClip` and morph weights, including CUBICSPLINE tangents and clip
      duration → fps.
- [ ] Round-trip tests for GLB and `.gltf` with a skinned, animated, morphing
      mesh, plus malformed accessors and out-of-range joints that must not panic.
- [ ] Update the README interchange table and limitations list once the path is
      complete.

## Suggested order

1. **Animation Core** — the channel model and quaternion rotation unblock morph
   animation and glTF rotation export.
2. **Weight Painting** and **UV Workflow** — independent of each other and of the
   interchange work; both are user-visible daily tools.
3. **Morph Targets**, then **glTF animation and skinning** — the interchange items
   depend on both the animation and morph models.
