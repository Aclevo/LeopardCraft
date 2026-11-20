//! Native scene file format, validation and defensive repair.
//!
//! `.lcscene` files are pretty-printed JSON with an explicit format name and
//! version. Loading validates structure, references and topology before the
//! document reaches the editor, and repairs recoverable inconsistencies so a
//! damaged file cannot crash the application.

use crate::{Document, NodeId, NodeKind, FORMAT_NAME, FORMAT_VERSION};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

pub fn to_json(doc: &Document) -> Result<String, String> {
    serde_json::to_string_pretty(doc).map_err(|e| format!("failed to serialise scene: {e}"))
}

pub fn from_json(text: &str) -> Result<Document, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("invalid scene file: {e}"))?;
    if !value.is_object() || value.get("format").is_none() || value.get("scene").is_none() {
        return Err("not a LeopardCraft scene file (missing format/scene)".into());
    }
    let mut doc: Document =
        serde_json::from_value(value).map_err(|e| format!("invalid scene file: {e}"))?;
    if doc.version > FORMAT_VERSION {
        return Err(format!(
            "scene version {} is newer than this build ({}); update LeopardCraft",
            doc.version, FORMAT_VERSION
        ));
    }
    if !doc.format.is_empty() && doc.format != FORMAT_NAME {
        return Err(format!("unrecognised scene format '{}'", doc.format));
    }
    doc.format = FORMAT_NAME.into();
    doc.version = FORMAT_VERSION;
    doc.sanitize();
    doc.validate()
        .map_err(|errors| format!("scene validation failed:\n{}", errors.join("\n")))?;
    Ok(doc)
}

pub fn save_native(path: impl AsRef<Path>, doc: &Document) -> Result<(), String> {
    let text = to_json(doc)?;
    std::fs::write(path.as_ref(), text)
        .map_err(|e| format!("failed to write {}: {e}", path.as_ref().display()))
}

pub fn load_native(path: impl AsRef<Path>) -> Result<Document, String> {
    let text = std::fs::read_to_string(path.as_ref())
        .map_err(|e| format!("failed to read {}: {e}", path.as_ref().display()))?;
    from_json(&text)
}

impl Document {
    /// Structural validation. Returns all errors (bounded) rather than only the
    /// first, which makes imported-file diagnostics actionable.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let push = |errors: &mut Vec<String>, msg: String| {
            if errors.len() < 100 {
                errors.push(msg);
            }
        };
        if self.format != FORMAT_NAME {
            push(&mut errors, format!("unexpected format '{}'", self.format));
        }
        if self.version == 0 || self.version > FORMAT_VERSION {
            push(&mut errors, format!("unsupported version {}", self.version));
        }
        if self.settings.layers.is_empty() {
            push(&mut errors, "document has no layers".into());
        }
        if self.settings.render.width == 0 || self.settings.render.height == 0 {
            push(&mut errors, "render resolution is zero".into());
        }
        if let Some(id) = self.settings.active_camera {
            match self.scene.get(id) {
                Some(n) if matches!(n.kind, NodeKind::Camera { .. }) => {}
                _ => push(&mut errors, format!("active camera {id} is missing")),
            }
        }

        let nodes = &self.scene.nodes;
        let mut roots = HashSet::new();
        for &root in &self.scene.roots {
            if !roots.insert(root) {
                push(&mut errors, format!("root {root} listed twice"));
            }
            match nodes.get(&root) {
                Some(n) if n.parent.is_none() => {}
                Some(_) => push(&mut errors, format!("root {root} has a parent")),
                None => push(&mut errors, format!("root {root} does not exist")),
            }
        }
        for (&id, node) in nodes {
            if node.id != id {
                push(
                    &mut errors,
                    format!("node map key {id} != node id {}", node.id),
                );
            }
            if node.name.trim().is_empty() {
                push(&mut errors, format!("node {id} has an empty name"));
            }
            if node.layer as usize >= self.settings.layers.len() {
                push(
                    &mut errors,
                    format!("node {id} uses missing layer {}", node.layer),
                );
            }
            if let Some(m) = node.material {
                if self.materials.get(m).is_none() {
                    push(
                        &mut errors,
                        format!("node {id} references missing material {m}"),
                    );
                }
            }
            for (slot, m) in &node.material_slots {
                if self.materials.get(*m).is_none() {
                    push(
                        &mut errors,
                        format!("node {id} slot {slot} references missing material {m}"),
                    );
                }
            }
            if let Some(parent) = node.parent {
                match nodes.get(&parent) {
                    Some(p) => {
                        if !p.children.contains(&id) {
                            push(&mut errors, format!("node {id} parent {parent} omits it"));
                        }
                    }
                    None => push(&mut errors, format!("node {id} parent {parent} is missing")),
                }
            } else if !self.scene.roots.contains(&id) {
                push(&mut errors, format!("parentless node {id} is not a root"));
            }
            for &child in &node.children {
                match nodes.get(&child) {
                    Some(c) => {
                        if c.parent != Some(id) {
                            push(
                                &mut errors,
                                format!("node {id} child {child} has wrong parent {:?}", c.parent),
                            );
                        }
                    }
                    None => push(&mut errors, format!("node {id} child {child} is missing")),
                }
            }
            // Tag references.
            for tag in &node.tags {
                match tag {
                    crate::Tag::LookAt { target, .. }
                    | crate::Tag::CopyLocation { source: target, .. } => {
                        if !nodes.contains_key(target) {
                            push(
                                &mut errors,
                                format!("node {id} tag references missing node {target}"),
                            );
                        }
                    }
                    crate::Tag::MaterialOverride { material } => {
                        if self.materials.get(*material).is_none() {
                            push(
                                &mut errors,
                                format!("node {id} tag references missing material {material}"),
                            );
                        }
                    }
                    _ => {}
                }
            }
            for modifier in &node.modifiers {
                if let crate::Modifier::Boolean { operand, .. } = modifier {
                    if !nodes.contains_key(operand) {
                        push(
                            &mut errors,
                            format!("node {id} boolean references missing node {operand}"),
                        );
                    }
                }
                match modifier {
                    crate::Modifier::Lathe { segments, .. } => {
                        if *segments < 3 {
                            push(&mut errors, format!("node {id} lathe has too few segments"));
                        }
                    }
                    crate::Modifier::Array { count, .. } if *count == 0 => {
                        push(&mut errors, format!("node {id} array count is zero"));
                    }
                    _ => {}
                }
            }
            // Mesh topology.
            if let NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable { mesh, .. },
            } = &node.kind
            {
                if let Err(mesh_errors) = mesh.validate() {
                    for e in mesh_errors.into_iter().take(5) {
                        push(&mut errors, format!("node {id} mesh: {e}"));
                    }
                }
            }
            if let Some(skin) = &node.skin {
                if skin.inverse_bind.len() != skin.joints.len() {
                    push(
                        &mut errors,
                        format!(
                            "node {id} skin has {} joints but {} inverse bind matrices",
                            skin.joints.len(),
                            skin.inverse_bind.len()
                        ),
                    );
                }
                for joint in &skin.joints {
                    if !nodes.contains_key(joint) {
                        push(
                            &mut errors,
                            format!("node {id} skin joint {joint} is missing"),
                        );
                    }
                }
            }
            // Animation curves must be finite and sorted.
            for track in &node.animation.tracks {
                let mut last = f32::NEG_INFINITY;
                for key in &track.curve.keys {
                    if !key.time.is_finite() || !key.value.is_finite() {
                        push(
                            &mut errors,
                            format!("node {id} has a non-finite animation key"),
                        );
                        break;
                    }
                    if key.time < last {
                        push(
                            &mut errors,
                            format!("node {id} animation keys are not sorted"),
                        );
                        break;
                    }
                    last = key.time;
                }
            }
        }

        // Cycle detection and reachability.
        for (&id, _) in nodes {
            let mut seen = HashSet::new();
            let mut current = Some(id);
            while let Some(c) = current {
                if !seen.insert(c) {
                    push(&mut errors, format!("hierarchy cycle involves node {id}"));
                    break;
                }
                current = nodes.get(&c).and_then(|n| n.parent);
            }
        }
        let mut reachable = HashSet::new();
        let mut stack: Vec<NodeId> = self.scene.roots.clone();
        while let Some(id) = stack.pop() {
            if !reachable.insert(id) {
                continue;
            }
            if let Some(n) = nodes.get(&id) {
                stack.extend(n.children.iter().copied());
            }
        }
        if reachable.len() != nodes.len() {
            push(
                &mut errors,
                format!(
                    "{} node(s) are unreachable from the scene roots",
                    nodes.len().saturating_sub(reachable.len())
                ),
            );
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Repair recoverable inconsistencies so the editor can open files even if
    /// they were hand-edited or written by an interrupted save.
    pub fn sanitize(&mut self) {
        self.format = FORMAT_NAME.into();
        self.version = FORMAT_VERSION;
        self.settings.ensure_valid_layers();
        let layer_count = self.settings.layers.len() as u8;
        let scene = &mut self.scene;
        scene.next_id = scene
            .nodes
            .keys()
            .next_back()
            .map(|id| id + 1)
            .unwrap_or(1)
            .max(scene.next_id)
            .max(1);

        let node_ids: BTreeSet<NodeId> = scene.nodes.keys().copied().collect();
        let material_ids: BTreeSet<u32> = self.materials.materials.keys().copied().collect();
        let valid_materials = material_ids;

        for (&key, node) in scene.nodes.iter_mut() {
            node.id = key;
            if node.name.trim().is_empty() {
                node.name = format!("Object {key}");
            }
            if node.layer >= layer_count {
                node.layer = 0;
            }
            if let Some(m) = node.material {
                if !valid_materials.contains(&m) {
                    node.material = None;
                }
            }
            node.material_slots
                .retain(|_, m| valid_materials.contains(m));
            node.tags.retain(|tag| match tag {
                crate::Tag::LookAt { target, .. }
                | crate::Tag::CopyLocation { source: target, .. } => node_ids.contains(target),
                crate::Tag::MaterialOverride { material } => valid_materials.contains(material),
                _ => true,
            });
            node.modifiers.retain(|m| match m {
                crate::Modifier::Boolean { operand, .. } => node_ids.contains(operand),
                _ => true,
            });
            sanitize_animation(&mut node.animation);
            if let NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable { mesh, .. },
            } = &mut node.kind
            {
                repair_mesh(mesh);
            }
            if let Some(skin) = &mut node.skin {
                if skin.inverse_bind.len() != skin.joints.len()
                    || skin.joints.iter().any(|j| !node_ids.contains(j))
                {
                    node.skin = None;
                } else {
                    for w in &mut skin.weights {
                        for x in w.iter_mut() {
                            if !x.is_finite() || *x < 0.0 {
                                *x = 0.0;
                            }
                        }
                    }
                }
            }
        }

        // Normalise roots and reciprocal child links, preserving children order
        // when the hierarchy is already consistent.
        let mut consistent = true;
        for (&id, node) in &scene.nodes {
            match node.parent {
                Some(p) => {
                    if !scene.nodes.contains_key(&p) {
                        consistent = false;
                    } else if !scene
                        .nodes
                        .get(&p)
                        .map(|n| n.children.contains(&id))
                        .unwrap_or(false)
                    {
                        consistent = false;
                    }
                }
                None => {
                    if !scene.roots.contains(&id) {
                        consistent = false;
                    }
                }
            }
            for child in &node.children {
                if !scene.nodes.contains_key(child) {
                    consistent = false;
                }
            }
        }
        if !consistent {
            let mut parents: std::collections::BTreeMap<NodeId, Option<NodeId>> =
                scene.nodes.iter().map(|(&id, n)| (id, n.parent)).collect();
            // Break parent cycles by detaching the offending node.
            for id in scene.nodes.keys().copied() {
                let mut seen = HashSet::new();
                let mut current = Some(id);
                while let Some(c) = current {
                    if !seen.insert(c) {
                        if let Some(p) = parents.get_mut(&id) {
                            *p = None;
                        }
                        break;
                    }
                    current = parents.get(&c).copied().flatten();
                }
            }
            let detach: Vec<NodeId> = parents
                .iter()
                .filter_map(|(id, parent)| match parent {
                    Some(p) if *p == *id || !scene.nodes.contains_key(p) => Some(*id),
                    _ => None,
                })
                .collect();
            for id in detach {
                parents.insert(id, None);
            }
            let old_root_order = scene.roots.clone();
            let resolved: Vec<(NodeId, Option<NodeId>, Vec<NodeId>)> = scene
                .nodes
                .iter()
                .map(|(&id, _node)| {
                    let parent = parents.get(&id).copied().flatten();
                    (id, parent, Vec::new())
                })
                .collect();
            let mut child_links: Vec<(NodeId, NodeId)> = Vec::new();
            for (id, parent, _) in &resolved {
                if let Some(p) = parent {
                    child_links.push((*p, *id));
                }
            }
            for node in scene.nodes.values_mut() {
                node.children.clear();
            }
            for (parent, child) in child_links {
                if let Some(p) = scene.nodes.get_mut(&parent) {
                    p.children.push(child);
                }
            }
            for (id, parent, _) in resolved {
                if let Some(node) = scene.nodes.get_mut(&id) {
                    node.parent = parent;
                }
            }
            for node in scene.nodes.values_mut() {
                node.children.sort_unstable();
                node.children.dedup();
            }
            scene.roots.clear();
            for id in old_root_order {
                if scene
                    .nodes
                    .get(&id)
                    .map(|n| n.parent.is_none())
                    .unwrap_or(false)
                    && !scene.roots.contains(&id)
                {
                    scene.roots.push(id);
                }
            }
            for (&id, node) in &scene.nodes {
                if node.parent.is_none() && !scene.roots.contains(&id) {
                    scene.roots.push(id);
                }
            }
        }
        if let Some(cam) = self.settings.active_camera {
            if !matches!(
                self.scene.get(cam).map(|n| &n.kind),
                Some(NodeKind::Camera { .. })
            ) {
                self.settings.active_camera = None;
            }
        }
        if self.settings.active_camera.is_none() {
            self.settings.active_camera = self.active_camera_id();
        }
    }
}

fn sanitize_animation(animation: &mut lc_anim::AnimationClip) {
    for track in &mut animation.tracks {
        track
            .curve
            .keys
            .retain(|k| k.time.is_finite() && k.value.is_finite());
        for key in &mut track.curve.keys {
            key.time = key.time.max(0.0);
            if !key.tangent_in.is_finite() {
                key.tangent_in = 0.0;
            }
            if !key.tangent_out.is_finite() {
                key.tangent_out = 0.0;
            }
        }
        track.curve.keys.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        track
            .curve
            .keys
            .dedup_by(|a, b| (a.time - b.time).abs() < 1e-6);
        if !track.curve.keys.is_empty() {
            track.curve.update_tangents();
        }
    }
    animation.tracks.retain(|t| !t.curve.keys.is_empty());
}

fn repair_mesh(mesh: &mut lc_geom::Mesh) {
    for p in &mut mesh.positions {
        if !p.is_finite() {
            *p = glam::Vec3::ZERO;
        }
    }
    for uv in &mut mesh.uvs {
        if !uv.is_finite() {
            *uv = glam::Vec2::ZERO;
        }
    }
    let uv_len = mesh.uvs.len();
    let point_len = mesh.positions.len();
    mesh.polygons.retain_mut(|poly| {
        poly.corners.retain(|c| (c.point as usize) < point_len);
        for c in &mut poly.corners {
            if c.uv as usize >= uv_len {
                c.uv = 0;
            }
        }
        poly.corners.dedup_by_key(|c| c.point);
        while poly.corners.len() > 1
            && poly.corners.first().map(|c| c.point) == poly.corners.last().map(|c| c.point)
        {
            poly.corners.pop();
        }
        let mut distinct = BTreeSet::new();
        for c in &poly.corners {
            distinct.insert(c.point);
        }
        poly.corners.len() >= 3 && distinct.len() >= 3
    });
    mesh.compact_points();
    mesh.remove_degenerate_polygons();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_roundtrips_exactly() {
        let doc = Document::demo();
        let json = to_json(&doc).unwrap();
        let back = from_json(&json).unwrap();
        assert_eq!(doc.signature(), back.signature());
        assert_eq!(doc.scene.len(), back.scene.len());
        assert_eq!(back.scene.roots, doc.scene.roots);
    }

    #[test]
    fn editable_origin_roundtrips_through_native_format() {
        let sphere = crate::Primitive::Sphere {
            radius: 1.0,
            segments: 12,
            rings: 6,
        };
        let mut doc = Document::empty();
        doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable {
                    mesh: sphere.build(),
                    origin: Some(sphere),
                },
            },
            "Converted Sphere",
            None,
        );
        let json = to_json(&doc).unwrap();
        assert!(json.contains("\"origin\""));
        let back = from_json(&json).unwrap();
        let id = back.scene.find_by_name("Converted Sphere").unwrap();
        match &back.scene.get(id).unwrap().kind {
            NodeKind::Mesh { geometry } => {
                assert_eq!(geometry.origin_copied(), Some(sphere));
            }
            _ => panic!("expected a mesh node"),
        }
    }

    #[test]
    fn rejects_newer_versions() {
        let doc = Document::demo();
        let mut json = to_json(&doc).unwrap();
        json = json.replacen("\"version\": 1", "\"version\": 999", 1);
        assert!(from_json(&json).is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(from_json("not json").is_err());
        assert!(from_json("{}").is_err());
    }

    #[test]
    fn repairs_broken_parent_links() {
        let mut doc = Document::demo();
        let id = doc.scene.find_by_name("Sphere").unwrap();
        doc.scene.get_mut(id).unwrap().parent = Some(999_999);
        doc.sanitize();
        assert!(doc.validate().is_ok(), "{:?}", doc.validate().err());
        assert!(doc.scene.get(id).unwrap().parent.is_none());
        assert!(doc.scene.roots.contains(&id));
    }

    #[test]
    fn repairs_dangling_material() {
        let mut doc = Document::demo();
        let id = doc.scene.find_by_name("Torus").unwrap();
        doc.scene.get_mut(id).unwrap().material = Some(7777);
        doc.sanitize();
        assert!(doc.scene.get(id).unwrap().material.is_none());
        assert!(doc.validate().is_ok());
    }

    #[test]
    fn repairs_mesh_indices() {
        let mut mesh = lc_geom::Mesh::new("Broken");
        mesh.positions.push(glam::Vec3::ZERO);
        mesh.positions.push(glam::Vec3::X);
        mesh.positions.push(glam::Vec3::Z);
        mesh.add_face(&[0, 1, 99]);
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable { mesh, origin: None },
            },
            "Broken",
            None,
        );
        doc.sanitize();
        assert!(doc.scene.get(id).is_some());
        assert!(doc.validate().is_ok(), "{:?}", doc.validate().err());
    }
}
