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
42. Manual mirror (Clone tool: modes and Copy/Holes/Direction/Scale/Rotation/variation properties) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tools/Polygon/CloneTool/CloneTool.html>
43. Manual mirror (Scalpel tool: edge/point clicks, chained cuts and ESC) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tools/Polygon/ScalpelTool/ScalpelTool.html>
44. Stam and Loop, *Quad/Triangle Subdivision*, Computer Graphics Forum 22(1):79–85 (2003) — <https://www.dgp.toronto.edu/public_user/stam/reality/Research/pdf/qtEG.pdf>
45. Warren and Schaefer, *A Crash Course on Subdivision* (the same two-pass factorization with explicit quad/triangle masks) — <https://www.cs.rice.edu/~jwarren/papers/subdivision_tutorial.pdf>
46. Manual mirror (Ring Cut tool: edge click, drag adjust and Shift normal move) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tools/Polygon/RingCutTool/RingCutTool.html>
47. Manual mirror (Weld tool: two point clicks, To property and ESC) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tools/Polygon/WeldTool/WeldTool.html>
48. Manual mirror (Bridge tool: two border point clicks) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tools/Polygon/BridgeTool/BridgeTool.html>
49. Manual mirror (Insert Point tool: polygon, edge and empty-space clicks) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tools/Common/InsertPointTool/InsertPointTool.html>
50. Manual mirror (Catmull-Clark Subdivide tool: menu command on the mesh or the selected polygons) — <https://tgjp.github.io/Cheetah3D_jp/Cheetah3Dmanual/Tools/Polygon/SubdivideTool/SubdivideTool.html>

## Matrix

| Area | Cheetah3D 8.1.3 | LeopardCraft | Status | Notes |
| --- | --- | --- | --- | --- |
| Modelling — primitives | 12 parametric primitives | 8: box, sphere, capsule, cylinder, cone, disc, torus, plane | Partial | No tube, polygon, subdivision or other creator objects |
| Modelling — splines | 9 spline types | 1 spline object with linear, Bézier and smooth interpolation | Partial | No separate Helix, Spiral, Text, Path or similar creators |
| Modelling — polygon tools | Extrude, inner extrude, bevel, bridge, collapse, cover, create polygon, crumple, disconnect, edge rotate, fill hole, flip normal, jigsaw, normal move/rotate/scale, optimize, point slide, ring cut, scalpel, shell, split, triangulate, weld, array and clone | Extrude, inset, bevel, ring cut, weld, collapse, bridge, fill hole with border-click tool, cover, create polygon, flip normal, split, disconnect, optimize, point slide, shell, triangulate with Shortest Diagonal/Quad Pair, Clone with Copies/Holes/Direction/Scale/Rotation/variations, scalpel between two boundary picks, component move/rotate/scale, material assignment | Partial | Scalpel cuts one polygon between two edge or point picks and the click tool chains as documented (a cut's end point becomes the next start, ESC resets, source 43), with the polygon under the pointer resolving the face when two picks share an edge; Split copies the polygons instead of moving them (source 40), and Shell uses typed thickness/Inside/Angle/Quantize fields instead of the documented drag gesture (source 41), and the Ring Cut click tool cuts the plane perpendicular to the picked edge instead of sliding on drag (source 46) |
| Modelling — modifiers | Roughly 18 documented modifiers including subdivision, displacement, formula, magnet, polygon reduction, smooth and wrap | 21: subdivision (Catmull-Clark, Stam-Loop or Linear), extrude/bevel faces, array, symmetry, boolean, bend, taper, twist, shear, bulge, spherify, displacement, magnet, lathe, sweep, ring cut, formula, Laplacian smooth, polygon reduction and wrap, with shared Cage/Type controls on the deformers | Partial | Displacement reads a procedural noise field instead of a texture, Formula is a clean-room per-axis expression dialect, and Laplacian smooth, polygon reduction and wrap are named approximations of undocumented pages; Boolean and subdivision favour robustness over speed |
| Modelling — subdivision | Catmull-Clark, Stam-Loop and linear subdivision; creased edges under the smooth schemes | Catmull-Clark, Stam-Loop and Linear schemes; full and fractional per-edge creases honoured by both smooth schemes and visible in every selection mode; watertight face-selective Subdivide tool; Constrained/Unconstrained/Cage Type on the deformers | Partial | Stam-Loop follows the published hybrid (sources 44 and 45): quads stay quads, triangles split into three quads, quad-only rings use Catmull-Clark masks, triangle-only rings use Loop masks and mixed rings use the paper's alpha mask. The optional Table 1 vertex correction and the figure-only mixed-edge mask are not reproduced — a quad/triangle edge takes the mean of both edge rules — and displacement reads a noise field instead of a texture |
| Modelling — booleans | Union, subtract, intersect | Union, subtract, intersect BSP boolean modifier | Yes | Mesh operand is chosen in the modifier stack |
| UV | UV editor, cubic/spherical/flat/cylindrical/frontal mapping, ABF/LSCM unwrap, Autoseam, pinned UVs, UV PDF | UV view with multi-point selection, seam authoring (toggle/all/loop/clear), multi-point pins, chart move/rotate/scale, shelf packing with padding/rotation/alignment, weld/split/align/distribute/flip/rotate/numeric entry, UV0/UV1/… sets, per-texture UV set binding, checker and distortion overlays | Partial | No ABF/LSCM, Autoseam or UV PDF export; charts are a seams-and-angle splitter and packing is a shelf packer, not a distortion-minimising solver |
| Materials | Node materials, PBR/Principled workflow, procedural textures | Node material graph, Principled surface, procedural noise, texture cache | Partial | Core authoring path exists; not every Cheetah node type is present |
| Rendering — engines | Cheetah raytracer (irradiance cache, photon maps) and Falcon path tracer | One CPU path tracer with progressive accumulation | Partial | No second engine or photon-mapped caustics |
| Rendering — camera/lighting | HDRI/sky, area/spot/point/distant lights, depth of field, motion blur (Falcon), caustics | Environment/HDRI, sun, directional, point, area and spot lights (cone angle controls), depth of field | Partial | No motion blur or caustics; point and spot radius samples a spherical shell for soft shadows (a clean-room approximation), while area lights use their own shape |
| Rendering — output | Up to 8K, G-buffer/AOV output, texture baking | PNG and EXR, single-frame background render at editable resolution | Partial | No 8K preset, G-buffer, baking or animation-sequence render |
| Animation — curves | Keyframes and F-curve editor; Constant, Linear (manual default), Smooth and Spline interpolation; `Animation -> Tangent -> Type`, `Left Tangent -> Type` and `Right Tangent -> Type` | Keyframes, F-curve editor, all four per-key interpolation modes, selection-wide `Tangent`/`Left Tangent`/`Right Tangent` Type menus, Spline tangent handles for scalar/morph curves, quaternion interpolation with per-side tangent types, key select/drag/copy/paste, key filtering, selected-key factor retiming and an editable global playback range/loop | Partial | Quaternion curves have no scalar handles; Cheetah3D calls Linear its default while LeopardCraft records new keys as Smooth |
| Animation — takes | Take Manager for multiple animation takes | Document-level named takes with an active take, per-take fps, node/tag/material clips, create/rename/duplicate/delete/activate, and whole-clip retiming | Partial | The Takes tab is a compact document list rather than Cheetah3D's multi-column Take Manager; take-level render/visibility flags and split/trim commands are absent |
| Character — skinning | Joints, bind mesh with automatic weights, max influence/dropoff, weight painting with add/subtract brushes and auto-normalise | Joint objects, chosen-joint bind/rebind with normalized proximity auto-weights (dropoff/strength/max influences), four-influence paint brush, active-joint heat map | Partial | No heat-diffusion solver; selected-joint, preserve-on-rebind and screen-space viewport joint picking are clean-room extensions; brush UI details are native |
| Character — rigging | FK, IK, spline IK and constraints | Joint transforms; tip-origin IK chains with target/optional pole, analytic two-bone and damped CCD solving, reach clamping, per-joint local XYZ rotation limits, guide-curve Spline IK, chain stiffness, IK Point/Angle/Orient/Parent tags and per-frame take baking | Partial | The limits and tags are clean-room approximations, not Cheetah3D's IK Constraint tag/heading-pitch-bank semantics or documented 3D-only behavior; the chain stiffness is the opposite convention to the documented tag stiffness; Spline IK follows a guide as a chain of chords because the manual defines no solver; bake uses the global playback range and writes ordinary rotation F-curves |
| Character — morphs | Morph tag with recorded base and named target poses | Morph targets with recorded base, named poses, capture/restore, in-place base updates that keep target poses, and undo; dynamic Morph Weight F-curve rows, evaluation with 0–1 clamping, native round-trip and glTF export | Partial | Weight rows use LeopardCraft's F-curve interpolation, base rebasing keeps each target's absolute pose (the manual does not define that transition), and the 0–1 range is clean-room; Cheetah3D documents neither the numeric range nor morph-specific interpolation; glTF import now reads morph base, target names, offsets and default weights, but not skinning or animation |
| Dynamics | Bullet rigid and soft bodies, particles | None | No | |
| Scene management | Tags, takes, layers, Smart Folder, autosave | Tags, document takes, layers, hierarchy, object browser, autosave/recovery, selection sets and multi-object editing | Partial | No Smart Folder or take-level render/visibility flags |
| Interchange — native | `.jas` scene format | `.lcscene` JSON document with validation and recovery | Yes | Clean-room format, not Cheetah3D-compatible |
| Interchange — import | FBX, OBJ/MTL, 3DS, DAE, USDZ, SCN, STL, DXF, SIA (public 8.1.3 docs list no glTF/GLB import) | OBJ/MTL, STL, glTF/GLB (mesh, materials, morph targets, cameras, punctual lights, skins and animation channels) and native `.lcscene` | Partial | LeopardCraft's glTF/GLB import is an extension beyond the documented Cheetah3D import list; cubic quaternion rotation becomes Smooth keys and camera `aspectRatio` hints are reported instead of read. No FBX, 3DS, DAE, USDZ, SCN, DXF or SIA |
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
  a 0–1 slider, removes targets and restores the base. The Object Browser's
  **Morphs** tab lists the same targets by owning object with weight and derived
  On/Off state, and selecting a row selects that object.
- Each target contributes a dynamic **Morph Weight** row to the object's
  F-curve editor. The row reads and writes `AnimationClip.morph_weights`, keyed
  by target index, and uses the same key selection, drag, copy/paste,
  interpolation and delete paths as transform channels. Recording a row stores
  the target's current weight; the row shows the target name and a per-target
  interpolation control.
- Animation keys address a target (the clip owner, an object id, a tag or a
  material), a registry channel and a component. The public Cheetah3D manual
  lists which parameters are animatable but does not publish how its files
  address them, so this address model is a LeopardCraft clean-room choice. Old
  files that keyed flat property names load into owner-relative addresses, and
  their Euler rotation curves stay readable.
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
  on other scopes do not exist. glTF import now reads skins and animation, and export does not
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
  later selected keys move toward or away from it. **Takes: Retime**
  retimes every key in the selected object's active-take clip around **0 seconds**.
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
- Selected-key retiming and active-take clip retiming are each one undo step that
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
  constant/linear/smooth falloff, plus a joint list, screen-space handles on the
  bound joints that set the active joint, and a per-vertex heat map that shows
  zero-weight vertices in grey;
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
  concepts where the sources are solid. Only-visible brush filtering,
  heat-diffusion weighting and the constraint system are not implemented.
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
- Spline IK: a chain may name a Spline node, and the inspector offers it in the
  **Spline** row next to the point target. The clean-room semantics sample the
  spline into a world-space polyline and aim each bone at the curve point one
  bone length further along from that joint's projection onto the curve, twice
  per solve. Rotating only keeps every bone length exact, so the chain
  approximates the curve as a chain of chords; the tip reports its distance from
  the curve, a guide shorter than the chain clamps to its end instead of
  stretching, joint limits clamp after each correction, and a missing or
  non-spline node falls back to the point target. Cheetah3D's public pages list
  Spline IK but define neither its solver nor its twist control, so this is an
  explicit approximation rather than claimed parity;
- stiffness: `IkChain.stiffness` blends every solved joint back toward its
  pre-solve rotation once, after the solve, so 1 keeps the full solve, 0 keeps
  the rest pose and 0.5 lands halfway. One shared step serves the analytic
  two-bone/pole, CCD and spline paths; joint limits re-clamp after the blend, and
  the blended rotations are what skinning, playback and Bake IK read. The
  inspector exposes it as a **Stiffness** slider. Cheetah3D's manual instead
  describes a tag stiffness of one meaning the joint does not bend; the two
  conventions are opposites, and this chain-wide blend was chosen so a file
  without the field keeps solving exactly as before;
- four clean-room constraint tags evaluated in the existing post-IK tag pass:
  **IK Point** lerps the owner's world origin toward a target's origin by
  `weight` (on a joint this translates the joint itself, not the bones around
  it), **IK Angle** keeps the owner's local +X axis inside a `max_degrees` cone
  around the direction to a target by rotating the excess back toward it (it
  only pulls, never pushes), **Orient** slerps the owner's world rotation toward
  a target's, and **Parent** blends a target's world position and rotation. Blend
  weights are measured against the pose the solver produced, so the evaluator's
  two ordering passes reproduce the same result instead of compounding a
  partial weight; tags still apply in list order, so a later Clamp Rotation can
  win. A joint whose only driver is one of these tags is baked by **Bake IK
  take**. The Tags section adds them with Target and Weight/Cone rows;
- skinning consumes the solved world matrices, so a bound mesh deforms through
  IK-driven joints. Skin arrays and their weights are not rewritten by a solve.

Deliberate limits: the per-joint limits and the four constraint tags are not
Cheetah3D's IK Constraint tag: Cheetah3D documents heading/pitch/bank
coordinates on a tag, marks the limit properties as 3D-solver-only and
publishes no solver for its IK tags, while LeopardCraft uses local XYZ Euler
degrees on the joint and clean-room world-space rules (aim cone, origin pull,
rotation copy, transform follow). Stiffness here is a chain-level blend where 1
is a full solve, the opposite of the documented tag stiffness where one means
the joint does not bend. Spline IK follows a guide curve as a chain of chords
with no twist or up-vector control, and the manual publishes no spline-IK solver
to match. The target and pole are node origins, not arbitrary control surfaces.
Tag constraints still run after IK and can override a solved joint; the broader
constraint system remains separate. Bake uses the global playback range rather
than document-level takes, and IK settings are not a glTF interchange feature
(baked keys are). The two-bone pole is an exact bend-plane selection; the CCD
pole is a preferred plane after convergence, not a hard twist constraint. glTF
import and Cheetah3D-style
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

Skins and animation import after the node walk, when every joint node exists.
A node's `skin` becomes a `Skin` whose joints are the mapped node ids,
`inverseBindMatrices` become the inverse binds (identity when the accessor is
absent), and `JOINTS_0`/`WEIGHTS_0` are copied per imported point, clamped to
four influences and renormalized. A joint index outside the skin, missing
`JOINTS_0`/`WEIGHTS_0` on a skinned mesh, or a malformed inverse-bind accessor
fails with a message naming the node. Animation channels keep glTF's seconds
and interpolation: STEP becomes Constant, LINEAR becomes Linear, CUBICSPLINE
becomes Spline with the authored tangents stored as hand-authored slopes, and
cubic quaternion rotation becomes Smooth because quaternion keys have no
tangent handles. The first animation fills the active take, later ones become
additional takes, and a take's frame range is its clip duration at that take's
fps.

One authored field is still not representable: a camera `aspectRatio` hint,
because LeopardCraft takes aspect from the render output. It is the only field
`ImportReport` still names. When a report is non-empty, Open and Import stage
the parsed document and show an **Import Report** window before changing the
scene; Cancel leaves the document untouched. MCP `open_scene` returns the
summary as `import_report`, and headless startup prints it to stderr.

## Clone: what this slice added

Cheetah3D's Clone tool creates shifted copies of a polygon or point selection
and exposes Copies, Holes, Direction, Scale, Rotation, position/scale/rotation
variation and Keep ratio (source 42). LeopardCraft implements the documented
property set in point and polygon mode; with no selection it clones every point
or polygon, as the manual states. Each copy is transformed around the selection
centroid, the last copy lands at the end of the Direction vector, Scale and
Rotation interpolate from the identity at the start to the authored values at
the last copy, and rotation is around Direction. Holes skips whole copies at the
requested probability rather than punching holes in faces; the manual describes
the value as only approximate, so this is a clean-room interpretation.

Variation uses `Pcg32` with a fixed seed instead of wall-clock randomness, which
keeps evaluation deterministic and makes repeated Apply commands reproducible.
With Keep ratio the scale variation is one uniform factor; without it each axis
varies independently. Cloned faces keep their corner UVs, material slots and
creases, seams and fractional crease weights inside the selection; point-only
cloning adds loose points and invents no faces. Mesh Editing's **Clone** section
and `Tools -> Clone` commit one undo step.

## Triangulate: what this slice added

Cheetah3D lists a Triangulate polygon tool in its feature set (source 1); the
manual page for it is not among the pages indexed here, so the split rule below
is a LeopardCraft clean-room choice rather than a documented Cheetah3D rule.

`lc_geom::triangulate_faces_with` splits each selected polygon into `corners - 2`
triangles. **Shortest Diagonal** (the default) ear-clips the polygon, removing
the valid convex ear with the shortest remaining diagonal first; **Quad Pair**
splits a quad on its shorter diagonal and ear-clips other n-gons. The Mesh
Editing **Split** selector chooses between them, replacing the earlier fixed
fan from corner 0. Only selected polygons change: everything else keeps its
n-gons, so the operation edits exactly what the user selected. Each triangle
reuses the original corner records, which keeps point indices, UV slots and
material slots intact, and because creases, seams and pinned UV slots are
point/corner indices they survive untouched; the new diagonals carry no crease
or seam. Degenerate triangles are skipped so the result still validates, and
out-of-range face indices are ignored. Mesh Editing's **Triangulate** button
and `Tools -> Triangulate` share one predicate — polygon mode on editable
meshes with at least one selected polygon of more than three corners — so the
command is refused with a reason rather than silently doing nothing, and one
undo step covers the action.

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

LeopardCraft now adds stack equivalents for those deformers with the shared
**Type** control: **Constrained** clamps a point to the cage before deforming
and reuses that border displacement outside it, **Unconstrained** deforms
freely, and **Cage** modifies only points inside the cage (the manual warns
this can crack at the border). **Cage** is the cage size as a multiple of the
object's half bounds. The six deformers and all three documented subdivision
schemes, including creased edges, are:

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
- **Displacement** offsets every point along its area-weighted normal by smooth
  deterministic value noise: `amplitude` scales the field, `frequency` scales
  the sample position and `seed` selects a different field. Cheetah3D documents
  a texture input; LeopardCraft has no texture a modifier can read, so the
  procedural field is the clean-room substitute and the same mesh and
  parameters always give the same geometry.
- **Formula** evaluates one expression per axis in the point's rest `x`, `y` and
  `z`, so a point can be repositioned by `x + 1`, `2 * y - z` or
  `sin(x * pi)`. Expressions support `+ - * / ^`, parentheses, unary minus, the
  constants `pi` and `e`, and `sin cos tan abs sqrt exp ln floor ceil round sign
  min max`. An empty expression keeps that coordinate and a non-finite result
  (division by zero, an out-of-domain `sqrt`) keeps it too, so a formula cannot
  write NaN into geometry; a malformed expression is refused by the add/update
  predicate with a message naming the axis and the syntax problem. The manual
  does not document Cheetah3D's formula dialect here, so this grammar is a
  clean-room reading.
- **Magnet** is an attraction field: points inside `radius` move toward the
  bounds centre by `strength` times a smoothstep falloff that is 1 at the centre
  and 0 at the radius, so points beyond the radius feel no field. Negative
  strength pushes points away, strength is clamped to -1..=1, and the shared
  Cage/Type control applies like the other deformers.
- **Laplacian Smooth** runs the uniform umbrella operator: each pass moves every
  point `strength` of the way toward the average of its edge neighbours, with
  the iteration count capped at 64 per evaluation. With **Keep border** set, the
  points of a boundary edge stay put and only the interior relaxes.
- **Polygon Reduction** collapses the shortest edge that belongs to a triangle,
  at its midpoint, until the face count reaches `ratio` of the authored count.
  Only triangle edges are candidates because collapsing one always removes a
  face, which is what makes the loop terminate instead of merely turning quads
  into triangles; a quad-only cage is therefore returned unchanged. Creases,
  seams and pinned UVs follow the surviving point through the same merge tail
  Collapse and Optimize use, so the reduced mesh still validates. This is a
  deterministic greedy decimator, not a quadric-error implementation.
- **Wrap** rotates points around the target object's centre line: the object's
  normalized position along the chosen X/Y/Z axis becomes a rotation angle of up
  to `angle_deg / 2` at each end, so the object curls around the target while
  keeping each point's distance from that line. It is created with **Add Wrap**
  because it needs a target mesh, and the target is evaluated and moved into the
  wrapper's local space exactly like a Boolean operand. A Wrap that names its
  own object has no operand and leaves the mesh alone.
- **Subdivision** now offers **Catmull-Clark**, **Stam-Loop** and **Linear**
  schemes beside its level count, which now defaults to 2 like Cheetah3D.
  Catmull-Clark produces quads from any n-gon. Stam-Loop is the published
  hybrid (sources 44 and 45): each iteration splits every n-gon into n quads
  around its centroid — three for a triangle, four for a quad — and positions
  the old vertices, edge points and face points with Catmull-Clark masks on
  quad-only rings, Loop masks on triangle-only rings and Stam and Loop's mixed
  mask (`alpha` on the vertex, `alpha/2` on each edge midpoint, `alpha/4` on
  each face centroid) where the two meet, so a mixed cage stays all quads. An
  edge between a quad and a triangle takes the mean of the two edge rules, and
  the optional Table 1 vertex correction is omitted because the pure regions
  already use the exact Catmull-Clark and Loop masks. Linear splits every n-gon
  into n quads around its centroid and
  shares edge midpoints between neighbours, keeping the surface watertight
  without smoothing it. Old documents without a scheme load as Catmull-Clark,
  and new modifiers default to it.
- **Edge creases** are a per-mesh list of canonical point pairs, so they are
  independent of temporary edge ordering and survive point compaction,
  welding and mesh merging. Full creases live in one sorted list; fractional
  strengths live in a second sorted list keyed by the same canonical pair, and
  `set_crease_weight` moves an edge between them at 0 and 1. In Edge mode the
  Mesh Editing panel offers **Toggle Crease** and a 0–1 **Weight** field with
  **Set Crease Weight**; creased edges are drawn blue in every selection mode.
  Catmull-Clark computes both rules and interpolates: the edge point is
  `lerp(smooth edge point, midpoint, weight)`, and a vertex with two incident
  creases interpolates from its smooth vertex point to the 3/4 + 1/8 + 1/8
  crease rule by the smallest incident weight. A vertex with three or more
  creases blends from smooth to pinned the same way, and a single crease stays
  smooth. Both child edges inherit the parent weight, so fractional creases
  survive further levels. Stam-Loop blends its region's edge point and crease
  vertex rule by the same weights. Linear subdivision deliberately ignores
  crease sharpness, so its geometry is unchanged; the crease marks and weights
  are still split onto child edges for later editing. Loading normalizes
  duplicate, degenerate and out-of-range entries, clamps weights to 0–1 and
  promotes a weight of 1 into the full-crease list.
- **Partial subdivision**: `subdivide_faces` subdivides only a face selection.
  It builds a patch from the selected faces, runs any of the three schemes,
  pins every original patch-boundary vertex and new boundary point onto the
  original boundary segments, then re-adds the unselected faces with the new
  collinear boundary points threaded into their corner lists in order. Every
  selected boundary edge is therefore shared by the patch and its unselected
  neighbour through the same shared midpoints, and unselected vertices keep
  their exact positions. UVs for unselected corners are appended to the
  patch's per-point UV slots, extra UV sets are recomputed through the same
  path, and creases, weights, seams and material slots are preserved. The Mesh
  Editing **Subdivide** button and `Tools -> Subdivide` expose level count and
  scheme and commit one undo step.

The modifier inspector edits each parameter in the stack, and the geometry
layer has unit tests for shape change, finite extremes, watertight linear
topology, three-way scheme distinctness on a mixed quad/triangle cage,
crease persistence and sharpening under Catmull-Clark and Stam-Loop at
multiple iteration counts, linear crease independence, material-style
serialization and legacy documents with neither crease data nor a scheme
field.

Deliberate gaps: Stam-Loop follows the published hybrid topology and masks, but
the paper's optional vertex-correction table and its figure-only mixed-edge mask
are not reproduced: pure regions use the exact Catmull-Clark and Loop masks, and
an edge between a quad and a triangle takes the mean of the two edge rules.
Displacement, Formula, Magnet, Laplacian Smooth,
Polygon Reduction and Wrap now exist as clean-room readings of the documented
modifier names; the pages that would pin their exact parameters (a texture input
for Displacement, Cheetah3D's formula dialect, its decimation algorithm) are not
among the indexed sources, so those parameter sets are approximations rather
than verified parity.

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
- **Relax** is chart-aware: its graph is split at every seam and at the same
  normal-angle borders `automatic_unwrap` uses, so neither an authored cut nor a
  sharp fold is averaged onto one shared point value. Seam, normal-angle-break
  and mesh-boundary corners are held when the pin-boundary option is on (the
  editor's Relax command passes `true`), and without pins the smoothed charts
  are shelf-packed back into 0..1 so two charts cannot drift into each other's
  bounds. The manual defines seams only as unwrap cuts (source 37: "A seam is an
  edge where the mesh will tear up when calling the Unwrap UV command") and
  documents no Relax tool, so applying the same chart rule and adding a pack
  step to relaxation is a LeopardCraft clean-room choice rather than a
  documented Cheetah3D behaviour.
- Subdivision inherits seams onto both child edges of a split seam edge, the way
  it inherits creases, so a seam survives a subdivision modifier stack.
- Native `.lcscene` documents serialize seams only when present; loading
  removes duplicate, degenerate and out-of-range entries, and older documents
  without the field load with no seams.

Clean-room limits of this slice: unwrapping is a normal-angle chart splitter
with seam barriers and a shelf packer, not ABF, LSCM or Autoseam, so it does not
minimise distortion the way the documented Cheetah3D solvers claim to. The later
G34 UV authoring suite added seam authoring and seam selection inside the UV
view; its clean-room policy is described in "UV authoring suite" below.

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

## UV authoring suite: what this slice added

The manual's UV editor documents point and edge selection, seams, pins, mapping
modes, Unwrap UV and packing, but not the internal chart splitter, slot topology
or how a renderer binds a second set (source 37 and the UV matrix row).
LeopardCraft therefore uses its own clean-room model:

- A chart is a group of connected faces across edges that are neither authored
  seams nor normal-angle breaks, reusing the unwrap threshold. Chart tools treat
  a chart as one island; there is no ABF/LSCM/Autoseam solver, so packing
  controls (padding, optional 90° rotation, block alignment) change placement,
  not distortion.
- Chart transforms move/rotate/scale the selected charts' coordinates around
  their combined centre. Packing skips charts that contain a pinned slot and
  reports how many it left in place, so a pin stays exact; this is a documented
  LeopardCraft policy, not a claim about Cheetah3D.
- Weld merges selected slots that share a mesh point (the lowest index
  survives); Split gives every selected shared slot its own per-corner slot.
  Both change the shared slot topology, so they update every UV set and remap
  pins. Align, distribute, flip, 90° rotation and numeric entry only rewrite
  coordinates on the active set.
- Multiple UV sets are parallel coordinate arrays over one shared slot topology
  (`Mesh::extra_uvs`). Old documents load as UV0, native files keep every set,
  and glTF writes and reads `TEXCOORD_1`…; OBJ writes UV0 and records the sets
  it skipped. Material texture and lightmap binding to a non-zero set is not
  implemented, so that part of the manual's workflow remains an open
  rendering-lane gap.
- The checker and distortion overlays are view state. Distortion is a
  normalized UV-area to surface-area ratio, not the manual's UV PDF or a solver
  error metric.

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

- **Invocation.** The selection command remains, and the manual's click
  workflow is now provided by the **Click tool** checkbox (also
  `Tools -> Fill Hole Tool`). It maps a viewport click to the nearest point on
  an open boundary and calls the same `fill_holes` operation, stays active for
  multiple holes, and ignores clicks away from a border. The manual documents
  no deactivation key, so the tool stays on until its checkbox is cleared.
- **UVs.** The manual documents no properties, so the new polygon's UVs are a
  LeopardCraft choice: the border is projected onto its best-fit plane and scaled
  into the loop's own 0..1 box. Cheetah3D's own fill UVs are undocumented; true
  parity would need an observable reference, and until then the user can re-map
  the new face from the UV editor.
- **Material slot.** The new face inherits the lowest material slot of the faces
  on the border. The manual does not state a rule.
- **Topology.** Each loop becomes one n-gon. A hole whose border encloses a
  separate mesh island is rejected with `that hole encloses an island; fill or
  remove the island first` instead of placing an n-gon through it; the island is
  detected by connected-component ownership and planar containment of the
  border. The manual does not document this case, so refusing is the clean-room
  choice that cannot silently hide geometry. A border that is not a single
  closed loop is not filled either; the operation rejects borders whose edges
  have two faces and never invents points.

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
   tip-origin IK with take baking, clean-room per-joint limits, guide-curve
   Spline IK, chain stiffness and the IK Point/Angle/Orient/Parent tags are in
   place; heat-diffusion weighting, Cheetah3D's IK Constraint tag semantics and
   the broader constraint system remain.
2. **glTF import and tangent semantics** — export covers skins, morphs and
   animations, native morph-weight F-curve rows are keyable, and the four
   documented interpolation modes plus per-side Tangent Type controls are now
   in the F-curve editor. Import now carries a mesh's morph base, target names,
   offsets and default weights, cameras, and every punctual light type with its
   cone angles and falloff distance, refusing inconsistent or unusable data
   with a message that names the node; skinning and animation import remain.
   Cheetah3D's public docs do not list a glTF importer,
   so LeopardCraft's import remains an extension; the manual does not specify
   a tangent slope formula, and Cheetah3D's exact morph-weight range remains
   undocumented rather than matched.
3. **UV workflow** — explicit seams, seam-driven unwrap and pinned UVs that
   unwrap, relax and normalize all honour are in place; chart tools,
   multi-point UV pin selection, seam authoring from the UV view, multiple UV
   sets and UV export are the missing daily-modelling pieces.
4. **Remaining polygon tools and modifiers** — Fill Hole, Cover, Create Polygon,
   Split, Shell, Triangulate, Flip Normal, Collapse, Disconnect, Optimize, Point
   Slide, Clone, Scalpel and the Displacement, Formula, Magnet, Laplacian
   Smooth, Polygon Reduction and Wrap modifiers are in place; the Scalpel's
   click chaining now follows source 43, and the Stam-Loop hybrid follows the
   masks in sources 44 and 45; each is a self-contained operation with a clear
   undo boundary.
5. **Scripting/plugin surface** — valuable for parity, but it needs an API
   design and cannot be reduced to a single feature slice. A headless MCP server
   now provides external agent automation, but embedded scripting and a plugin
   API remain open.
6. **Second renderer and advanced AOV/bake output** — expensive and lower
   priority than authoring and interchange.

## Viewport click workflows: researched facts

Cheetah3D splits its polygon tools into pick-driven tools, whose input is a
viewport click, and selection-driven tools, which act on the current component
selection from the menu. The pick-driven set is small, and every one of them is
"pick an element, then commit on a second element or on release":

- **Scalpel** (source 43): click an edge (green) or point (blue) as the start,
  then click a second edge or point of the same polygon to split it in two; the
  end point becomes the next cut's start, so cuts chain until ESC resets.
- **Ring Cut** (source 46): one edge click selects the ring; dragging before
  release slides the cut, and Shift-drag moves it along the surface normals. The
  ring stops at a polygon with an odd edge count.
- **Weld** (source 47): click one point, then a second point; the `To` property
  chooses the midpoint or the first point, and ESC resets.
- **Bridge** (source 48): click a point on the border of each of the two polygon
  groups; the groups are bridged.
- **Insert Point** (source 49): a click inserts a point where the cursor is —
  inside a polygon (which is split into triangles), on an edge, or in empty
  space on the camera-facing plane.
- **Fill Hole** (source 38): already implemented as a border-click tool.

Everything else in the polygon menu is selection-driven rather than
click-driven. The Catmull-Clark Subdivide tool subdivides the whole mesh from
the menu command and only the selected polygons when polygons are selected
(source 50); Triangulate, Clone, Collapse, Cover, Split, Shell, Extrude and
Inner Extrude, Point Slide, Optimize, Flip Normal, Disconnect and the normal
transforms are applied from the menu or their property panel.

Consequence for LeopardCraft: a viewport click tool is warranted only where the
operation's input is a picked point, edge or loop. Subdivide, Triangulate and
Clone are selection-driven in the source application and stay menu actions.
Every pick-driven tool now has a click form: Scalpel with its chained cuts,
Fill Hole's border click, and Ring Cut, Weld, Bridge and Insert Point through
the same mutually exclusive **Click tool** selector. Two documented behaviours
remain gaps: Ring Cut cuts the plane perpendicular to the clicked edge instead
of sliding on drag or moving along normals, and Insert Point refuses empty-space
clicks because an isolated camera-plane point has no representation in the
document model.
