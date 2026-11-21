//! Native scene file format, validation and defensive repair.
//!
//! `.lcscene` files are pretty-printed JSON with an explicit format name and
//! version. Loading validates structure, references and topology before the
//! document reaches the editor, and repairs recoverable inconsistencies so a
//! damaged file cannot crash the application.

use crate::{Document, NodeId, NodeKind, FORMAT_NAME, FORMAT_VERSION};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

thread_local! {
    /// Non-fatal scene warnings for the current thread. The editor drains this
    /// after startup, save, open and autosave operations so failures are visible
    /// in the status bar and Console instead of only on stderr.
    static WARNINGS: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
thread_local! {
    /// Test-only fault injection: fail the next atomic write after this many
    /// bytes. Compiled out of normal builds, so the production path has no
    /// fault hook.
    static FAIL_NEXT_WRITE_AFTER: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
    /// Test-only simulation of a process crash: leave the fully or partially
    /// staged temporary file behind instead of running the error cleanup.
    static ABANDON_NEXT_WRITE_AFTER: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(all(test, unix))]
thread_local! {
    static FAIL_NEXT_DIRECTORY_SYNC: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn fail_next_write_after(bytes: usize) {
    FAIL_NEXT_WRITE_AFTER.with(|cell| cell.set(Some(bytes)));
}

#[cfg(test)]
fn abandon_next_write_after(bytes: usize) {
    ABANDON_NEXT_WRITE_AFTER.with(|cell| cell.set(Some(bytes)));
}

/// Drain non-fatal scene warnings emitted on this thread.
pub fn take_warnings() -> Vec<String> {
    WARNINGS.with(|warnings| std::mem::take(&mut *warnings.borrow_mut()))
}

#[cfg(all(test, unix))]
fn fail_next_directory_sync() {
    FAIL_NEXT_DIRECTORY_SYNC.with(|cell| cell.set(true));
}

fn warn(message: &str) {
    const MAX_WARNINGS: usize = 64;
    WARNINGS.with(|warnings| {
        let mut warnings = warnings.borrow_mut();
        if warnings.len() >= MAX_WARNINGS {
            warnings.remove(0);
        }
        warnings.push(message.to_string());
    });
    #[cfg(not(test))]
    eprintln!("warning: {message}");
}

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
    let path = path.as_ref();
    let _ = recover_interrupted_save(path);
    let text = to_json(doc)?;
    write_atomic(path, text.as_bytes())
}

/// Remove scene-staging files left behind by an interrupted save of `path`.
///
/// Native saves replace the target with one atomic rename, so no backup file
/// is needed. A crash before that rename can only leave a hidden temporary
/// sibling; removing it restores the pre-save directory state.
pub fn recover_interrupted_save(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    let result = recover_interrupted_save_inner(path);
    if let Err(error) = &result {
        warn(&format!("failed to reconcile {}: {error}", path.display()));
    }
    result
}

fn recover_interrupted_save_inner(path: &Path) -> Result<(), String> {
    let mut errors = Vec::new();
    for temp in temp_artifacts(path)? {
        match std::fs::remove_file(&temp) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => errors.push(format!(
                "failed to remove stale scene staging file {}: {error}",
                temp.display()
            )),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Reconcile every interrupted `.lcscene` save represented in `dir`.
/// Returns the number of scenes whose staging files were removed. Failures for
/// individual scenes are non-fatal warnings; only a directory scan can fail.
pub fn recover_interrupted_saves(dir: impl AsRef<Path>) -> Result<usize, String> {
    let dir = dir.as_ref();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(format!(
                "failed to scan {} for interrupted scene saves: {error}",
                dir.display()
            ))
        }
    };

    let mut targets = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to scan {} for interrupted scene saves: {error}",
                dir.display()
            )
        })?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let Some(target_name) = interrupted_scene_target(&file_name) else {
            continue;
        };
        targets.insert(dir.join(target_name));
    }

    let mut recovered = 0;
    for target in targets {
        if recover_interrupted_save(&target).is_ok() {
            recovered += 1;
        }
    }
    Ok(recovered)
}

fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

fn temp_artifacts(target: &Path) -> Result<Vec<PathBuf>, String> {
    let parent = parent_dir(target);
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid scene path {}", target.display()))?;
    let prefix = format!(".{name}.tmp-");
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "failed to scan {} for interrupted scene saves: {error}",
                parent.display()
            ))
        }
    };

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to scan {} for interrupted scene saves: {error}",
                parent.display()
            )
        })?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let Some(suffix) = file_name.strip_prefix(prefix.as_str()) else {
            continue;
        };
        if !is_artifact_suffix(suffix) {
            continue;
        }
        paths.push(entry.path());
    }
    paths.sort();
    Ok(paths)
}

fn interrupted_scene_target(file_name: &str) -> Option<&str> {
    let name = file_name.strip_prefix('.')?;
    let (target, suffix) = name.rsplit_once(".tmp-")?;
    if target.is_empty() || !is_artifact_suffix(suffix) {
        return None;
    }
    let extension = Path::new(target).extension()?.to_str()?;
    extension.eq_ignore_ascii_case("lcscene").then_some(target)
}

fn is_artifact_suffix(suffix: &str) -> bool {
    let Some((pid, nonce)) = suffix.split_once('-') else {
        return false;
    };
    pid.parse::<u32>().is_ok() && nonce.parse::<u64>().is_ok()
}

fn temp_sibling(path: &Path, nonce: u64) -> Result<PathBuf, String> {
    let parent = parent_dir(path);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid scene path {}", path.display()))?;
    Ok(parent.join(format!(".{name}.tmp-{}-{nonce}", std::process::id())))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if FAIL_NEXT_DIRECTORY_SYNC.with(|cell| cell.replace(false)) {
        return Err(std::io::Error::other("induced test directory-sync failure"));
    }
    std::fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Write a scene through a sibling temporary file and an atomic rename.
///
/// A crash during the write leaves the previous scene untouched. After the
/// rename, the parent directory is flushed so the new directory entry is
/// durable; a directory-flush failure is a warning because the save itself has
/// already committed. The mode of the existing file is not copied, matching
/// the previous `fs::write` behaviour, but the contents are never left
/// half-written.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    static NEXT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nonce = NEXT_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp = temp_sibling(path, nonce)?;
    let parent = parent_dir(path);
    #[cfg(test)]
    let abandoned = std::cell::Cell::new(false);
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        #[cfg(test)]
        if let Some(limit) = ABANDON_NEXT_WRITE_AFTER.with(|cell| cell.take()) {
            let limit = limit.min(bytes.len());
            file.write_all(&bytes[..limit])?;
            file.sync_all()?;
            abandoned.set(true);
            return Err(std::io::Error::other(format!(
                "simulated crash after {limit} bytes"
            )));
        }
        #[cfg(test)]
        if let Some(limit) = FAIL_NEXT_WRITE_AFTER.with(|cell| cell.take()) {
            let limit = limit.min(bytes.len());
            file.write_all(&bytes[..limit])?;
            file.sync_all()?;
            return Err(std::io::Error::other(format!(
                "induced test failure after {limit} bytes"
            )));
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // `rename` replaces an existing file on all supported platforms; never
        // remove the previous scene first, or a failed rename would lose it.
        std::fs::rename(&temp, path)?;
        if let Err(error) = sync_directory(parent) {
            warn(&format!(
                "saved {} but could not flush its directory entry: {error}",
                path.display()
            ));
        }
        Ok(())
    };
    match write() {
        Ok(()) => Ok(()),
        Err(error) => {
            #[cfg(test)]
            if abandoned.get() {
                return Err(format!("failed to write {}: {error}", path.display()));
            }
            let _ = std::fs::remove_file(&temp);
            Err(format!("failed to write {}: {error}", path.display()))
        }
    }
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

    fn temp_scene_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("leopardcraft-format-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn dir_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn assert_exact_dir_contents(dir: &Path, expected: &[&str]) {
        let mut expected: Vec<String> = expected.iter().map(|name| name.to_string()).collect();
        expected.sort();
        assert_eq!(dir_names(dir), expected);
    }

    fn staging_files(dir: &Path) -> Vec<String> {
        dir_names(dir)
            .into_iter()
            .filter(|name| name.starts_with('.'))
            .collect()
    }

    fn blocking_staging_dir(path: &Path, nonce: u64) -> PathBuf {
        let staging = temp_sibling(path, nonce).unwrap();
        std::fs::create_dir(&staging).unwrap();
        std::fs::write(staging.join("blocker"), b"keep").unwrap();
        staging
    }

    #[test]
    fn atomic_save_replaces_contents_and_cleans_up() {
        let dir = temp_scene_dir("atomic");
        let path = dir.join("scene.lcscene");
        let mut doc = Document::demo();
        save_native(&path, &doc).unwrap();

        let id = *doc.scene.roots.first().unwrap();
        doc.scene.get_mut(id).unwrap().name = "Atomically Renamed".into();
        save_native(&path, &doc).unwrap();

        let restored = load_native(&path).unwrap();
        assert_eq!(restored.signature(), doc.signature());
        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with('.'))
            .collect();
        assert!(
            leftovers.is_empty(),
            "leftover temporary files: {leftovers:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mid_write_failure_preserves_the_previous_scene() {
        let dir = temp_scene_dir("midwrite");
        let path = dir.join("scene.lcscene");
        let original = Document::demo();
        save_native(&path, &original).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();

        let mut changed = original.clone();
        let id = *changed.scene.roots.first().unwrap();
        changed.scene.get_mut(id).unwrap().name = "Interrupted Save".into();

        fail_next_write_after(64);
        assert!(save_native(&path, &changed).is_err());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            original_bytes,
            "a partial write must not touch the previous scene"
        );
        let leftovers = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
            .count();
        assert_eq!(leftovers, 0, "a partial write left a temporary file behind");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn interrupted_first_save_leaves_no_scene_and_next_save_cleans_staging() {
        let dir = temp_scene_dir("crash-first");
        let path = dir.join("scene.lcscene");
        let doc = Document::demo();

        abandon_next_write_after(usize::MAX);
        let error = save_native(&path, &doc).unwrap_err();
        assert!(error.contains("simulated crash"), "{error}");
        assert!(!path.exists(), "an interrupted first save created a target");
        let staging = staging_files(&dir);
        assert_eq!(staging.len(), 1, "{staging:?}");
        assert!(staging[0].starts_with(".scene.lcscene.tmp-"));

        save_native(&path, &doc).unwrap();
        assert_eq!(load_native(&path).unwrap().signature(), doc.signature());
        assert_exact_dir_contents(&dir, &["scene.lcscene"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn interrupted_overwrite_keeps_the_previous_scene_and_next_save_cleans_staging() {
        let dir = temp_scene_dir("crash-overwrite");
        let path = dir.join("scene.lcscene");
        let original = Document::demo();
        save_native(&path, &original).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();

        let mut changed = original.clone();
        let id = *changed.scene.roots.first().unwrap();
        changed.scene.get_mut(id).unwrap().name = "Crash Overwrite".into();

        abandon_next_write_after(usize::MAX);
        let error = save_native(&path, &changed).unwrap_err();
        assert!(error.contains("simulated crash"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(
            load_native(&path).unwrap().signature(),
            original.signature()
        );
        assert_eq!(staging_files(&dir).len(), 1);

        save_native(&path, &changed).unwrap();
        assert_eq!(load_native(&path).unwrap().signature(), changed.signature());
        assert_exact_dir_contents(&dir, &["scene.lcscene"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn startup_recovery_discards_a_fully_written_staging_file() {
        let dir = temp_scene_dir("crash-full-temp");
        let path = dir.join("scene.lcscene");
        let original = Document::demo();
        save_native(&path, &original).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();

        let mut changed = original.clone();
        let id = *changed.scene.roots.first().unwrap();
        changed.scene.get_mut(id).unwrap().name = "Fully Staged".into();
        let changed_json = to_json(&changed).unwrap();

        abandon_next_write_after(usize::MAX);
        assert!(save_native(&path, &changed).is_err());
        let staging = staging_files(&dir);
        assert_eq!(staging.len(), 1, "{staging:?}");
        assert_eq!(
            std::fs::read(dir.join(&staging[0])).unwrap(),
            changed_json.as_bytes(),
            "the simulated crash should leave the complete new scene staged"
        );
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);

        assert_eq!(recover_interrupted_saves(&dir).unwrap(), 1);
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_exact_dir_contents(&dir, &["scene.lcscene"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn startup_recovery_ignores_unrelated_files_and_temps() {
        let dir = temp_scene_dir("crash-startup");
        let first = dir.join("first.lcscene");
        let second = dir.join("second.lcscene");
        let doc = Document::demo();

        abandon_next_write_after(usize::MAX);
        assert!(save_native(&first, &doc).is_err());
        assert!(!first.exists());

        save_native(&second, &doc).unwrap();
        let second_bytes = std::fs::read(&second).unwrap();
        let mut changed = doc.clone();
        let id = *changed.scene.roots.first().unwrap();
        changed.scene.get_mut(id).unwrap().name = "Second Interrupted".into();
        abandon_next_write_after(usize::MAX);
        assert!(save_native(&second, &changed).is_err());

        std::fs::write(dir.join("notes.txt"), b"keep").unwrap();
        std::fs::write(dir.join(".notes.txt.tmp-123-1"), b"keep").unwrap();

        assert_eq!(recover_interrupted_saves(&dir).unwrap(), 2);
        assert!(!first.exists());
        assert_eq!(std::fs::read(&second).unwrap(), second_bytes);
        assert_exact_dir_contents(
            &dir,
            &[".notes.txt.tmp-123-1", "notes.txt", "second.lcscene"],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_staging_cleanup_failure_is_a_warning_and_does_not_block_save() {
        let dir = temp_scene_dir("cleanup-failure-save");
        let path = dir.join("scene.lcscene");
        let stale = blocking_staging_dir(&path, u64::MAX);
        let doc = Document::demo();
        let _ = take_warnings();

        save_native(&path, &doc).unwrap();
        assert_eq!(load_native(&path).unwrap().signature(), doc.signature());
        let warnings = take_warnings();
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("failed to remove stale scene staging file")),
            "expected a staging-cleanup warning, got {warnings:?}"
        );
        assert!(
            stale.is_dir(),
            "the intentionally unremovable artifact vanished"
        );
        let stale_name = stale.file_name().unwrap().to_string_lossy().into_owned();
        assert_exact_dir_contents(&dir, &[stale_name.as_str(), "scene.lcscene"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn startup_sweep_continues_past_cleanup_failure_and_ignores_unrelated() {
        let dir = temp_scene_dir("cleanup-failure-startup");
        let bad = dir.join("bad.lcscene");
        let bad_stale = blocking_staging_dir(&bad, u64::MAX - 1);
        let good = dir.join("good.lcscene");
        let original = Document::demo();
        save_native(&good, &original).unwrap();
        let good_bytes = std::fs::read(&good).unwrap();

        let mut changed = original.clone();
        let id = *changed.scene.roots.first().unwrap();
        changed.scene.get_mut(id).unwrap().name = "Uncommitted".into();
        abandon_next_write_after(usize::MAX);
        assert!(save_native(&good, &changed).is_err());

        std::fs::write(dir.join("notes.txt"), b"keep").unwrap();
        std::fs::write(dir.join(".notes.txt.tmp-123-1"), b"keep").unwrap();

        let _ = take_warnings();
        assert_eq!(recover_interrupted_saves(&dir).unwrap(), 1);
        let warnings = take_warnings();
        assert!(
            warnings.iter().any(|warning| {
                warning.contains("failed to reconcile") && warning.contains("bad.lcscene")
            }),
            "expected a warning for the failing scene, got {warnings:?}"
        );
        assert!(bad_stale.is_dir(), "the failing artifact vanished");
        assert_eq!(std::fs::read(&good).unwrap(), good_bytes);
        let bad_name = bad_stale
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_exact_dir_contents(
            &dir,
            &[
                ".notes.txt.tmp-123-1",
                "notes.txt",
                "good.lcscene",
                bad_name.as_str(),
            ],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn directory_flush_failure_after_commit_is_a_warning_not_a_save_failure() {
        let dir = temp_scene_dir("directory-flush-failure");
        let path = dir.join("scene.lcscene");
        let doc = Document::demo();
        let _ = take_warnings();

        fail_next_directory_sync();
        save_native(&path, &doc).unwrap();
        assert_eq!(load_native(&path).unwrap().signature(), doc.signature());
        let warnings = take_warnings();
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("could not flush its directory entry")),
            "expected a directory-flush warning, got {warnings:?}"
        );
        assert_exact_dir_contents(&dir, &["scene.lcscene"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_atomic_save_does_not_clobber_the_target() {
        let dir = temp_scene_dir("failure");
        let target = dir.join("occupied");
        std::fs::create_dir(&target).unwrap();

        assert!(save_native(&target, &Document::demo()).is_err());
        assert!(target.is_dir());
        let leftovers = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
            .count();
        assert_eq!(leftovers, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
