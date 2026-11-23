# Cheetah3D feature parity

This is the clean-room parity map for LeopardCraft against [Cheetah3D](https://www.cheetah3d.com/).
The inventory was compiled on 2026-11-22 from Cheetah3D's public feature list,
release notes and manual for version **8.1.3** (12 March 2025). No proprietary
code, resources or file formats were examined.

Status legend:

- **Yes** — implemented and exercised by the normal editor or headless path.
- **Partial** — a working subset exists; the note names the missing part.
- **No** — not implemented.

Cheetah3D exposes two renderers, a large parametric-object and modifier
library, character animation (joints, IK, constraints, morphs, particles and
Bullet dynamics), a node-based material system, UV editing and broad
interchange. LeopardCraft is intentionally a smaller native modeller; the
table records the real gap rather than treating "a similar feature exists" as
parity.

## Sources

1. Official feature list — <https://www.cheetah3d.com/www4/featurelist.php>
2. News and release history — <https://www.cheetah3d.com/news.php>
3. Cheetah3D 7.0 release notes — <https://www.cheetah3d.com/news.php?n=47>
4. Manual mirror (Morph Tag) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tags/MorphTag/MorphTag.html>
5. Manual mirror (index) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual>
6. Current download and system requirements — <https://www.cheetah3d.com/download.php>
7. Khronos glTF 2.0 specification — <https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html>
8. Cheetah3D forum — vertex weighting workflow — <https://www.cheetah3d.com/forum/index.php?threads/4073/>
9. Manual mirror (Skeleton Tag) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tags/SkeletonTag/SkeletonTag.html>
10. Manual mirror (Vertex Weight Tool) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tools/Character/VertexWeightTool/VertexWeightTool.html>
11. Cheetah3D 4.1 release notes (2D IK solver and Take Manager bake command) — <https://www.cheetah3d.com/news.php?n=14>
12. Cheetah3D forum — IK Handle Target and Pole — <https://www.cheetah3d.com/forum/index.php?threads/12753/>
13. Cheetah3D forum — IK bake keys every frame — <https://www.cheetah3d.com/forum/index.php?threads/13467/>
14. Jukka blog — joint and IK Handle workflow, reported Length default 2 — <https://blog.jukkagraph.net/archives/4072>
15. Manual mirror (IK Constraint Tag, indexed as IK Limits) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tags/IKConstraintsTag/IKConstraintsTag.html>
16. Manual mirror (IK Handle Tag) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tags/IKHandleTag/IKHandleTag.html>
17. Cheetah3D forum — IK constraint applied to a joint — <https://www.cheetah3d.com/forum/index.php?threads/9364/>
18. Manual mirror (Timeline) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Animation/TimeLine.html>
19. Manual mirror (F-Curve Editor) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Animation/FCurveEditor.html>
20. Cheetah3D forum — morph-tag recording workflow — <https://www.cheetah3d.com/forum/index.php?threads/3728/>
21. Cheetah3D 7.3 release notes (glTF/GLB export) — <https://www.cheetah3d.com/news.php?n=50>
22. Cheetah3D forum — glTF import is not available — <https://www.cheetah3d.com/forum/index.php?threads/14050/>
23. Manual mirror (Key Editor) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Animation/KeyEditor.html>
24. Manual source (Take Manager) — <https://raw.githubusercontent.com/tgjp/Cheetah3D_jp/master/docs/Cheetah3Dmanual/Animation/TakeManager/TakeManager.html>
25. Cheetah3D forum — compressing a take with Scale Take.js — <https://www.cheetah3d.com/forum/index.php?threads/11591/>
26. Cheetah3D forum — retiming all keys with F-Curve Utility and native Split — <https://www.cheetah3d.com/forum/index.php?threads/13730/>
27. Manual source (F-Curve Editor, interpolation and Tangent commands) — <https://raw.githubusercontent.com/tgjp/Cheetah3D_jp/master/docs/Cheetah3Dmanual/Animation/FCurveEditor.html>
28. Manual source (Key Editor, no interpolation display) — <https://raw.githubusercontent.com/tgjp/Cheetah3D_jp/master/docs/Cheetah3Dmanual/Animation/KeyEditor.html>
29. Cheetah3D feature list — animation interpolation modes — <https://www.cheetah3d.com/featurelist.php>
30. Cheetah3D forum — default Smooth tangents and recording reset — <https://www.cheetah3d.com/forum/index.php?threads/12038/>
31. Manual source (Subdivision modifier, schemes and Iterations) — <https://raw.githubusercontent.com/tgjp/Cheetah3D_jp/master/docs/Cheetah3Dmanual/Objects/ModifierObjects/SubDiv/SubDiv.html>
32. Manual source (Twist modifier) — <https://raw.githubusercontent.com/tgjp/Cheetah3D_jp/master/docs/Cheetah3Dmanual/Objects/ModifierObjects/Twist/Twist.html>
33. Manual source (Shear modifier) — <https://raw.githubusercontent.com/tgjp/Cheetah3D_jp/master/docs/Cheetah3Dmanual/Objects/ModifierObjects/Shear/Shear.html>
34. Manual source (Bulge modifier) — <https://raw.githubusercontent.com/tgjp/Cheetah3D_jp/master/docs/Cheetah3Dmanual/Objects/ModifierObjects/Bulge/Bulge.html>
35. Manual source (Spherify modifier) — <https://raw.githubusercontent.com/tgjp/Cheetah3D_jp/master/docs/Cheetah3Dmanual/Objects/ModifierObjects/Spherify/Spherify.html>
36. Manual source (Polygon object, creases and smooth-shading modes) — <https://raw.githubusercontent.com/tgjp/Cheetah3D_jp/master/docs/Cheetah3Dmanual/Objects/PolygonObjects/Polygonobject.html>
37. Manual mirror (UV unwrapping tutorial: seams, pinning, Unwrap UV) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tutorials/Unwrap/Unwrap.html>
38. Manual mirror (Fill Hole tool: modes and click workflow) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tools/Polygon/FillHoleTool/FillHoleTool.html>
39. Manual mirror (Split tool: new object, modes, tags copied) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tools/Polygon/SplitTool/SplitTool.html>
40. Cheetah3D user forum, "Cutting objects in half" (Split leaves the source polygons in place) — <https://www.cheetah3d.com/forum/index.php?threads/3580/>
41. Manual mirror (Shell tool: wall workflow, modes, Inside/Angle/Quantize) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tools/Polygon/ShellTool/ShellTool.html>

## Matrix

| Area | Cheetah3D 8.1.3 | LeopardCraft | Status | Notes |
| --- | --- | --- | --- | --- |
| Modelling — primitives | 12 parametric primitives | 8: box, sphere, capsule, cylinder, cone, disc, torus, plane | Partial | No tube, polygon, subdivision or other creator objects |
| Modelling — splines | 9 spline types | 1 spline object with linear, Bézier and smooth interpolation | Partial | No separate Helix, Spiral, Text, Path or similar creators |
| Modelling — polygon tools | Extrude, inner extrude, bevel, bridge, collapse, cover, create polygon, crumple, disconnect, edge rotate, fill hole, flip normal, jigsaw, normal move/rotate/scale, optimize, point slide, ring cut, scalpel, shell, split, triangulate, weld, array and clone | Extrude, inset, bevel, ring cut, weld, collapse, bridge, fill hole, cover, create polygon, flip normal, split, disconnect, optimize, point slide, shell, triangulate, component move/rotate/scale, material assignment | Partial | No scalpel or clone; Fill Hole acts on the current component selection rather than a viewport click, Split copies the polygons instead of moving them (source 40), Shell uses typed thickness/Inside/Angle/Quantize fields instead of the documented drag gesture (source 41), and Triangulate fans from the polygon's first corner because the manual page is not among the indexed sources |
| Modelling — modifiers | Roughly 18 documented modifiers including subdivision, displacement, formula, magnet, polygon reduction, smooth and wrap | 15: subdivision (Catmull-Clark or Linear), extrude/bevel faces, array, symmetry, boolean, bend, taper, twist, shear, bulge, spherify, lathe, sweep, ring cut | Partial | No displacement, formula, magnet, polygon reduction, Laplacian smooth, wrap or cage/type controls; Boolean and subdivision favour robustness over speed |
| Modelling — subdivision | Catmull-Clark, Stam-Loop and linear subdivision; creased edges under the smooth schemes | Catmull-Clark, Stam-Loop and Linear schemes; per-edge creases toggled in Edge mode and honoured by Catmull-Clark and Stam-Loop | Partial | Stam-Loop is a clean-room Loop-on-triangulated-cage approximation, not a verified Stam implementation; no crease weights, partial subdivision or crease display outside Edge mode |
| Modelling — booleans | Union, subtract, intersect | Union, subtract, intersect BSP boolean modifier | Yes | Mesh operand is chosen in the modifier stack |
| UV | UV editor, cubic/spherical/flat/cylindrical/frontal mapping, ABF/LSCM unwrap, Autoseam, pinned UVs, UV PDF | UV view, planar/box/cylindrical/spherical projection, seam edges, seam-driven automatic unwrap and per-chart relax, pinned UVs | Partial | No ABF/LSCM, Autoseam, chart tools, multiple UV sets or UV PDF export; seams are authored in the 3D view only and the UV view pins one point at a time |
| Materials | Node materials, PBR/Principled workflow, procedural textures | Node material graph, Principled surface, procedural noise, texture cache | Partial | Core authoring path exists; not every Cheetah node type is present |
| Rendering — engines | Cheetah raytracer (irradiance cache, photon maps) and Falcon path tracer | One CPU path tracer with progressive accumulation | Partial | No second engine or photon-mapped caustics |
| Rendering — camera/lighting | HDRI/sky, area/spot/point/distant lights, depth of field, motion blur (Falcon), caustics | Environment/HDRI, sun, directional, point, area and spot lights (cone angle controls), depth of field | Partial | No motion blur or caustics; light radius is authored but soft shadows still sample the light exactly |
| Rendering — output | Up to 8K, G-buffer/AOV output, texture baking | PNG and EXR, single-frame background render at editable resolution | Partial | No 8K preset, G-buffer, baking or animation-sequence render |
| Animation — curves | Keyframes and F-curve editor; Constant, Linear (manual default), Smooth and Spline interpolation; `Animation -> Tangent -> Type`, `Left Tangent -> Type` and `Right Tangent -> Type` | Keyframes, F-curve editor, all four per-key interpolation modes, selection-wide `Tangent`/`Left Tangent`/`Right Tangent` Type menus, Spline tangent handles for scalar/morph curves, quaternion interpolation with per-side tangent types, key select/drag/copy/paste and selected-key factor retiming | Partial | Key filtering remains open; quaternion curves have no scalar handles; Cheetah3D calls Linear its default while LeopardCraft records new keys as Smooth |
| Animation — takes | Take Manager for multiple animation takes | Per-object animation clips; Takes tab reports the per-object model and can retime a whole take by a factor | Partial | No document-level takes or per-take fps; a take is still stored per object |
| Character — skinning | Joints, bind mesh with automatic weights, max influence/dropoff, weight painting with add/subtract brushes and auto-normalise | Joint objects, chosen-joint bind/rebind with normalized proximity auto-weights (dropoff/strength/max influences), four-influence paint brush, active-joint heat map | Partial | No viewport joint picking or heat-diffusion solver; selected-joint and preserve-on-rebind behavior is a clean-room extension; brush UI details are native |
| Character — rigging | FK, IK, spline IK and constraints | Joint transforms; tip-origin IK chains with target/optional pole, analytic two-bone and damped CCD solving, reach clamping, per-joint local XYZ rotation limits and per-frame take baking | Partial | The limits are a clean-room local-Euler approximation, not Cheetah3D's IK Constraint tag/heading-pitch-bank semantics or documented 3D-only behavior; no Spline IK, IK Angle/Point constraints, stiffness, or viewport joint picking; bake uses the global playback range and writes ordinary rotation F-curves |
| Character — morphs | Morph tag with recorded base and named target poses | Morph targets with recorded base, named poses, capture/restore and undo; dynamic Morph Weight F-curve rows, evaluation with 0–1 clamping, native round-trip and glTF export | Partial | Weight rows use LeopardCraft's F-curve interpolation and a clean-room 0–1 range; Cheetah3D documents neither the numeric range nor morph-specific interpolation; glTF import now reads morph base, target names, offsets and default weights, but not skinning or animation |
| Dynamics | Bullet rigid and soft bodies, particles | None | No | |
| Scene management | Tags, takes, layers, Smart Folder, autosave | Tags, layers, hierarchy, object browser, autosave/recovery, selection sets and multi-object editing | Partial | No Smart Folder or document-level takes |
| Interchange — native | `.jas` scene format | `.lcscene` JSON document with validation and recovery | Yes | Clean-room format, not Cheetah3D-compatible |
| Interchange — import | FBX, OBJ/MTL, 3DS, DAE, USDZ, SCN, STL, DXF, SIA (public 8.1.3 docs list no glTF/GLB import) | OBJ/MTL, STL, glTF/GLB (mesh, materials, morph targets, cameras and all punctual light types with cones and range) and native `.lcscene` | Partial | LeopardCraft's glTF/GLB import is an extension beyond the documented Cheetah3D import list; it does not read skins or animation, and reports those and camera `aspectRatio` hints instead of dropping them silently. No FBX, 3DS, DAE, USDZ, SCN, DXF or SIA |
| Interchange — export | FBX, OBJ, 3DS, DAE, glTF/GLB, USDZ, STL, DXF | OBJ, STL, animated/skinned/morphing glTF/GLB and native `.lcscene` | Partial | glTF export now carries skins, morph targets and animated TRS/morph weights; no FBX, DAE, USDZ, DXF or 3DS |
| Scripting and automation | JavaScript/WebKit macros, custom tools, loaders and exporters | Headless `selftest`/CLI paths and a headless MCP stdio server | Partial | No embedded scripting, plugin API, custom import/export hooks or live-editor automation; MCP edits its own document |
| UI | Customisable Cheetah-style workspace, object browser, property editor, timeline, render manager | Fixed Cheetah-style egui workspace, object browser, properties, material shelf, timeline, Render Manager | Partial | Customisable panel layouts are not implemented |
| Platform | macOS 10.15.7+, Apple Silicon and Intel, Metal viewport | Linux, macOS and Windows via eframe/wgpu | Yes | Different platform baseline and graphics backend |

## Morph targets: what this slice added

Cheetah3D's Morph tag workflow is: add the tag, record **Base**, move only
vertices, press the add button to store the current shape as a named morph,
restore the base, then blend targets with the weight sliders (source 4). The
Morph page says the sliders "increase or decrease the weight" and blend
"between" the stored poses; it does not state a numeric weight range, whether
negative or greater-than-one weights are accepted, or how morph weights are
interpolated.

LeopardCraft now mirrors the useful core of that workflow:

- A mesh node carries a `MorphSet` with a recorded base vertex array and named
  `MorphTarget` offset arrays. Targets default to weight 0 and are additive.
- **Morph Targets** appears in the mesh inspector. It records the base, captures
  the current editable-mesh pose as a target, renames targets, blends each with
  a 0–1 slider, removes targets and restores the base.
- Each target contributes a dynamic **Morph Weight** row to the object's
  F-curve editor. The row reads and writes `AnimationClip.morph_weights`, keyed
  by target index, and uses the same key selection, drag, copy/paste,
  interpolation and delete paths as transform channels. Recording a row stores
  the target's current weight; the row shows the target name and a per-target
  interpolation control.
- Evaluation samples each morph-weight curve at the take time, falling back to
  the target's authored weight when no curve exists, restores the recorded base,
  applies the weighted offsets, then runs modifiers, skinning and the node
  transform. This ordering matters: the base is captured from the editable mesh
  before the modifier stack changes its topology.
- One `SetMorphCmd` undo step covers capture, removal, rename and weight drags;
  restoring the base is a shape edit with its own undo entry. Morph-weight F-curve
  edits are ordinary clip edits and use the same undo/redo path as other keys.
- Native `.lcscene` JSON serializes the morph-weight curves with the animation
  clip; load round-trips them and keeps their authored times, values and
  interpolation.
- Loading validates finite deltas and weights, clamps evaluated weights to 0–1,
  removes targets whose vertex count no longer matches the base or live mesh,
  repairs duplicate/empty names and drops morph data from non-mesh nodes.

### Morph-weight animation: documented facts and clean-room choices

The manual explicitly documents these general animation facts, not a
morph-weight-specific contract:

- The Timeline page says almost all object, tag and material properties can be
  animated, that keys record the selected property values, and that properties
  are interpolated between keys (source 18).
- The F-Curve Editor page documents constant, linear (the default), smooth and
  spline interpolation; spline exposes editable Bézier tangents. It calls these
  general animation controls and does not discuss Morph Tag weights (source 19).
- The Morph Tag page documents the sliders and target poses but says nothing
  about a numeric weight range, negative or over-one values, or how weight
  changes are interpolated (source 4). Consequently the manuals do not
  explicitly confirm that a Morph weight slider is a recordable parameter; the
  general Timeline statement is the closest documented support, and the forum
  workflow is community evidence rather than a manual guarantee (source 20).

The shipped behavior is therefore a clean-room approximation:

- Morph weights are exposed as F-curve rows because LeopardCraft treats tags as
  animatable targets. The manual does not explicitly name this parameter.
- The authored curve keeps unclamped values so the editor can show what the user
  wrote; evaluation and glTF export clamp to `0..=1`. **The manual does not
  document this range.** The 0–1 bound is a LeopardCraft choice and should not be
  read as a Cheetah3D-documented maximum; negative and over-one authored values
  are clamped only when applied.
- Morph-weight rows now use the same four F-curve modes as every other channel:
  Constant holds, Linear is straight, Smooth derives Catmull-Rom tangents, and
  Spline exposes the scalar incoming/outgoing tangent handles. Selection-wide
  interpolation and Tangent Type commands include morph keys, and native
  `.lcscene` round-trip preserves both the modes and tangent sides (sources 19,
  27 and 28). New keys are recorded as Smooth, LeopardCraft's existing authoring
  default; the manual calls Linear its default, so that default differs even
  though any key can be set to any of the four modes.

Deliberate limits of this slice:

- The dynamic rows are only shown for object animation targets; morph weights
  on other scopes do not exist. glTF import is still static, and export does not
  carry tags, constraints or material animations.
- Cheetah3D's exact weight limits, negative/over-one behavior and any
  morph-specific easing remain unverified rather than claimed. Morph rows use
  the general four-mode F-curve controls documented above, including the
  Tangent Type menus and Spline handles; the manual does not describe
  morph-specific interpolation.
- Editing the base geometry after recording requires a new base; the UI does
  not offer "update base" yet.
- A target recorded on a mesh whose modifier stack changes the vertex count
  still evaluates from the pre-modifier base, which is the intended editable
  mesh workflow.

## F-curve interpolation and tangent types: what this slice added

The public F-Curve Editor manual says Cheetah3D offers four interpolation
types and defines them as follows: **Constant** performs no interpolation and
holds the value of the left-hand key; **Linear** changes at constant speed and
is the manual's stated default; **Smooth** tries to create a smooth curve
through the keys; and **Spline** is an arbitrary Bezier curve with visible
left/right tangent handles that are dragged with the mouse (sources 19 and 27).
The same chapter says selected keys are highlighted in red and directs the user
to `Animation -> Tangent -> Type` to set both tangents, or to
`Animation -> Left Tangent -> Type` and `Animation -> Right Tangent -> Type` to
set one side independently; two different types can be combined arbitrarily
(source 27). The manual names `Type` as a menu placeholder and does not
enumerate the tangent submenu entries or give a tangent slope algorithm.
It also states that the Key Editor shows neither parameter values nor the
interpolation type between keys, making the F-curve editor the place to inspect
and change interpolation (sources 23 and 28). The current feature list still
advertises the same family of interpolation modes (source 29), while the 7.0
release notes specifically announce constant-key interpolation (source 3).

LeopardCraft now provides four per-key interpolation modes on scalar object,
tag/material and morph-weight curves:

- **Constant** holds the left key until the next key; **Linear** evaluates the
  straight segment; **Smooth** re-derives both tangents as Catmull-Rom slopes;
  and **Spline** uses the same cubic Hermite evaluator while exposing the
  scalar tangent handles. The four modes are selected per key, and a
  selection-wide control applies the choice to every selected key.
- Scalar and morph-weight curves store incoming and outgoing slopes plus an
  independent `TangentType` for each side. The mapping is explicit because the
  manual does not enumerate the submenu: Constant gives a flat (zero) slope,
  Linear gives the adjacent segment's secant, Smooth gives the Catmull-Rom
  slope, and Spline keeps a hand-authored slope or falls back to the adjacent
  secant. The distinct fallback means an untouched Spline curve is measurably
  different from Smooth rather than collapsing into the same mode.
  `Animation -> Tangent -> Type` sets both sides; the `Left Tangent` and
  `Right Tangent` menus set one side. The F-curve toolbar also exposes the
  same selection-wide interpolation and tangent controls.
- Selecting **Smooth** genuinely redefines both tangents as automatic and
  clears hand-authored slopes. Constant and Linear leave stored slopes alone
  because they do not read them, and Spline preserves hand-authored slopes so
  switching to Spline can reveal and edit them. A non-Spline tangent type does
  redefine that side, so the previous slope is replaced by the declared
  derivation. These rules are documented at the implementation site.
- Quaternion rotation keys accept the same four interpolation choices and the
  same Tangent Type command. They have no scalar handles, so Constant holds,
  Linear uses shortest-path slerp, and Smooth/Spline use the component-wise
  cubic quaternion derivative with hemisphere alignment. Each cubic side uses
  its tangent type: Constant is flat, Linear is the adjacent secant, Smooth is
  Catmull-Rom, and Spline falls back to the secant because there is no scalar
  quaternion handle to drag. The types are stored and round-trip.
- Interpolation and tangent-type edits preserve their data through native JSON
  save/load. Changing either for the selection is one undo step that restores
  the prior types and the prior tangent slopes on undo and reapplies them on
  redo. Retiming leaves both modes and tangent types unchanged; a retimed
  Constant interval still holds and a retimed Linear interval remains straight.

Cheetah3D's manual calls Linear its default; LeopardCraft continues to record
new keys as Smooth, its pre-existing authoring default (and the behavior
reported by Cheetah3D forum users in practice, source 30). Every key can now be
set explicitly to any of the four documented modes. The tangent-type mapping
and the absence of quaternion handles are clean-room LeopardCraft definitions,
not claims about Cheetah3D's internal tangent math, handle lengths, snapping or
multi-key behavior.

## Animation retiming: what this slice added

The Take Manager chapter documents only that the last column displays the take
length in seconds and that this length determines the timeline length (source
24). It does not say that editing that field scales existing keys. The Timeline
chapter describes dragging the preview/render interval ends (source 18), not
key timing. The Key Editor chapter says keys can be selected, moved around or
deleted (source 23), while the F-Curve chapter documents interpolation and
tangent controls (source 19); none of these chapters documents a native
"scale keys in time" command.

Community evidence points to a script instead. In a 2015 thread asking to
compress an 18-second animation to 11 seconds, the replies distinguish the
Takes length field and Frames per Second from key retiming, then point to
**Scale Take.js**. Its poster describes entering a numeric factor (0.6111 for
18 to 11 seconds) and says it applies to every keyframe in the entire take
(source 25). A later thread points to **Scale Take.js** or **F-Curve Utility.js**
for retiming, while describing the native Takes **Split** command as a trim
rather than a scale (source 26). The public manual therefore does not document
a native key-scale command; whole-take scaling is community-script behavior,
and the exact pivot, collision and tangent rules are not documented.

LeopardCraft now provides a native retiming extension:

- **F-curve: Scale Keys** retimes the selected keys by a positive numeric
  factor. Its pivot is the earliest selected key, so that key stays fixed and
  later selected keys move toward or away from it. **Takes: Retime Take**
  retimes every key in the selected per-object take around **0 seconds**.
- Scalar object, tag and material tracks and morph-weight curves all receive
  new key times. Quaternion rotation keys move in the same pass; their smooth
  tangents are implicit in sampling and therefore follow the new times rather
  than retaining a separate stale slope.
- Times are not frame-snapped: a factor that implies a sub-frame time produces
  that fractional time. A retime is rejected atomically if it would move a key
  to a negative time, produce a non-finite time, or place two keys on the same
  track within `KEY_TIME_EPSILON` (0.001 seconds). A rejected retime changes no
  keys and creates no undo entry.
- Hand-authored scalar tangents are rescaled independently on their incoming
  and outgoing sides by the incident segment's old/new duration. Because the
  Hermite sampler multiplies the slope by that duration, each affected segment
  keeps its normalised shape. Automatic tangents are re-derived from the new
  neighbouring times. This also updates a hand-authored tangent on an
  unselected key when an adjacent selected key changes the segment duration.
- Every key's interpolation mode and both tangent types move with the key
  unchanged. A retimed Constant interval still holds its left value and a
  retimed Linear interval still samples a straight segment; Smooth and Spline
  keep their automatic/manual tangent ownership rather than silently becoming
  one generic curve type.
- Selected-key retiming and whole-take retiming are each one undo step that
  restores both the old times and the old tangent slopes; the retimed clip
  round-trips through the native `.lcscene` JSON format.

The pivot rules, fractional-time behavior, collision rejection and tangent
rescaling are LeopardCraft definitions, not claims about Cheetah3D's script or
UI semantics.

## Skin binding and weight painting: what these slices added

Cheetah3D's public feature list advertises joint-based character animation,
heat weighting, vertex-weight painting, IK and constraints (source 1). The
Skeleton Tag manual documents the root-joint **Joint** table: **Bind Mesh**
calculates vertex weights for every joint in that table, with **max. influence**
(2–4 normally enough) and **dropoff rate** (higher makes a joint lose influence
faster) (source 9). The Vertex Weight Tool manual documents selecting a joint in
the table, red/yellow weights with grey zero-weight vertices, Add and Subtract
brushes, size/intensity, constant or bell brush shape, Only visible, and
auto-normalize after every stroke (source 10). The complete 8.x bind and brush
panels are not publicly documented, so the chosen-subset controls, Strength
slider, preserve-on-rebind rule and Replace/Smooth brush modes are clean-room
equivalents rather than claims about exact Cheetah3D layout.

LeopardCraft now provides:

- a Skinning panel with All/None and per-joint checkboxes; binding stores
  inverse-bind matrices only for the chosen joints, and manual binding leaves
  uncovered vertices unweighted so they evaluate at rest instead of silently
  falling back to the root joint;
- a deterministic proximity auto-weight pass for those joints: normalized
  inverse-distance weights to joint bone segments, capped by Max influences,
  with Dropoff and Strength controls; this is a proximity solver, not
  Cheetah3D's advertised heat-diffusion weighting;
- rebinding that carries every existing positive weight whose joint remains
  selected, auto-weights only vertices with no carried influence, and does not
  overwrite the whole painted skin;
- a viewport **Weight Paint** mode for a bound editable mesh with no modifier
  stack, with Add, Subtract, Replace and Smooth brushes, radius, strength and
  constant/linear/smooth falloff, plus a joint list and a per-vertex heat map
  that shows zero-weight vertices in grey;
- automatic normalization and compaction after each brush stamp, keeping at
  most four influences;
- one undo entry per drag stroke, and native-load repair of weight lengths,
  invalid joint indices, duplicate slots, non-finite/negative weights and
  non-normalized vertices.

Deliberate limits of these slices:

- The brush is restricted to modifier-free editable meshes because skin arrays
  are indexed against the evaluated mesh.
- Cheetah3D's Skeleton tag computes weights for all joints in its table. Binding
  a chosen subset and preserving existing weights on rebind are clean-room
  extensions; the bind controls match the documented max-influence/dropoff
  concepts where the sources are solid. Viewport joint picking, Only-visible
  brush filtering, heat-diffusion weighting, Spline IK and constraints are not
  implemented.
- Smooth uses mesh-edge neighbours and smooths only the active joint's weight;
  it is not a general all-channel weight relaxer.

## Inverse kinematics: what this slice added

Cheetah3D's 4.1 release notes document a 2D IK solver and an IK bake command in
the Take Manager (source 11); the public feature list also advertises Spline IK
and IK Angle/Point Constraint tags (source 1), and the manual index lists IK
Handle and IK Limits tags (source 5). Community explanations distinguish the
handle's Target from its Pole and report a default handle Length of 2, with the
length counting the joints that bend (sources 12 and 14); another community
note says baking IK keys every frame of the animated object (source 13). The
8.1.3 solver's damping, iteration/tolerance controls, exact chain-counting
semantics, rotation-limit interpretation and bake sampling are not publicly
documented. The implementation below is therefore a clean-room equivalent, not
a claim that it reproduces Cheetah3D's internal solver or file format.

The manual page indexed as **IK Limits** is titled **IK Constraint Tag**
(source 15). It says the tag's constrain property chooses which rotation
coordinates of the tag owner to constrain, with minimum and maximum values for
heading, pitch and bank. It states that these limit properties work only with
the 3D solver, that angle limits constrain the solver's possible solutions, and
that a stiffness of one means the joint does not bend (source 15; the IK Handle
page also describes limiting a joint's rotation, source 16). A forum report
places the tag on a joint rather than the mesh (source 17). The manual does not
document the limit reference frame, inclusive endpoints, 2D-solver enforcement,
reversed bounds or zero-width limits. LeopardCraft therefore implements the
following as a clean-room approximation rather than claiming those Cheetah3D
semantics.

LeopardCraft now provides:

- an `IkChain` on the selected tip Joint: target node, optional pole node,
  movable-ancestor count (default 2), iteration and tolerance limits, CCD
  damping, and an enabled flag. A missing or deleted target simply disables that
  chain; old `.lcscene` documents load with no IK data;
- a deterministic evaluation step that solves enabled chains after animation
  sampling and before the existing tag-constraint pass. The tip joint's world
  origin is the end effector. Exactly two movable joints use a closed-form
  law-of-cosines solve; the optional pole chooses the bend side. Longer chains
  use damped cyclic coordinate descent, with the pole used as a post-convergence
  plane preference. A one-joint chain uses the same CCD path;
- no bone stretch: targets beyond maximum reach or inside minimum reach are
  clamped to the nearest reachable point. Zero-length bones, missing ancestors,
  coincident pivots and missing pole nodes skip safely instead of producing
  NaNs; iterations are capped at 128 and chain length at 64;
- an **Inverse Kinematics** section in the Joint inspector for creating and
  removing a chain, selecting target and pole, editing all solve controls, and
  baking the global playback range. **Bake IK take** evaluates every frame in
  the range (to a 10,000-frame safety cap) and writes quaternion rotation keys
  for each movable joint through one undo step; with IK disabled, playback at
  those frames reproduces the solved rotations, and the keys are ordinary
  animation data for glTF export;
- per-joint `JointLimits`: an optional enabled flag plus minimum and maximum
  local XYZ Euler angles in degrees. Limits are serialized with the joint,
  survive native load/undo, clamp direct or animated joint rotation during
  evaluation, and are enforced after each correction in both the analytic
  two-bone and damped CCD paths. A zero-width axis locks that axis; reversed
  finite bounds are swapped; non-finite values fall back to the full range. The
  inspector can add, remove and edit the three ranges;
- skinning consumes the solved world matrices, so a bound mesh deforms through
  IK-driven joints. Skin arrays and their weights are not rewritten by a solve.

Deliberate limits: the per-joint limits are not Cheetah3D's IK Constraint tag:
Cheetah3D documents heading/pitch/bank coordinates on a tag and marks the limit
properties as 3D-solver-only, while LeopardCraft uses local XYZ Euler degrees on
the joint and applies them to direct posing and both solver paths. There is no
stiffness control, no IK Limits/Angle/Point, Orient or Parent tag, no Spline IK
and no viewport joint picking. The target and pole are node origins, not
arbitrary control surfaces or curves. Tag constraints still run after IK and
can override a solved joint; the broader constraint system remains separate.
Bake uses the global playback range rather than document-level takes, and IK
settings are not a glTF interchange feature (baked keys are). The two-bone pole
is an exact bend-plane selection; the CCD pole is a preferred plane after
convergence, not a hard twist constraint. glTF import and Cheetah3D-style
morph-weight interpolation/range semantics remain open. Cheetah3D publicly
documents glTF/GLB export (source 21) but not glTF/GLB import; LeopardCraft's
static glTF importer is therefore an extension, not a parity claim (source 22).

## glTF skinning, morph and animation export, and morph import

The exporter now writes a bind-pose mesh rather than the evaluated deformation.
It emits, in the same glTF/GLB:

- a `skin` per bound mesh, with `joints` as glTF node indices, an
  `inverseBindMatrices` MAT4 accessor, and `JOINTS_0`/`WEIGHTS_0` (four
  influences, normalized weights, unsigned-short joint indices);
- named morph targets on every primitive, with POSITION deltas copied from the
  recorded `MorphSet` and NORMAL deltas derived by comparing base and target
  corner normals, plus `mesh.extras.targetNames` and node default `weights`;
- one `Take` animation containing node translation, rotation and scale channels
  and morph-weight channels, sampled on one seconds-based time grid that
  includes every authored key plus scene-fps frames. Uniform STEP and LINEAR
  channels map directly; all-Smooth/Spline scalar channels use sampled
  CUBICSPLINE tangents, while mixed channels and smooth rotations are sampled
  LINEAR.

Joint nodes are exported in their bind-pose TRS, recovered from the stored
inverse-bind matrices, so the mesh rest pose and skin are consistent before an
animation is applied. A mesh whose skin is incomplete, or whose morph base no
longer matches the exported topology or rest positions, is exported without
that feature instead of writing corrupt attributes. Morph targets also require
an empty modifier stack because evaluation applies them before the stack;
topology-changing modifiers leave the modified mesh unskinned rather than
baking mismatched indices.

Import now reads a mesh's morph data instead of silently dropping it. Each
primitive's `targets` POSITION accessors become target offsets in a `MorphSet`
whose base is the imported rest mesh (glTF targets are deltas by specification,
so an imported target poses the mesh exactly as in the source file), names come
from `mesh.extras.targetNames` or a target's own `extras.name`, and default
weights come from `mesh.weights`, with a node's `weights` array overriding the
mesh values for that instance. Data the importer cannot represent is refused
with an actionable message naming the mesh or node rather than skipped: merged
primitives that declare different target counts, a target-name list of the
wrong length, a POSITION target accessor whose element count does not match the
primitive, and a weights array whose length does not match the target count. A
target with no POSITION accessor (normals only) imports as zero offsets.
Imported weights are clamped to the 0–1 range evaluation uses; that range is a
LeopardCraft clean-room choice, since neither the glTF specification nor the
Cheetah3D manual states a morph-weight range.

Cameras and lights import too. A node's `camera` reference becomes a real camera
object: perspective `yfov` (radians per the glTF specification) becomes
`fov_y_deg`, `znear`/`zfar` become the clip planes, and an orthographic camera's
`ymag` half-height becomes `ortho_scale`. Every `KHR_lights_punctual` type maps
to a LeopardCraft light with its authored colour and intensity: `directional`
and `point` directly, and `spot` to the spot light object, whose
`innerConeAngle`/`outerConeAngle` radians are converted into the degrees the
document stores. A light's `range` becomes that light's radius, LeopardCraft's
falloff/shadow distance, instead of the fixed 1 m default. These are clean-room
mappings, because Cheetah3D's public 8.1.3 documentation lists no glTF importer
at all: glTF intensity is photometric (lux/candela) while LeopardCraft's is an
arbitrary scale, so the value is copied verbatim rather than converted; glTF
`range` is a distance cutoff while our radius is a shadow/falloff radius.
Camera and light data the importer cannot use fails the import with a message
naming the node and the fix: a camera with no perspective or orthographic block,
a perspective camera with no `yfov`, a `yfov` outside 0–180°, a non-positive
orthographic `ymag`, a dangling `camera`/light index, a spot cone that breaks
`0 <= inner <= outer <= 90°`, and a light whose `type` is missing or outside the
extension's set.

One authored field is still not representable: a camera `aspectRatio` hint,
because LeopardCraft takes aspect from the render output. It is counted in
`ImportReport`, whose actionable summary also counts skinned meshes and
animation clips, which are still not imported. The editor shows the summary
after Open and Import, MCP `open_scene` returns it as `import_report`, and
headless startup prints it to stderr; an import with nothing to report stays
silent and behaves exactly as before.

## Triangulate: what this slice added

Cheetah3D lists a Triangulate polygon tool in its feature set (source 1); the
manual page for it is not among the pages indexed here, so the split rule below
is a LeopardCraft clean-room choice rather than a documented Cheetah3D rule.

`lc_geom::triangulate_faces` fans each selected polygon into `corners - 2`
triangles from its first corner. Only selected polygons change: everything else
keeps its n-gons, so the operation edits exactly what the user selected. Each
triangle reuses the original corner records, which keeps point indices, UV slots
and material slots intact, and because creases, seams and pinned UV slots are
point/corner indices they survive untouched; the new fan diagonals carry no
crease or seam. Fans whose corners repeat a point are skipped so the result
still validates, and out-of-range face indices are ignored. Mesh Editing's
**Triangulate** button and `Tools -> Triangulate` share one predicate — polygon
mode on editable meshes with at least one selected polygon of more than three
corners — so the command is refused with a reason rather than silently doing
nothing, and one undo step covers the action.

## Flip Normal: what this slice added

Cheetah3D lists Flip Normal in its polygon-tool set (source 1); its manual page
is not among the pages indexed here, so reversing the current polygon selection
is a LeopardCraft clean-room choice rather than a documented Cheetah3D rule.

`lc_geom::flip_faces` reverses each selected polygon's corner order, which is
what every consumer — `Mesh::face_normal`, `corner_normals`, render triangles
and glTF export — reads as the face normal; `Mesh::flip_normals` applies the
same reversal to a whole mesh after mirroring. Unselected polygons keep their
winding, and points, UV slots, material slots, creases, seams and pinned UV
slots are untouched because none of them depend on corner order. Repeated face
indices flip once and out-of-range indices are ignored. Mesh Editing's **Flip
Normal** button and `Tools -> Flip Normal` share one predicate — polygon mode on
editable meshes with at least one selected polygon — with refusals that name the
reason, the selection is preserved so the tool toggles back, and one undo step
restores the exact prior winding.

## Collapse: what this slice added

Cheetah3D lists Collapse in its polygon-tool set (source 1); its manual page is
not among the pages indexed here, so acting on the current point selection is a
LeopardCraft clean-room choice. The editor's existing Weld command already
performed the same merge, so both now call `Mesh::merge_points` through
`lc_geom::collapse_points` instead of keeping two copies.

The lowest selected point index survives at the centroid of the selection, and
every face that used a merged point uses it instead. Adjacent duplicate corners
fold away first, so a quad that loses a corner becomes a triangle; a face that
still repeats the point, or drops below three corners, is deleted rather than
left pinched. Creases and seams follow the surviving indices (a mark whose both
endpoints merged disappears), pins on UV slots no face uses any more are
dropped, and unused points are compacted away with a remap, so no attribute
keeps a stale index. Mesh Editing's **Collapse** button and `Tools -> Collapse`
share one predicate — point mode on editable meshes with two distinct selected
points — refuse other modes with a reason, remap the component selection to the
merged vertex and cover the action in one undo step.

## Disconnect: what this slice added

Cheetah3D lists Disconnect in its polygon-tool set (source 1); its manual page is
not among the pages indexed here, so working on the current polygon selection is
a LeopardCraft clean-room choice.

`lc_geom::disconnect_faces` duplicates every point that a selected face shares
with an unselected face, rewrites only the selected faces to the copies and
leaves the unselected faces on the originals. The region becomes its own
topological island with identical positions and winding order, interior points of
the selection stay shared among its faces, and UV slots and material slots are
untouched, so a corner keeps its UV value and its pin. Creases and seams follow
the faces on each side: the original mark stays where an unselected face still
uses the edge and the cut gains the same mark on the duplicated indices. Every
point stays referenced, so nothing is compacted away and face indices never
change, which keeps the component selection valid. `lc_geom::can_disconnect`
backs one predicate — polygon mode on editable meshes with a selected face that
borders unselected geometry — so a whole-mesh or empty selection is refused with
a reason, and one undo step restores the exact prior points and faces.

## Cover: what this slice added

Cheetah3D lists Cover in its polygon-tool set (source 1); its manual page is not
among the pages indexed here, so reading the selection as the ring of faces
around a hole is a LeopardCraft clean-room choice. Fill Hole covers the other
direction: one selected border point in any component mode fills every hole it
touches (source 38).

`lc_geom::coverable_holes` keeps a boundary loop only when every one of its
edges is an open boundary edge owned by a selected face, so a loop the selection
partly borders, a closed mesh and an open chain that cannot be closed are
skipped. `lc_geom::cover_faces` fills those loops through the existing
`fill_holes` constructor: the new polygon reuses the border points in their
existing order, is reversed to wind with its neighbours, inherits the lowest
bordering material slot and gets a planar UV projection into its own 0..1 box.
No existing point or polygon changes, the hole becomes watertight, and the new
face indices go back to the editor so the component selection names them. The
button and `Tools -> Cover` share one predicate — polygon mode on editable meshes
with a completely ringed hole — and one undo step restores the exact prior
points and faces.

## Create Polygon: what this slice added

Cheetah3D lists Create Polygon in its polygon-tool set (source 1); its manual
page is not among the pages indexed here, so reading the selection as the points
around an open region is a LeopardCraft clean-room choice.

`lc_geom::faceable_loops` keeps a boundary loop only when every one of its points
is selected, no polygon already uses exactly that point set and its Newell area
is non-zero. The loops come from the mesh's own boundary walk, so the face order
never depends on the order the points were clicked, and a partial run, a
collinear run or a duplicate face is skipped while valid runs still become faces
in the same action. `lc_geom::create_polygon_faces` appends one polygon per loop
over the existing point indices, reversed to wind with its neighbours, with each
corner inheriting its point's UV and pin and the polygon inheriting the lowest
bordering material slot. Nothing is created, moved or renumbered apart from the
new face, which becomes the component selection; one undo step restores the
exact prior points and faces.

## Point Slide: what this slice added

Cheetah3D lists Point Slide in its polygon-tool set (source 1); its manual page is
not among the pages indexed here, so this is a LeopardCraft clean-room reading of
the tool.

`Mesh::slide_points` takes the adjacency straight from the mesh's polygon edges
(deduplicated per point), so no new selection concept is involved and neither the
order the points were selected in nor a duplicate selection can change the result.
Each selected point moves the Fraction (default 0.5, restricted to 0..=1) of the
distance from its authored position to the centroid of its neighbours, with every
centroid measured from the positions *before* the move. A point with exactly two
neighbours therefore lands on the segment midpoint at a fraction of 1, which is
what makes the reading defensible; points with more neighbours get the
generalized version. Only the selected positions are rewritten: no corner record,
winding, material slot, UV slot, crease, seam or pin changes and no index moves,
so the component selection stays valid. The **Point Slide** button, its Fraction
field and `Tools -> Point Slide` share `mesh_point_slide_ready` — component mode,
point mode, a fraction above zero and at most one, and a selected point with a
neighbour — with a distinct message per refusal, and one undo step restores the
exact prior positions.

## Optimize: what this slice added

Cheetah3D lists Optimize in its polygon-tool set (source 1); its manual page is
not among the pages indexed here, so welding only the selected point clusters
within a tolerance is a LeopardCraft clean-room reading of the tool.

`Mesh::weld_points` groups the selected points that lie within the tolerance and
keeps the lowest-indexed one of each cluster at its authored position. Points are
visited in index order and each joins the first earlier survivor inside the
range, so the result is deterministic and independent of selection order. Faces
keep the vertices they do not merge, a face that loses an edge becomes a smaller
polygon, and a face that repeats a merged point is deleted. Creases and seams
follow the surviving indices, pins on UV slots no face uses are dropped, and
unused points are compacted away by the same `finish_point_merge` tail that
Collapse uses. Mesh Editing's **Optimize** button, its Tolerance field and `Tools
-> Optimize` share one predicate — point mode, a positive finite tolerance and at
least one mergeable cluster — with a distinct refusal message per case, and one
undo step restores the exact prior points and faces.

## Deformer and subdivision parity

Cheetah3D's deformer objects are non-destructive modifiers dropped inside a
polygon object. They share a **Cage** size and a **Type** (`Constrained`,
`Unconstrained` or `Cage`) and process in stack order. The manual specifies
Bend around the modifier's Z axis with an **Angle**, Twist around Y with an
**Angle**, Shear with a **Distance** and **Angle**, Bulge on the X and/or Z
axes with a **Strength**, and Spherify with a **Radius** and **Strength**
(sources 32–35). Its Subdivision modifier exposes **Iterations** (default 2)
and a **Scheme** of Catmull-Clark, Stam-Loop or Linear (source 31).

LeopardCraft now adds clean-room stack equivalents for four of those
deformers and all three documented subdivision schemes, including creased
edges:

- **Twist** rotates the two coordinates perpendicular to a chosen X, Y or Z
  axis linearly along that axis, by the total angle. The object's bounds define
  the axis extent, so the deformation is predictable for any mesh.
- **Shear** offsets points perpendicular to a chosen axis in proportion to
  their position along it, with a direction angle in that perpendicular plane.
  The midpoint is unchanged, which keeps the object centred.
- **Bulge** expands or pinches the X and/or Z cross-section around the Y
  mid-plane. Positive strength expands the middle, negative strength pinches
  it, and the scale is clamped positive so the mesh cannot invert.
- **Spherify** moves vertices toward a sphere of the given radius around the
  bounds centre by a 0–1 strength, leaving the topology unchanged.
- **Subdivision** now offers **Catmull-Clark**, **Stam-Loop** and **Linear**
  schemes beside its level count, which now defaults to 2 like Cheetah3D.
  Catmull-Clark produces quads from any n-gon. Stam-Loop triangulates the
  cage and applies Loop edge and vertex rules, producing four triangles per
  triangle, so mixed quad/triangle cages take a different path from both other
  schemes. Linear splits every n-gon into n quads around its centroid and
  shares edge midpoints between neighbours, keeping the surface watertight
  without smoothing it. Old documents without a scheme load as Catmull-Clark,
  and new modifiers default to it.
- **Edge creases** are a per-mesh list of canonical point pairs, so they are
  independent of temporary edge ordering and survive point compaction,
  welding and mesh merging. In Edge mode the Mesh Editing panel offers
  **Toggle Crease** for the selected edges, and the viewport draws creased
  edges in blue. Catmull-Clark treats a creased edge as a sharp edge: its edge
  point is the midpoint, a vertex with two incident creases follows the
  3/4 + 1/8 + 1/8 crease rule, a vertex with three or more stays pinned, and
  both child edges inherit the crease. Stam-Loop applies the equivalent Loop
  crease rules. Linear subdivision deliberately ignores crease sharpness, so
  its geometry is unchanged; the crease marks are still split onto child edges
  for later editing. Loading normalizes duplicate, degenerate and out-of-range
  crease entries.

The modifier inspector edits each parameter in the stack, and the geometry
layer has unit tests for shape change, finite extremes, watertight linear
topology, three-way scheme distinctness on a mixed quad/triangle cage,
crease persistence and sharpening under Catmull-Clark and Stam-Loop at
multiple iteration counts, linear crease independence, material-style
serialization and legacy documents with neither crease data nor a scheme
field.

Deliberate gaps: Cheetah3D's shared Cage/Type controls remain unimplemented,
and LeopardCraft's deformers act on the whole mesh with bounds-relative
falloff instead. Stam-Loop here is a clean-room Loop-on-triangulated-cage
approximation of the documented mixed quad/triangle scheme, not a verified
reimplementation of Stam's published rules. Crease weights, partial
subdivision and crease display outside Edge mode are still open, as are
Cheetah3D's displacement, formula, magnet, polygon-reduction, Laplacian smooth
and wrap modifiers.

## UV seams: what this slice added

The UV unwrapping tutorial documents the daily workflow (source 37). It says a
**seam** is an edge where the mesh tears when `Tools -> UV -> Unwrap UV` runs,
that seams are set like creases by selecting edges and calling
`Selection -> Toggle Seam`, and that seam edges then appear **green**. It also
documents pinned UV coordinates: points are pinned from the UV editor's context
menu (`Toggle Pinned UV`), pinned points turn yellow, and `Unwrap UV` leaves
their position unchanged. The feature list separately advertises ABF and LSCM
unwrapping, Autoseam and several mapping types (source 1). The tutorial does not
say how a seam interacts with the unwrap algorithm, whether one seam edge may
belong to several charts, how charts are packed, or what Autoseam uses to pick
its cuts.

LeopardCraft now authors seams and uses them as unwrap cut lines:

- A mesh carries `seams`: canonical undirected point pairs, stored beside the
  existing creases and independent of them. Marking an edge as a seam does not
  crease it, and vice versa. The list survives point compaction, welding, mesh
  merging and loading, and is included in the semantic mesh signature.
- Edges are marked in Edge mode with **Toggle Seam**, available in the Mesh
  Editing panel and as `Selection -> Toggle Seam`. One selection-wide toggle is
  one undo step, matching the crease command. Toggling the same selection twice
  removes the seams again.
- Seam edges are drawn green in the 3D viewport, above crease blue in priority,
  and as green border lines over the UV wireframe. The UV view reports the seam
  count for the primary mesh.
- **Automatic Unwrap** treats every seam as a hard chart border: the chart
  flood-fill never crosses a seam, so marking a cut flattens a closed or
  smoothly curved surface into separate islands even where the adjacent face
  normals agree. Without seams the previous normal-angle charting still applies.
  Charts are projected onto their average normal and shelf-packed into 0..1 as
  before.
- **Relax** is chart-aware: its graph is split at every seam, so the two sides
  of an authored cut relax inside their own charts instead of being averaged onto
  one shared point value, and seam or mesh-boundary corners are held when the
  pin-boundary option is on (the editor's Relax command passes `true`). The
  manual defines seams only as unwrap cuts (source 37: "A seam is an edge where
  the mesh will tear up when calling the Unwrap UV command") and documents no
  Relax tool, so applying the same cut rule to relaxation is a LeopardCraft
  clean-room choice rather than a documented Cheetah3D behaviour.
- Subdivision inherits seams onto both child edges of a split seam edge, the way
  it inherits creases, so a seam survives a subdivision modifier stack.
- Native `.lcscene` documents serialize seams only when present; loading
  removes duplicate, degenerate and out-of-range entries, and older documents
  without the field load with no seams.

Clean-room limits of this slice: seam authoring exists only in the 3D Edge
mode, not in the UV view, and there are no Select All Seams, Clear Seams or
"select the seam loop" commands. Unwrapping is a normal-angle chart splitter
with seam barriers and a shelf packer, not ABF, LSCM or Autoseam, so it does not
minimise distortion the way the documented Cheetah3D solvers claim to. The
relaxation chart splitter divides on seams only, not on the unwrap's
normal-angle charts, and relaxation does not re-align or re-pack charts after
smoothing.

## Pinned UVs: what this slice added

The unwrap tutorial documents the pinning workflow (source 37): after cutting
seams the user "pin[s] some UV coords" by selecting points in the UV editor and
calling **Toggle Pinned UV** from the UV editor context menu; the tutorial says
"the pinned UVs will become yellow (after unselecting the points)" and that
"the position of pinned UVs won't be changed when calling the Unwrap UV
command". It also shows pinned points being moved with the transform tool before
re-running Unwrap UV, so a pin fixes a *position* the user chooses rather than
rejecting later edits. The tutorial does not say how many points a pin command
accepts, whether pins survive other tools, or how the unwrap solver uses them
beyond keeping their position.

LeopardCraft now stores and honours pins:

- A mesh carries `pinned_uvs`: sorted indices into its `uvs` slot list, stored
  beside the seam and crease lists. Pins are part of the mesh signature, are
  serialized only when present, survive point compaction, welding and mesh
  merging (UV indices are rebased), and old `.lcscene` documents load with no
  pins. Loading drops duplicate and out-of-range pin entries.
- **Toggle Pinned UV** pins or unpins the UV point selected in the UV view. It
  is available in the UV panel toolbar and as `Selection -> Toggle Pinned UV`,
  is one undo step per toggle, and stays disabled until an editable mesh has a
  UV set *and* a UV point is selected, so it cannot pin a stale index.
- Pinned UV points draw yellow in the UV view, larger than unpinned points and
  ringed with the selection colour when they are also selected. The UV footer
  reports the pinned count next to the point, polygon and seam counts.
- **Automatic Unwrap** keeps every pinned slot at its exact authored coordinate.
  Each chart that owns a pin is first fitted to its pins with a best-fit uniform
  scale plus translation, so the unpinned points of that chart move towards the
  pinned frame, and the pinned slots are then written back exactly. Charts
  without pins are unchanged, and the pinned slots are renumbered with the rest
  of the slot list so the pins still refer to the same UV points afterwards.
- **Relax** holds pinned points fixed during the Laplacian passes and writes
  their slots back exactly; when any pin exists the layout is no longer re-fitted
  into 0..1, because that re-fit would move the pins. Pins act per corner inside
  the chart that owns them, and the passes never cross a seam. **Normalize**
  likewise leaves pinned slots where the user put them.
- Projections (planar, box, cylindrical, spherical) re-author every UV
  coordinate, so they clear pins instead of leaving them attached to unrelated
  slots. Subdivision rebuilds its UV set per point and clears pins for the same
  reason.

Clean-room limits, recorded rather than glossed over:

- A pin is a UV **slot** index, not a UV-point selection set. Cheetah3D's UV
  editor selects several points and toggles them together; LeopardCraft's UV
  view selects one point, so pins are toggled one at a time and there is no
  "pin all selected" command.
- The unwrap remains a normal-angle chart splitter with seam barriers and a
  shelf packer. The alignment step is a per-chart best-fit translate + uniform
  scale, not ABF or LSCM: it has no rotational or area freedom, it does not
  iterate between the 2D layout and the 3D surface, and charts can still overlap
  each other. Pinned charts are intentionally not re-packed, so a pinned point
  may sit outside 0..1 after an unwrap.
- Honouring pins in Relax and Normalize is a LeopardCraft extension. The
  Cheetah3D sources consulted describe pins only for Unwrap UV.
- Pins do not survive a subdivision modifier or a fresh projection; both replace
  the UV set, and the sources do not describe pin behaviour there either.

## Fill Hole: what this slice added

The Fill Hole manual page (source 38) documents the tool's modes and workflow:
it "belongs to the family of tools which can be used to create new polygons",
and "you just have to click on one point on hole border. Cheetah3D will do the
rest!". It states the tool "is available in point, edge and polygon mode and can
only be applied on raw polygon objects", and that after clicking one point "on
the hole's border ... Cheetah3D will now automatically detect the rest of the
hole border and will fill up the hole with a polygon". The page lists **no keys
and no properties**. It does not document the new polygon's winding, UVs,
material slot, what happens with several holes at once, or holes whose border
encloses an island.

LeopardCraft now provides it as a selection-driven command:

- `lc_geom::open_holes(mesh, points)` returns the boundary loops that contain at
  least one of the given points, so one selected border point identifies the
  whole hole exactly as the manual describes. `lc_geom::fill_holes` closes each
  of those loops with a single n-gon.
- The loop is **reversed** before it becomes a face, so every shared border edge
  is traversed opposite to its existing neighbour. That keeps the new polygon's
  winding consistent with the surrounding surface; the unit test compares the
  filled box's signed volume against the intact solid.
- The action is available in point, edge and polygon mode, matching source 38.
  A selection only qualifies when every selected editable mesh has at least one
  selected point on an open border, which is the same predicate the
  **Tools -> Fill Hole** menu entry and the Mesh Editing button use, so the
  enablement and the handler cannot disagree. Primitives must be converted to
  editable meshes first, which is the nearest equivalent of the manual's "raw
  polygon objects" restriction.
- One fill is one undo step (the existing `SetShapesCmd` path), and the component
  selection is cleared afterwards because filling changes edge and polygon
  indices. The Mesh Editing panel reports how many open holes the selection
  touches.
- Filling preserves the mesh's creases, seams and pinned UVs, and a filled mesh
  round-trips through the native `.lcscene` format unchanged.

Clean-room approximations and what closing them would need:

- **Invocation.** Cheetah3D runs Fill Hole as a click tool; LeopardCraft applies
  it to the current component selection from a menu or panel button. Closing the
  gap needs a viewport pick mode that maps a click to a boundary point and then
  calls the same geometry operation.
- **UVs.** The manual documents no properties, so the new polygon's UVs are a
  LeopardCraft choice: the border is projected onto its best-fit plane and scaled
  into the loop's own 0..1 box. Cheetah3D's own fill UVs are undocumented; true
  parity would need an observable reference, and until then the user can re-map
  the new face from the UV editor.
- **Material slot.** The new face inherits the lowest material slot of the faces
  on the border. The manual does not state a rule.
- **Topology.** Each loop becomes one n-gon. A hole that encloses an island, and
  a border that is not a single closed loop, are not filled; the operation
  rejects borders whose edges have two faces and never invents points.

## Split: what this slice added

The Split page (source 39) describes a copy tool: it "creates a completely new
polygon object with the content of the current polygon selection", which makes it
"very useful if you want to split up one big mesh into several smaller ones". It
"can only be applied to raw polygon objects" and "is only available in polygon
mode". The workflow is to "select some polygons of a raw polygon object. Then
call the menu command 'Tools>Polygon>Split'", after which "a new polygon object
will be created in the object browser which contains the former polygon
selection. All tags and selections will also be copied to the new object." The
page lists no keys and no properties.

The manual never says what happens to the source polygons. A Cheetah3D forum
thread (source 40) settles it: the reporter notes that after splitting the top
half of a cube "the original piece was still it's regular size", and the reply
is "you need to go back and delete the bit you split off manually". Split is a
copy, and the original keeps its polygons.

LeopardCraft now matches that:

- `lc_geom::extract_faces(mesh, faces)` copies the selected polygons into a
  standalone mesh, remapping only the points and UV slots those faces use, so a
  part never carries unused geometry. Creases and seams follow when both
  endpoints are copied, UV pins follow when their slot is copied, per-face
  material slots are preserved, and meshes without UVs stay without UVs.
- `Mesh Editing -> Split` and `Tools -> Split` copy the selection out of every
  selected editable mesh into a new object named `<source> Split`, inserted
  beside its source. The source mesh is untouched, so the documented workflow of
  deleting the originals afterwards still works.
- The copy keeps the source's transform, visibility/render/lock flags, layer,
  material assignment and slots, tags and per-tag animation clips, following
  "all tags and selections will also be copied to the new object".
- Enablement follows the documented restrictions: polygon mode, editable ("raw
  polygon") meshes, and at least one selected polygon on each selected mesh. The
  panel reports how many polygons the selection would copy and the status line
  names the new object.
- One action is one undo step: `AddNodesCmd` inserts every copy in a single
  command, so one undo removes them together.
- A split part round-trips through the native format, and a document that already
  carries creases, seams, UV pins and a filled hole still writes and reads
  unchanged after a split.

Clean-room approximations and what closing them would need:

- **What "tags and selections" covers.** The manual names tags and selections
  only. We copy tags, per-tag clips, the transform, node flags and material
  slots; modifiers, skin, morph targets and object animation stay behind because
  the new object holds raw geometry and the manual is silent. Closing this needs
  an updated manual or observation of the real tool.
- **Placement and naming.** Cheetah3D only says the new object appears "in the
  object browser"; we append it to the source's parent and name it
  `<source> Split` (uniquified by the scene). The manual documents neither.
- **Multi-object splitting.** Cheetah3D's tool works on one raw polygon object;
  we split every selected mesh, consistent with the other component tools. That
  is an extension rather than parity.
- **Marks that cross the selection border.** A crease or seam whose other
  endpoint lies outside the copied set is dropped. Cheetah3D does not document
  edge marks on Split parts at all, so there is nothing to match.

## Shell: what this slice added

The Shell Tool page (source 41) says "The shell tool is the tool of choice if you
want to make walls for a house or if you want to create a solid object out of a
plain surface." Its modes are documented as "The shell tool can only be applied
to raw polygon objects. It's only available in polygon mode." The workflow is
"select some polygons of a raw polygon object. Then choose the shell tool and
drag the mouse in the 3D view either to the left or right, depending on whether
you want to perform a negative or a positive shell. Once you are finished release
the mouse button." The page lists no keys and three properties:

- **Inside** — "decides if the shell is moved along the normal direction
  (outside) or against the normal direction (inside)".
- **Angle** — "Determines which polygon neighbors of a polygon selection belong
  to a shell. Bigger values allow more polygons too be grouped too a shell."
- **Quantize** — "Set this property to 0 if you want to adjust continuously,
  otherwise this value determines the distance you want to quantize the shell
  to."

LeopardCraft now provides it:

- `lc_geom::shell_region(mesh, faces, angle)` grows the selected polygons across
  edges whose neighbouring faces deviate by at most `angle` degrees, which is the
  documented Angle grouping. `lc_geom::shell_faces` then offsets the region's
  points along their averaged normals (`inside` picks the against-normal
  direction, as the Inside property describes), appends a reversed copy of every
  region polygon that shares its UV and material slots, and joins every region
  border with a rim quad so an open patch becomes a closed wall. `quantize` snaps
  the distance to the documented step and rejects a distance that snaps to zero.
- `Mesh Editing -> Shell` and `Tools -> Shell` share `mesh_shell_ready()`: polygon
  mode, editable ("raw polygon") meshes, at least one selected polygon on each and
  a non-zero thickness, so the button and the menu command cannot disagree. The
  panel carries the thickness field, the **Inside** switch, the Angle and
  Quantize fields, and reports how many polygons the region covers; the status
  line confirms the shell.
- One action is one undo step through the existing `SetShapesCmd`, and the
  component selection is cleared because indices change.
- Shelling every polygon of a box hollows it: 12 polygons, no border, and the
  wall volume matches the inner offset. A shelled mesh round-trips through the
  native format beside a split part, with creases, seams, UV pins and a filled
  hole intact.

Clean-room approximations and what closing them would need:

- **The drag gesture.** Cheetah3D drags in the viewport for a signed shell; we
  take a non-negative thickness plus the documented Inside switch, and the tool
  rejects a negative distance instead of letting its sign cancel Inside. Closing
  this needs a viewport drag mode that maps the drag distance to the same
  geometry call. The Angle field clamps to 0-180° and Quantize to a
  non-negative step, so the panel never shows a value the geometry would
  silently adjust.
- **What Angle measures.** The manual only says bigger values group more
  neighbours. We use the angle between neighbouring face normals; closing the
  gap needs the real tool's threshold semantics.
- **Quantize target.** The manual calls it "the distance you want to quantize the
  shell to"; we snap the shell thickness to that step, with 0 disabling it.
- **Defaults.** Inside on, a 30° angle, 0.1 m thickness and no quantize are our
  choices; the page documents no default values.
- **Corner offset rule.** Points move along averaged incident face normals, which
  miters corners; Cheetah3D does not document its corner handling.
- **Orientation and partial closed meshes.** The copy always faces back toward
  the original, so an inside shell is a correctly oriented solid while an outside
  shell's copy faces inward; the page documents only the side, not the resulting
  normals. Shelling a partial selection of a closed mesh gives it a pocket whose
  rim edges are then shared by three faces, a case the manual does not describe.

## Highest-impact remaining gaps

1. **Rigging and weight acquisition** — chosen-joint binding, proximity
   auto-weights, preserve-on-rebind, the brush, heat map, auto-normalise,
   tip-origin IK with take baking and clean-room per-joint limits are in place;
   viewport joint picking, heat-diffusion weighting, Spline IK, Cheetah3D-style
   IK Constraint tags and the broader constraint system remain.
2. **glTF import and tangent semantics** — export covers skins, morphs and
   animations, native morph-weight F-curve rows are keyable, and the four
   documented interpolation modes plus per-side Tangent Type controls are now
   in the F-curve editor. Import now carries a mesh's morph base, target names,
   offsets and default weights, cameras, and every punctual light type with its
   cone angles and falloff distance, refusing inconsistent or unusable data
   with a message that names the node; skinning, animation and soft shadows
   driven by the light radius remain. Cheetah3D's public docs do not list a glTF importer,
   so LeopardCraft's import remains an extension; the manual does not specify
   a tangent slope formula, and Cheetah3D's exact morph-weight range remains
   undocumented rather than matched.
3. **UV workflow** — explicit seams, seam-driven unwrap and pinned UVs that
   unwrap, relax and normalize all honour are in place; chart tools,
   multi-point UV pin selection, seam authoring from the UV view, multiple UV
   sets and UV export are the missing daily-modelling pieces.
4. **Remaining polygon tools and modifiers** — Fill Hole, Cover, Create Polygon,
   Split, Shell, Triangulate, Flip Normal, Collapse, Disconnect, Optimize and
   Point Slide are in place; scalpel and clone remain, together with
   displacement, magnet, polygon reduction, Laplacian smooth and wrap
   modifiers, crease weights, partial subdivision and the exact Stam-Loop
   rules; each is a self-contained operation with a clear undo boundary.
5. **Scripting/plugin surface** — valuable for parity, but it needs an API
   design and cannot be reduced to a single feature slice. A headless MCP server
   now provides external agent automation, but embedded scripting and a plugin
   API remain open.
6. **Second renderer and advanced AOV/bake output** — expensive and lower
   priority than authoring and interchange.
