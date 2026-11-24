# LeopardCraft remaining-work roadmap

Created 2026-11-24 from an audit of every unchecked box in `TODO.md` plus the
section-prose gaps that are not boxes. Each `G` file is a startable batch in the
same format as `G29.md`: lane packets, allowed paths, acceptance checks and a
definition of done. Missions are ordered by dependency and may run only when
their prerequisites have landed. `G30`, `G31`, `G32`, `G33` and `G34` are
complete; `G35` is the next dispatch.

## Mission index

| Mission | Outcome | Lanes | TODO coverage | Depends on |
| --- | --- | --- | --- | --- |
| G30 | In-place morph base update; distinct spot-light icon | editor | §4 update base; §5 spot icon | — |
| G31 | Morph presentation and global playback controls | editor | §4 morph panel; §1 playback range and loop mode | — |
| G32 | Document-level takes with per-take fps (complete) | document, editor, interchange | §1 takes; §1 per-take fps | — |
| G33 | Channel addresses and sampled evaluation (complete) | document, editor, interchange | §1 `AnimProperty`; §1 grouped channels; §1 scratch sampling | G32 |
| G34 | UV authoring suite (complete) | geometry, document, editor, interchange | all seven §3 boxes | — |
| G35 | glTF skin and animation import | interchange, document, editor | §5 import; §5 pre-import summary; §5 round-trip tests | G32, G33 |
| G36 | Modifier and subdivision parity | geometry, document, editor | all three §6 boxes | — |
| G37 | Polygon-tool completion | geometry, editor | all three §6b boxes | — |
| G38 | Rigging constraints | document, editor | §2b prose: Spline IK, constraint tags, stiffness | G32 |
| G39 | MCP automation expansion | automation, editor | all three §7 boxes | G38 |

**Status:** G30–G35 and G38 are complete and verified; G36, G37 and G39 are
partially complete with their remaining items recorded in `TODO.md`. Every other
mission in this index is planned and blocked only by its listed dependencies.

## Coverage check

Every unchecked box in `TODO.md` maps to exactly one mission:

- §1 Animation Core: `AnimProperty` address and scratch sampling → G33; takes
  and per-take fps → G32; playback range/loop → G31; grouped channel list →
  G33.
- §3 UV Workflow: seam authoring from the UV view, multi-point pins, chart
  selection/transform/packing, UV editing operations, multiple UV sets, checker
  overlay/distortion stats and multi-set round-trip tests → G34.
- §4 Morph Targets: update the recorded base in place → G30; expose targets in
  the object browser or a tag-style panel → G31.
- §5 glTF Interchange: pre-import skip summary, skin/animation import and the
  skin/animation round-trip tests → G35; spot-light icon → G30.
- §6 Modifier parity: Cage/Type controls, exact Stam-Loop/crease/partial rules
  and the missing modifiers → G36.
- §6b Polygon tools: scalpel and clone, triangulation split direction, nested
  holes and viewport-click invocation → G37.
- §7 MCP automation: live-editor attachment, authoring tools and the HTTP /
  subscription / prompt surface → G39.
- §8 Agent workflow: the remaining item is user consent, not code. The manual
  step is listed below and cannot be completed by an agent.

The §2b prose gaps (Spline IK, IK Angle/Point/Orient/Parent constraints,
stiffness) were tracked by G38 rather than as checkboxes; they are implemented
and now recorded as checked bullets in §2b.

## Manual item

- Approve the project-scoped MCP server in an interactive Claude Code session
  and confirm `/mcp` lists the LeopardCraft tools. This needs the user's
  consent, so no lane packet can close it.

## Dispatch rules

- A mission is startable only when every dependency in the index is complete.
- Shared contracts (`scene.rs`, `geometry.rs`, `eval.rs`, `undo.rs`, `format.rs`,
  the `AnimChannel` registry and the MCP inventory) land before their consumers,
  with a legacy-load or round-trip test.
- Keep the G29 batch discipline: one lane check per packet, full
  `scripts/check.sh` once after integrating a mission, no commit unless asked.
