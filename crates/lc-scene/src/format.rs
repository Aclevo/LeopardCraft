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
    // Commit live edits into the active take so the saved file and the live
    // view agree, and mirror the take fps into legacy settings.
    let mut doc = doc.clone();
    doc.store_active_clips();
    doc.settings.fps = doc.active_take_fps();
    serde_json::to_string_pretty(&doc).map_err(|e| format!("failed to serialise scene: {e}"))
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
    // Files written before takes existed have no take list; `load_active_take`
    // migrates their live clips into a default take. Files with takes open on
    // their stored active take instead of a stale live mirror.
    doc.load_active_take();
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
                    | crate::Tag::CopyLocation { source: target, .. }
                    | crate::Tag::IkPoint { target, .. }
                    | crate::Tag::IkAngle { target, .. }
                    | crate::Tag::Orient { target, .. }
                    | crate::Tag::Parent { target, .. } => {
                        if !nodes.contains_key(target) {
                            push(
                                &mut errors,
                                format!("node {id} tag references missing node {target}"),
                            );
                        }
                    }
                    crate::Tag::MaterialOverride { material }
                        if self.materials.get(*material).is_none() =>
                    {
                        push(
                            &mut errors,
                            format!("node {id} tag references missing material {material}"),
                        );
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
        for &id in nodes.keys() {
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
            sanitize_transform(&mut node.transform);
            if node.name.trim().is_empty() {
                node.name = format!("Object {key}");
            }
            if let Some(limits) = &mut node.rotation_limits {
                *limits = limits.sanitized();
            }
            if !matches!(&node.kind, NodeKind::Joint { .. }) {
                node.rotation_limits = None;
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
            let old_tags = std::mem::take(&mut node.tags);
            let old_tag_animations = std::mem::take(&mut node.tag_animations);
            for (old_index, mut tag) in old_tags.into_iter().enumerate() {
                sanitize_tag(&mut tag);
                let valid = match &tag {
                    crate::Tag::LookAt { target, .. }
                    | crate::Tag::CopyLocation { source: target, .. }
                    | crate::Tag::IkPoint { target, .. }
                    | crate::Tag::IkAngle { target, .. }
                    | crate::Tag::Orient { target, .. }
                    | crate::Tag::Parent { target, .. } => node_ids.contains(target),
                    crate::Tag::MaterialOverride { material } => valid_materials.contains(material),
                    _ => true,
                };
                if valid {
                    if let Some(clip) = old_tag_animations.get(&(old_index as u32)) {
                        node.tag_animations
                            .insert(node.tags.len() as u32, clip.clone());
                    }
                    node.tags.push(tag);
                }
            }
            node.modifiers.retain_mut(|modifier| {
                if let crate::Modifier::Boolean { operand, .. } = modifier {
                    if !node_ids.contains(operand) {
                        return false;
                    }
                }
                modifier.sanitize();
                true
            });
            sanitize_animation(&mut node.animation);
            node.tag_animations
                .retain(|index, _| (*index as usize) < node.tags.len());
            for clip in node.tag_animations.values_mut() {
                sanitize_animation(clip);
            }
            if let NodeKind::Mesh { geometry } = &mut node.kind {
                match geometry {
                    crate::GeometrySource::Editable { mesh, origin } => {
                        repair_mesh(mesh);
                        if let Some(origin) = origin {
                            origin.sanitize();
                        }
                    }
                    crate::GeometrySource::Primitive { primitive } => primitive.sanitize(),
                }
            }
            if let NodeKind::Mesh { geometry } = &node.kind {
                let vertex_count = match geometry {
                    crate::GeometrySource::Editable { mesh, .. } => mesh.positions.len(),
                    crate::GeometrySource::Primitive { primitive } => {
                        primitive.build().positions.len()
                    }
                };
                let targets_before = node.morph.targets.len();
                node.morph.sanitize(Some(vertex_count));
                if node.morph.targets.len() != targets_before {
                    // Removing a target shifts the indices used by weight
                    // curves; dropping them is safer than driving the wrong
                    // target.
                    node.animation.morph_weights.clear();
                }
            } else {
                node.morph = Default::default();
            }
            let morph_target_count = node.morph.targets.len();
            node.animation
                .morph_weights
                .retain(|index, _| (*index as usize) < morph_target_count);
            let mesh_vertex_count = match &node.kind {
                NodeKind::Mesh { geometry } => Some(match geometry {
                    crate::GeometrySource::Editable { mesh, .. } => mesh.positions.len(),
                    crate::GeometrySource::Primitive { primitive } => {
                        primitive.build().positions.len()
                    }
                }),
                _ => None,
            };
            if let Some(skin) = &mut node.skin {
                if skin.inverse_bind.len() != skin.joints.len()
                    || skin.joints.is_empty()
                    || skin.joints.iter().any(|j| !node_ids.contains(j))
                {
                    node.skin = None;
                } else if let Some(mesh_vertex_count) = mesh_vertex_count {
                    // Skin arrays index the evaluated mesh. A modifier stack can
                    // change that topology, so only force the editable-mesh count
                    // when there is no stack to change it.
                    let vertex_count = if node.modifiers.is_empty() {
                        mesh_vertex_count
                    } else {
                        skin.vertex_count()
                    };
                    skin.sanitize(vertex_count);
                } else {
                    node.skin = None;
                }
            }
        }

        // Normalise roots and reciprocal child links, preserving children order
        // when the hierarchy is already consistent.
        let mut consistent = true;
        for (&id, node) in &scene.nodes {
            match node.parent {
                Some(p) => {
                    if !scene
                        .nodes
                        .get(&p)
                        .is_some_and(|n| n.children.contains(&id))
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

/// Repair tag values that evaluation cannot represent. A reversed
/// `ClampRotation` range is representable in JSON and would make `f32::clamp`
/// panic during evaluation, so the bounds are ordered here.
fn sanitize_tag(tag: &mut crate::Tag) {
    if let crate::Tag::ClampRotation { min, max } = tag {
        let (low, high) = crate::eval::normalized_clamp_bounds(*min, *max);
        *min = low;
        *max = high;
    }
    match tag {
        crate::Tag::IkPoint { weight, .. }
        | crate::Tag::Orient { weight, .. }
        | crate::Tag::Parent { weight, .. } => {
            *weight = if weight.is_finite() {
                weight.clamp(0.0, 1.0)
            } else {
                1.0
            };
        }
        crate::Tag::IkAngle { max_degrees, .. } => {
            *max_degrees = if max_degrees.is_finite() {
                max_degrees.clamp(0.0, 180.0)
            } else {
                45.0
            };
        }
        _ => {}
    }
}

/// Repair one scalar curve so sampling cannot see a non-finite or unsorted
/// key. Used for addressed tracks, morph-weight curves and the legacy Euler
/// rotation curves kept for pre-address files.
fn sanitize_curve(curve: &mut lc_anim::Curve) {
    curve
        .keys
        .retain(|k| k.time.is_finite() && k.value.is_finite());
    for key in &mut curve.keys {
        key.time = key.time.max(0.0);
        if !key.tangent_in.is_finite() {
            key.tangent_in = 0.0;
        }
        if !key.tangent_out.is_finite() {
            key.tangent_out = 0.0;
        }
        // Pre-tangent-type files deserialize the new fields as Spline. A
        // non-manual Smooth key used Catmull-Rom before those fields existed,
        // so restore that automatic rule instead of silently changing an old
        // scene to Spline's secant fallback.
        if !key.tangent_manual
            && key.interpolation == lc_anim::Interpolation::Smooth
            && key.tangent_in_type == lc_anim::TangentType::Spline
            && key.tangent_out_type == lc_anim::TangentType::Spline
        {
            key.tangent_in_type = lc_anim::TangentType::Smooth;
            key.tangent_out_type = lc_anim::TangentType::Smooth;
        }
    }
    curve.keys.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    curve.keys.dedup_by(|a, b| (a.time - b.time).abs() < 1e-6);
    if !curve.keys.is_empty() {
        curve.update_tangents();
    }
}

pub(crate) fn sanitize_animation(animation: &mut lc_anim::AnimationClip) {
    for track in &mut animation.tracks {
        sanitize_curve(&mut track.curve);
    }
    animation.tracks.retain(|t| !t.curve.keys.is_empty());
    for curve in animation.morph_weights.values_mut() {
        sanitize_curve(curve);
    }
    animation
        .morph_weights
        .retain(|_, curve| !curve.keys.is_empty());
    for curve in animation.legacy_euler_rotation.values_mut() {
        sanitize_curve(curve);
    }
    animation
        .legacy_euler_rotation
        .retain(|_, curve| !curve.keys.is_empty());
    if let Some(rotation) = animation.rotation.as_mut() {
        // Quaternion keys need the same repair as scalar keys: sampling binary
        // searches the track, so non-finite or unsorted keys are dropped,
        // clamped and ordered here instead of producing a NaN pose.
        rotation
            .keys
            .retain(|key| key.time.is_finite() && key.value.is_finite());
        for key in &mut rotation.keys {
            key.time = key.time.max(0.0);
            // The same legacy rule applies to quaternion keys: old Smooth
            // rotations used the automatic Catmull-Rom cubic control, while a
            // missing new field deserializes as Spline.
            if key.interpolation == lc_anim::Interpolation::Smooth
                && key.tangent_in_type == lc_anim::TangentType::Spline
                && key.tangent_out_type == lc_anim::TangentType::Spline
            {
                key.tangent_in_type = lc_anim::TangentType::Smooth;
                key.tangent_out_type = lc_anim::TangentType::Smooth;
            }
        }
        rotation.keys.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        rotation
            .keys
            .dedup_by(|a, b| (a.time - b.time).abs() < 1e-6);
        if rotation.keys.is_empty() {
            animation.rotation = None;
        }
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

/// Repair transform values that cannot be represented. A JSON number beyond
/// f32's range deserializes as infinity, and a zero or non-finite quaternion
/// cannot build a rotation matrix, so both are replaced with a neutral value
/// before evaluation. Merely large finite values are left alone.
fn sanitize_transform(transform: &mut lc_core::Transform) {
    transform.translation = glam::Vec3::new(
        finite_or(transform.translation.x, 0.0),
        finite_or(transform.translation.y, 0.0),
        finite_or(transform.translation.z, 0.0),
    );
    transform.scale = glam::Vec3::new(
        finite_or(transform.scale.x, 1.0),
        finite_or(transform.scale.y, 1.0),
        finite_or(transform.scale.z, 1.0),
    );
    let length_squared = transform.rotation.length_squared();
    if !length_squared.is_finite() || length_squared <= f32::EPSILON {
        transform.rotation = glam::Quat::IDENTITY;
    } else if (length_squared - 1.0).abs() > 1e-5 {
        transform.rotation = transform.rotation.normalize();
    }
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
    // Additional sets are parallel to `uvs`: truncate or zero-pad a damaged
    // set so every corner slot resolves in every set, and repair non-finite
    // coordinates the same way UV0 is repaired.
    let uv_len = mesh.uvs.len();
    for set in &mut mesh.extra_uvs {
        set.resize(uv_len, glam::Vec2::ZERO);
        for uv in set.iter_mut() {
            if !uv.is_finite() {
                *uv = glam::Vec2::ZERO;
            }
        }
    }
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
    mesh.normalize_creases();
    mesh.normalize_seams();
    mesh.normalize_pinned_uvs();
}

#[cfg(test)]
mod tests {
    use super::*;
    use lc_anim::{AnimChannel, ChannelAddress, ChannelTarget};

    #[test]
    fn non_finite_and_degenerate_transforms_are_repaired_on_load() {
        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "Broken", None);
        {
            let transform = &mut doc.scene.get_mut(id).unwrap().transform;
            transform.translation = glam::Vec3::new(f32::INFINITY, 2.0, f32::NAN);
            transform.rotation = glam::Quat::from_xyzw(0.0, 0.0, 0.0, 2.0);
            transform.scale = glam::Vec3::new(f32::NAN, 3.0, f32::NEG_INFINITY);
        }

        doc.sanitize();

        let transform = doc.scene.get(id).unwrap().transform;
        assert_eq!(transform.translation, glam::Vec3::new(0.0, 2.0, 0.0));
        assert_eq!(transform.scale, glam::Vec3::new(1.0, 3.0, 1.0));
        assert!(
            (transform.rotation.length() - 1.0).abs() < 1e-5,
            "rotation {:?}",
            transform.rotation
        );
        assert!(doc.evaluate(0.0).node(id).unwrap().world.is_finite());
    }

    fn primitive_variants() -> Vec<crate::Primitive> {
        use crate::Primitive;
        vec![
            Primitive::Box {
                width: 1.0,
                height: 2.0,
                depth: 3.0,
            },
            Primitive::Sphere {
                radius: 1.0,
                segments: 8,
                rings: 6,
            },
            Primitive::Cylinder {
                radius: 1.0,
                height: 2.0,
                segments: 8,
            },
            Primitive::Capsule {
                radius: 1.0,
                height: 2.0,
                segments: 8,
                rings: 4,
            },
            Primitive::Disc {
                radius: 1.0,
                segments: 8,
            },
            Primitive::Cone {
                radius: 1.0,
                height: 2.0,
                segments: 8,
            },
            Primitive::Torus {
                major_radius: 1.0,
                minor_radius: 0.25,
                segments: 8,
                rings: 6,
            },
            Primitive::Plane {
                width: 2.0,
                depth: 3.0,
                segments_x: 2,
                segments_z: 3,
            },
        ]
    }

    fn primitive_float_fields(primitive: &crate::Primitive) -> &'static [&'static str] {
        use crate::Primitive;
        match primitive {
            Primitive::Box { .. } => &["width", "height", "depth"],
            Primitive::Sphere { .. } => &["radius"],
            Primitive::Cylinder { .. } => &["radius", "height"],
            Primitive::Capsule { .. } => &["radius", "height"],
            Primitive::Disc { .. } => &["radius"],
            Primitive::Cone { .. } => &["radius", "height"],
            Primitive::Torus { .. } => &["major_radius", "minor_radius"],
            Primitive::Plane { .. } => &["width", "depth"],
        }
    }

    fn primitive_floats(primitive: &crate::Primitive) -> Vec<f32> {
        use crate::Primitive;
        match *primitive {
            Primitive::Box {
                width,
                height,
                depth,
            } => vec![width, height, depth],
            Primitive::Sphere { radius, .. } => vec![radius],
            Primitive::Cylinder { radius, height, .. } => vec![radius, height],
            Primitive::Capsule { radius, height, .. } => vec![radius, height],
            Primitive::Disc { radius, .. } => vec![radius],
            Primitive::Cone { radius, height, .. } => vec![radius, height],
            Primitive::Torus {
                major_radius,
                minor_radius,
                ..
            } => vec![major_radius, minor_radius],
            Primitive::Plane { width, depth, .. } => vec![width, depth],
        }
    }

    #[test]
    fn non_finite_primitive_dimensions_never_build_non_finite_geometry_on_load() {
        // `1e39` is a finite JSON number that serde casts to f32 infinity.
        // Every primitive variant must be repaired on load, not just the ones
        // that happen to flow through a mesh-modelling operation later.
        for primitive in primitive_variants() {
            let mut doc = Document::empty();
            let id = doc.append_node(
                NodeKind::Mesh {
                    geometry: crate::GeometrySource::from_primitive(primitive),
                },
                primitive.label(),
                None,
            );
            let mut value = serde_json::to_value(&doc).unwrap();
            {
                let primitive_value =
                    &mut value["scene"]["nodes"][id.to_string()]["kind"]["geometry"]["primitive"];
                assert!(primitive_value.is_object(), "primitive JSON path");
                for field in primitive_float_fields(&primitive) {
                    primitive_value[*field] = serde_json::json!(1e39);
                }
            }
            let loaded = from_json(&value.to_string()).expect("the crafted scene loads");

            let evaluated = loaded.evaluate(0.0);
            let mesh = evaluated
                .node(id)
                .and_then(|node| node.mesh())
                .cloned()
                .expect("the primitive evaluates");
            assert!(
                mesh.positions.iter().all(|position| position.is_finite()),
                "{} built non-finite positions from a crafted dimension: {:?}",
                primitive.label(),
                mesh.positions
            );
            assert!(mesh.validate().is_ok(), "{}", primitive.label());
            assert!(
                evaluated.bounds.min.is_finite() && evaluated.bounds.max.is_finite(),
                "{} produced non-finite scene bounds: {:?}",
                primitive.label(),
                evaluated.bounds
            );

            let stored = match &loaded.scene.get(id).unwrap().kind {
                NodeKind::Mesh { geometry } => geometry
                    .primitive()
                    .copied()
                    .expect("the node is still a primitive"),
                _ => panic!("the node kind changed on load"),
            };
            assert!(
                primitive_floats(&stored)
                    .iter()
                    .all(|value| value.is_finite()),
                "{} kept a non-finite dimension after load: {:?}",
                primitive.label(),
                primitive_floats(&stored)
            );
        }
    }

    #[test]
    fn non_finite_modifier_parameters_are_repaired_on_load() {
        // `1e39` is a finite JSON number but overflows f32 to infinity. A
        // modifier parameter is not mesh geometry, so `repair_mesh` never sees
        // it; the load sanitize pass must repair it before evaluation.
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::from_primitive(crate::Primitive::Box {
                    width: 1.0,
                    height: 1.0,
                    depth: 1.0,
                }),
            },
            "Extruded Box",
            None,
        );
        doc.scene
            .get_mut(id)
            .unwrap()
            .modifiers
            .push(crate::Modifier::ExtrudeFaces {
                distance: 0.5,
                individual: false,
                faces: Vec::new(),
            });

        let mut value = serde_json::to_value(&doc).unwrap();
        value["scene"]["nodes"][id.to_string()]["modifiers"][0]["distance"] =
            serde_json::json!(1e39);
        let loaded = from_json(&value.to_string()).unwrap();

        let mesh = loaded
            .evaluate(0.0)
            .node(id)
            .and_then(|node| node.mesh())
            .cloned()
            .expect("the box evaluates");
        assert!(
            mesh.positions.iter().all(|position| position.is_finite()),
            "a loaded Extrude distance overflowed evaluated positions: {:?}",
            mesh.positions
        );
        assert!(mesh.validate().is_ok());
        assert!(
            matches!(
                loaded.scene.get(id).unwrap().modifiers.as_slice(),
                [crate::Modifier::ExtrudeFaces { distance, .. }] if distance.is_finite()
            ),
            "the load sanitize pass left the modifier parameter non-finite"
        );
    }

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
    fn a_spot_light_roundtrips_through_native_format() {
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Light {
                light: crate::Light {
                    kind: crate::LightKind::Spot {
                        radius: 7.5,
                        inner_deg: 20.0,
                        outer_deg: 35.0,
                    },
                    color: glam::Vec3::new(0.2, 1.0, 0.6),
                    intensity: 2.0,
                    cast_shadows: false,
                },
            },
            "Spot",
            None,
        );
        let json = to_json(&doc).unwrap();
        let loaded = from_json(&json).unwrap();
        assert_eq!(
            loaded.scene.get(id).unwrap().kind,
            doc.scene.get(id).unwrap().kind,
            "spot cone, radius or colour changed across a save/load"
        );
    }

    #[test]
    fn creases_and_subdivision_schemes_roundtrip_through_native_format() {
        let mut mesh = lc_geom::box_mesh(2.0, 2.0, 2.0);
        lc_geom::box_projection(&mut mesh, 1.0);
        assert!(mesh.set_crease(0, 1, true));
        assert!(mesh.set_crease(1, 2, true));
        assert!(mesh.set_crease_weight(2, 3, 0.25));
        assert!(mesh.set_seam(2, 3, true));
        assert!(mesh.set_seam(3, 0, true));
        assert!(mesh.set_uv_pinned(2, true));
        assert!(mesh.set_uv_pinned(5, true));
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable { mesh, origin: None },
            },
            "Creased Mesh",
            None,
        );
        doc.scene
            .get_mut(id)
            .unwrap()
            .modifiers
            .push(crate::Modifier::Subdivision {
                levels: 3,
                scheme: crate::SubdivisionScheme::StamLoop,
            });
        let json = to_json(&doc).unwrap();
        assert!(json.contains("\"creases\""));
        assert!(json.contains("\"crease_weights\""));
        assert!(json.contains("\"seams\""));
        assert!(json.contains("\"pinned_uvs\""));
        assert!(json.contains("stam_loop"));
        let back = from_json(&json).unwrap();
        assert_eq!(doc.signature(), back.signature());
        let node = back.scene.get(id).unwrap();
        assert!(matches!(
            node.modifiers.first(),
            Some(crate::Modifier::Subdivision {
                levels: 3,
                scheme: crate::SubdivisionScheme::StamLoop
            })
        ));
        let NodeKind::Mesh {
            geometry: crate::GeometrySource::Editable { mesh, .. },
        } = &node.kind
        else {
            panic!("expected an editable mesh");
        };
        assert!(mesh.is_crease(0, 1));
        assert!(mesh.is_crease(1, 2));
        assert!((mesh.crease_weight(2, 3) - 0.25).abs() < 1e-6);
        assert!(mesh.is_seam(2, 3));
        assert!(mesh.is_seam(3, 0));
        assert!(mesh.is_uv_pinned(2));
        assert!(mesh.is_uv_pinned(5));
    }

    #[test]
    fn additional_uv_sets_roundtrip_and_damaged_sets_are_repaired_on_load() {
        let mut mesh = lc_geom::box_mesh(2.0, 2.0, 2.0);
        lc_geom::box_projection(&mut mesh, 1.0);
        assert_eq!(mesh.add_uv_set(), Some(1));
        assert!(mesh.set_uv_value(1, 0, glam::Vec2::new(0.25, 0.75)));
        let slots = mesh.uvs.len();
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable { mesh, origin: None },
            },
            "Two UV Sets",
            None,
        );
        let json = to_json(&doc).unwrap();
        assert!(json.contains("extra_uvs"), "additional sets are saved");
        let back = from_json(&json).unwrap();
        assert_eq!(doc.signature(), back.signature());
        let NodeKind::Mesh { geometry } = &back.scene.get(id).unwrap().kind else {
            panic!("expected a mesh node");
        };
        let crate::GeometrySource::Editable { mesh, .. } = geometry else {
            panic!("expected an editable mesh");
        };
        assert_eq!(mesh.uv_set_count(), 2);
        assert_eq!(mesh.uv_value(1, 0), Some(glam::Vec2::new(0.25, 0.75)));
        assert_eq!(mesh.extra_uvs[0].len(), slots);
        assert!(mesh.validate().is_ok());

        // A short, non-parallel set is repaired at the load boundary.
        let mut damaged: serde_json::Value = serde_json::from_str(&json).unwrap();
        fn shorten_extra_sets(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    if let Some(serde_json::Value::Array(sets)) = map.get_mut("extra_uvs") {
                        if let Some(serde_json::Value::Array(first)) = sets.first_mut() {
                            first.truncate(2);
                        }
                    }
                    for child in map.values_mut() {
                        shorten_extra_sets(child);
                    }
                }
                serde_json::Value::Array(items) => {
                    for child in items {
                        shorten_extra_sets(child);
                    }
                }
                _ => {}
            }
        }
        shorten_extra_sets(&mut damaged);
        let repaired = from_json(&damaged.to_string()).unwrap();
        let NodeKind::Mesh { geometry } = &repaired.scene.get(id).unwrap().kind else {
            panic!("expected a mesh node");
        };
        let crate::GeometrySource::Editable { mesh, .. } = geometry else {
            panic!("expected an editable mesh");
        };
        assert_eq!(mesh.extra_uvs[0].len(), slots, "sets are re-padded");
        assert_eq!(mesh.extra_uvs[0][3], glam::Vec2::ZERO);
        assert!(mesh.validate().is_ok());

        // Old files without the field load as one UV0 set.
        let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
        fn strip_extra_sets(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    map.remove("extra_uvs");
                    for child in map.values_mut() {
                        strip_extra_sets(child);
                    }
                }
                serde_json::Value::Array(items) => {
                    for child in items {
                        strip_extra_sets(child);
                    }
                }
                _ => {}
            }
        }
        strip_extra_sets(&mut legacy);
        let old = from_json(&legacy.to_string()).unwrap();
        let NodeKind::Mesh { geometry } = &old.scene.get(id).unwrap().kind else {
            panic!("expected a mesh node");
        };
        let crate::GeometrySource::Editable { mesh, .. } = geometry else {
            panic!("expected an editable mesh");
        };
        assert_eq!(mesh.uv_set_count(), 1);
        assert!(mesh.extra_uvs.is_empty());
    }

    #[test]
    fn a_filled_hole_roundtrips_through_the_native_format() {
        let mut mesh = lc_geom::box_mesh(2.0, 2.0, 2.0);
        lc_geom::box_projection(&mut mesh, 1.0);
        mesh.polygons.remove(0);
        let border = mesh.boundary_loops()[0].clone();
        assert!(mesh.set_crease(border[0], border[1], true));
        assert!(mesh.set_seam(border[1], border[2], true));
        assert!(mesh.set_uv_pinned(0, true));
        let holes = lc_geom::open_holes(&mesh, &border[..1]);
        let filled = lc_geom::fill_holes(&mesh, &holes).unwrap();
        assert!(filled.boundary_loops().is_empty());

        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable {
                    mesh: filled,
                    origin: None,
                },
            },
            "Filled Hole",
            None,
        );
        let json = to_json(&doc).unwrap();
        let back = from_json(&json).unwrap();
        assert_eq!(doc.signature(), back.signature());
        let NodeKind::Mesh { geometry } = &back.scene.get(id).unwrap().kind else {
            panic!("expected a mesh node");
        };
        let crate::GeometrySource::Editable { mesh, .. } = geometry else {
            panic!("expected an editable mesh");
        };
        assert_eq!(mesh.polygons.len(), 6);
        assert!(
            mesh.boundary_loops().is_empty(),
            "the filled border survives loading"
        );
        assert_eq!(mesh.creases.len(), 1);
        assert_eq!(mesh.seams.len(), 1);
        assert_eq!(mesh.pinned_uvs, vec![0]);
    }

    #[test]
    fn a_shelled_mesh_roundtrips_with_a_split_part_marks_and_a_filled_hole() {
        let mut mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        lc_geom::box_projection(&mut mesh, 1.0);
        let removed = mesh.polygons.remove(0);
        let border: Vec<u32> = removed.corners.iter().map(|c| c.point).collect();
        let holes = lc_geom::open_holes(&mesh, &border[..1]);
        mesh = lc_geom::fill_holes(&mesh, &holes).unwrap();
        mesh.toggle_crease(border[0], border[1]);
        mesh.toggle_seam(border[1], border[2]);
        mesh.set_uv_pinned(mesh.polygons[0].corners[0].uv, true);

        let faces: Vec<u32> = (0..mesh.polygons.len() as u32).collect();
        let shelled = lc_geom::shell_faces(&mesh, &faces, 0.1, true, 30.0, 0.0).unwrap();
        assert_eq!(shelled.polygons.len(), 12);
        let part = lc_geom::extract_faces(&shelled, &[0]).unwrap();

        let mut doc = Document::empty();
        let source = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable {
                    mesh: shelled,
                    origin: None,
                },
            },
            "Shelled Source",
            None,
        );
        let split = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable {
                    mesh: part,
                    origin: None,
                },
            },
            "Shelled Source Split",
            None,
        );

        let json = to_json(&doc).unwrap();
        let back = from_json(&json).unwrap();
        assert_eq!(doc.signature(), back.signature());

        fn editable_mesh_of(doc: &Document, id: crate::NodeId) -> lc_geom::Mesh {
            let NodeKind::Mesh { geometry } = &doc.scene.get(id).expect("node").kind else {
                panic!("expected a mesh node");
            };
            let crate::GeometrySource::Editable { mesh, .. } = geometry else {
                panic!("expected an editable mesh");
            };
            mesh.clone()
        }

        let source_mesh = editable_mesh_of(&back, source);
        assert_eq!(source_mesh.polygons.len(), 12, "the shell survives loading");
        assert_eq!(source_mesh.positions.len(), 16);
        assert!(
            source_mesh.boundary_loops().is_empty(),
            "the filled hole and the shell stay closed"
        );
        assert_eq!(source_mesh.creases.len(), 1);
        assert_eq!(source_mesh.seams.len(), 1);
        assert_eq!(source_mesh.pinned_uvs.len(), 1);

        let part_mesh = editable_mesh_of(&back, split);
        assert_eq!(part_mesh.polygons.len(), 1);
        assert!(part_mesh.validate().is_ok());
    }

    #[test]
    fn a_split_part_roundtrips_beside_the_filled_source_mesh() {
        let mut mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        lc_geom::box_projection(&mut mesh, 1.0);
        let removed = mesh.polygons.remove(0);
        let border: Vec<u32> = removed.corners.iter().map(|c| c.point).collect();
        let holes = lc_geom::open_holes(&mesh, &border[..1]);
        let mut filled = lc_geom::fill_holes(&mesh, &holes).unwrap();
        filled.toggle_crease(border[0], border[1]);
        filled.toggle_seam(border[1], border[2]);
        let filled_face = filled.polygons.len() - 1;
        filled.set_uv_pinned(filled.polygons[filled_face].corners[0].uv, true);
        let part = lc_geom::extract_faces(&filled, &[filled_face as u32]).unwrap();
        assert_eq!(part.polygons.len(), 1);
        assert_eq!(part.positions.len(), border.len());

        let mut doc = Document::empty();
        let source = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable {
                    mesh: filled,
                    origin: None,
                },
            },
            "Filled Source",
            None,
        );
        let split = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable {
                    mesh: part,
                    origin: None,
                },
            },
            "Filled Source Split",
            None,
        );
        doc.scene
            .get_mut(split)
            .unwrap()
            .tags
            .push(crate::Tag::RenderOverride {
                visible: true,
                casts_shadow: true,
            });

        let json = to_json(&doc).unwrap();
        let back = from_json(&json).unwrap();
        assert_eq!(doc.signature(), back.signature());

        fn editable_mesh_of(doc: &Document, id: crate::NodeId) -> lc_geom::Mesh {
            let NodeKind::Mesh { geometry } = &doc.scene.get(id).expect("node").kind else {
                panic!("expected a mesh node");
            };
            let crate::GeometrySource::Editable { mesh, .. } = geometry else {
                panic!("expected an editable mesh");
            };
            mesh.clone()
        }

        let source_mesh = editable_mesh_of(&back, source);
        assert_eq!(
            source_mesh.polygons.len(),
            6,
            "the source keeps every polygon"
        );
        assert!(source_mesh.boundary_loops().is_empty());
        assert_eq!(source_mesh.creases.len(), 1);
        assert_eq!(source_mesh.seams.len(), 1);
        assert_eq!(source_mesh.pinned_uvs.len(), 1);

        let part_mesh = editable_mesh_of(&back, split);
        assert_eq!(part_mesh.polygons.len(), 1);
        assert_eq!(part_mesh.boundary_loops().len(), 1);
        assert_eq!(part_mesh.creases.len(), 1);
        assert_eq!(part_mesh.seams.len(), 1);
        assert_eq!(part_mesh.pinned_uvs.len(), 1);
        assert_eq!(back.scene.get(split).unwrap().tags.len(), 1);
    }

    #[test]
    fn legacy_documents_without_creases_default_to_catmull_clark() {
        fn strip_new_subdivision_fields(value: &mut serde_json::Value) {
            if let Some(object) = value.as_object_mut() {
                object.remove("creases");
                object.remove("crease_weights");
                object.remove("seams");
                object.remove("pinned_uvs");
                object.remove("scheme");
                for child in object.values_mut() {
                    strip_new_subdivision_fields(child);
                }
            } else if let Some(array) = value.as_array_mut() {
                for child in array {
                    strip_new_subdivision_fields(child);
                }
            }
        }

        let mesh = lc_geom::box_mesh(2.0, 2.0, 2.0);
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable { mesh, origin: None },
            },
            "Legacy Mesh",
            None,
        );
        doc.scene
            .get_mut(id)
            .unwrap()
            .modifiers
            .push(crate::Modifier::Subdivision {
                levels: 2,
                scheme: crate::SubdivisionScheme::StamLoop,
            });
        let mut legacy: serde_json::Value = serde_json::from_str(&to_json(&doc).unwrap()).unwrap();
        strip_new_subdivision_fields(&mut legacy);
        let back = from_json(&legacy.to_string()).unwrap();
        let node = back.scene.get(id).unwrap();
        assert!(matches!(
            node.modifiers.first(),
            Some(crate::Modifier::Subdivision {
                levels: 2,
                scheme: crate::SubdivisionScheme::CatmullClark
            })
        ));
        let NodeKind::Mesh {
            geometry: crate::GeometrySource::Editable { mesh, .. },
        } = &node.kind
        else {
            panic!("expected an editable mesh");
        };
        assert!(mesh.creases.is_empty());
        assert!(mesh.crease_weights.is_empty());
        assert!(mesh.seams.is_empty());
        assert!(mesh.pinned_uvs.is_empty());
    }

    #[test]
    fn legacy_on_disk_fixture_loads_with_documented_defaults_and_evaluates() {
        // This fixture is checked in as a Wave 1-shaped file: scalar Euler
        // tracks without tangent metadata, no quaternion rotation, morph or
        // tag clips, and a Subdivision modifier written before schemes existed.
        // It must load through the real on-disk entry point, not a clip built
        // from today's enum.
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy_wave1.lcscene");
        let doc = load_native(&path).expect("the legacy fixture must load");

        let animated = doc.scene.get(1).expect("legacy animated node");
        assert!(animated.animation.rotation.is_none());
        assert!(animated.animation.morph_weights.is_empty());
        assert_eq!(animated.animation.tracks.len(), 1);
        assert_eq!(
            animated.animation.tracks[0].address,
            ChannelAddress::owner_component(AnimChannel::Translation, 0),
            "an old property name becomes an owner channel address"
        );
        assert!(animated.animation.legacy_euler_rotation.is_empty());
        let keys = &animated.animation.tracks[0].curve.keys;
        assert_eq!(keys.len(), 2);
        assert!(
            keys.iter().all(|key| !key.tangent_manual),
            "old keys default to automatic tangents"
        );
        assert_eq!(keys[0].tangent_in_type, lc_anim::TangentType::Spline);
        assert_eq!(keys[0].tangent_out_type, lc_anim::TangentType::Spline);
        assert!(animated.tag_animations.is_empty());
        assert!(animated.skin.is_none());
        assert!(animated.morph.is_empty());
        assert!(animated.ik.is_none());
        assert!(animated.rotation_limits.is_none());
        assert!(animated.material_slots.is_empty());
        assert!(!animated.locked);

        assert_eq!(doc.settings.units, "metres");
        assert_eq!(doc.settings.fps, 24.0);
        assert_eq!(doc.settings.layers.len(), 1);
        assert_eq!(doc.settings.render, crate::RenderSettings::default());
        assert!(matches!(
            doc.scene.get(2).unwrap().modifiers.first(),
            Some(crate::Modifier::Subdivision {
                levels: 2,
                scheme: crate::SubdivisionScheme::CatmullClark
            })
        ));

        let evaluated = doc.evaluate(0.5);
        let moved = evaluated.node(1).expect("the animated node evaluates");
        assert!(
            (moved.world.w_axis.x - 0.5).abs() < 1e-4,
            "legacy Euler keys must still animate: {:?}",
            moved.world.w_axis
        );
        assert!(evaluated.node(2).is_some());
    }

    #[test]
    fn manual_tangents_roundtrip_through_native_format() {
        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "Tangent Null", None);
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.animation.set_key(
                ChannelAddress::owner_component(AnimChannel::Translation, 0),
                0.0,
                0.0,
                lc_anim::Interpolation::Smooth,
            );
            node.animation.set_key(
                ChannelAddress::owner_component(AnimChannel::Translation, 0),
                1.0,
                1.0,
                lc_anim::Interpolation::Smooth,
            );
            let curve = &mut node
                .animation
                .track_mut(ChannelAddress::owner_component(AnimChannel::Translation, 0))
                .unwrap()
                .curve;
            assert!(curve.set_manual_tangents(0.0, 0.0, 2.0));
            assert!(curve.set_manual_tangents(1.0, 0.0, 0.0));
        }
        let json = to_json(&doc).unwrap();
        assert!(json.contains("\"tangent_manual\""));
        let back = from_json(&json).unwrap();
        let curve = &back
            .scene
            .get(id)
            .unwrap()
            .animation
            .track(ChannelAddress::owner_component(AnimChannel::Translation, 0))
            .unwrap()
            .curve;
        assert!(curve.keys[0].tangent_manual);
        assert!((curve.keys[0].tangent_out - 2.0).abs() < 1e-5);
        assert!((curve.sample(0.5) - 0.75).abs() < 1e-4);
        assert_eq!(doc.signature(), back.signature());
    }

    #[test]
    fn retimed_animation_roundtrips_through_native_format() {
        let mut mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        mesh.compact_points();
        let base = mesh.positions.clone();
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable { mesh, origin: None },
            },
            "Retimed Mesh",
            None,
        );
        doc.scene.get_mut(id).unwrap().morph = crate::MorphSet {
            base: base.clone(),
            targets: vec![crate::MorphTarget::new(
                "Lift",
                vec![glam::Vec3::Y * 0.25; base.len()],
            )],
        };
        {
            let animation = &mut doc.scene.get_mut(id).unwrap().animation;
            animation.set_key(
                ChannelAddress::owner_component(AnimChannel::Translation, 0),
                1.0,
                0.0,
                lc_anim::Interpolation::Smooth,
            );
            animation.set_key(
                ChannelAddress::owner_component(AnimChannel::Translation, 0),
                3.0,
                1.0,
                lc_anim::Interpolation::Smooth,
            );
            let curve = &mut animation
                .track_mut(ChannelAddress::owner_component(AnimChannel::Translation, 0))
                .unwrap()
                .curve;
            assert!(curve.set_manual_tangents(1.0, 0.0, 2.0));
            assert!(curve.set_manual_tangents(3.0, 0.0, 0.0));
            animation.set_morph_weight_key(0, 1.0, 0.0, lc_anim::Interpolation::Linear);
            animation.set_morph_weight_key(0, 3.0, 1.0, lc_anim::Interpolation::Linear);
            animation.set_rotation_key(
                ChannelTarget::Owner,
                1.0,
                glam::Quat::IDENTITY,
                lc_anim::Interpolation::Linear,
            );
            animation.set_rotation_key(
                ChannelTarget::Owner,
                3.0,
                glam::Quat::from_rotation_y(1.0),
                lc_anim::Interpolation::Linear,
            );
            assert!(animation.retime_all(0.0, 2.0));
        }
        let json = to_json(&doc).unwrap();
        let back = from_json(&json).unwrap();
        let animation = &back.scene.get(id).unwrap().animation;
        let scalar = animation
            .track(ChannelAddress::owner_component(AnimChannel::Translation, 0))
            .unwrap();
        assert!((scalar.curve.keys[0].time - 2.0).abs() < 1e-5);
        assert!((scalar.curve.keys[1].time - 6.0).abs() < 1e-5);
        assert!(scalar.curve.keys[0].tangent_manual);
        assert!((scalar.curve.keys[0].tangent_out - 1.0).abs() < 1e-5);
        assert!((animation.morph_weights.get(&0).unwrap().keys[1].time - 6.0).abs() < 1e-5);
        assert!((animation.rotation.as_ref().unwrap().keys[1].time - 6.0).abs() < 1e-5);
        assert_eq!(doc.signature(), back.signature());
    }

    #[test]
    fn interpolation_and_tangent_types_roundtrip_through_native_format() {
        let mut mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        mesh.compact_points();
        let base = mesh.positions.clone();
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable { mesh, origin: None },
            },
            "Interpolation Mesh",
            None,
        );
        doc.scene.get_mut(id).unwrap().morph = crate::MorphSet {
            base: base.clone(),
            targets: vec![crate::MorphTarget::new(
                "Lift",
                vec![glam::Vec3::Y * 0.25; base.len()],
            )],
        };
        {
            let animation = &mut doc.scene.get_mut(id).unwrap().animation;
            let modes = [
                lc_anim::Interpolation::Constant,
                lc_anim::Interpolation::Linear,
                lc_anim::Interpolation::Smooth,
                lc_anim::Interpolation::Spline,
            ];
            for (index, mode) in modes.into_iter().enumerate() {
                animation.set_key(
                    ChannelAddress::owner_component(AnimChannel::Translation, 0),
                    index as f32,
                    index as f32,
                    mode,
                );
            }
            let curve = &mut animation
                .track_mut(ChannelAddress::owner_component(AnimChannel::Translation, 0))
                .unwrap()
                .curve;
            assert!(curve.set_tangent_type(0.0, lc_anim::TangentType::Smooth, true, true));
            assert!(curve.set_tangent_type(2.0, lc_anim::TangentType::Constant, true, true));
            assert!(curve.set_manual_tangents(3.0, -1.0, 0.0));

            animation.set_morph_weight_key(0, 0.0, 0.0, lc_anim::Interpolation::Linear);
            animation.set_morph_weight_key(0, 3.0, 1.0, lc_anim::Interpolation::Smooth);
            animation.set_rotation_key(
                ChannelTarget::Owner,
                0.0,
                glam::Quat::IDENTITY,
                lc_anim::Interpolation::Constant,
            );
            animation.set_rotation_key(
                ChannelTarget::Owner,
                3.0,
                glam::Quat::from_rotation_y(1.0),
                lc_anim::Interpolation::Spline,
            );
        }
        let before = doc.signature();
        let json = to_json(&doc).unwrap();
        assert!(json.contains("\"tangent_in_type\""));
        assert!(json.contains("\"spline\""));
        let back = from_json(&json).unwrap();
        assert_eq!(before, back.signature());

        let animation = &back.scene.get(id).unwrap().animation;
        let curve = &animation
            .track(ChannelAddress::owner_component(AnimChannel::Translation, 0))
            .unwrap()
            .curve;
        assert_eq!(
            curve.keys[0].interpolation,
            lc_anim::Interpolation::Constant
        );
        assert_eq!(curve.keys[1].interpolation, lc_anim::Interpolation::Linear);
        assert_eq!(curve.keys[2].interpolation, lc_anim::Interpolation::Smooth);
        assert_eq!(curve.keys[3].interpolation, lc_anim::Interpolation::Spline);
        assert_eq!(curve.keys[0].tangent_in_type, lc_anim::TangentType::Smooth);
        assert_eq!(
            curve.keys[2].tangent_out_type,
            lc_anim::TangentType::Constant
        );
        assert!(curve.keys[3].tangent_manual);
        assert_eq!(curve.keys[3].tangent_in_type, lc_anim::TangentType::Spline);
        assert!((curve.sample(0.5) - 0.0).abs() < 1e-5);
        assert!((curve.sample(1.5) - 1.5).abs() < 1e-4);
        assert_eq!(
            animation.morph_weights.get(&0).unwrap().keys[1].interpolation,
            lc_anim::Interpolation::Smooth
        );
        assert_eq!(
            animation.rotation.as_ref().unwrap().keys[0].interpolation,
            lc_anim::Interpolation::Constant
        );
        assert_eq!(
            animation.rotation.as_ref().unwrap().keys[1].interpolation,
            lc_anim::Interpolation::Spline
        );
        let rotation = animation.rotation.as_ref().unwrap();
        assert_eq!(
            rotation.keys[0].tangent_out_type,
            lc_anim::TangentType::Constant
        );
        assert_eq!(
            rotation.keys[1].tangent_in_type,
            lc_anim::TangentType::Spline
        );
    }

    #[test]
    fn legacy_smooth_keys_without_tangent_types_keep_catmull_sampling() {
        fn strip_tangent_types(value: &mut serde_json::Value) {
            if let Some(object) = value.as_object_mut() {
                object.remove("tangent_in_type");
                object.remove("tangent_out_type");
                for child in object.values_mut() {
                    strip_tangent_types(child);
                }
            } else if let Some(array) = value.as_array_mut() {
                for child in array {
                    strip_tangent_types(child);
                }
            }
        }

        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "Legacy Smooth Null", None);
        {
            let animation = &mut doc.scene.get_mut(id).unwrap().animation;
            for (time, value) in [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)] {
                animation.set_key(
                    ChannelAddress::owner_component(AnimChannel::Translation, 0),
                    time,
                    value,
                    lc_anim::Interpolation::Smooth,
                );
                animation.set_rotation_key(
                    ChannelTarget::Owner,
                    time,
                    glam::Quat::from_rotation_y(value * 0.4),
                    lc_anim::Interpolation::Smooth,
                );
            }
        }
        let json = to_json(&doc).unwrap();
        let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
        strip_tangent_types(&mut legacy);
        let back = from_json(&legacy.to_string()).unwrap();
        let curve = &back
            .scene
            .get(id)
            .unwrap()
            .animation
            .track(ChannelAddress::owner_component(AnimChannel::Translation, 0))
            .unwrap()
            .curve;
        assert!(curve.keys.iter().all(|key| {
            key.tangent_in_type == lc_anim::TangentType::Smooth
                && key.tangent_out_type == lc_anim::TangentType::Smooth
        }));
        let sample = curve.sample(0.5);
        assert!(
            (sample - 0.625).abs() < 1e-4,
            "legacy Smooth files must retain their Catmull-Rom shape; sampled {sample}"
        );
        let rotation = back
            .scene
            .get(id)
            .unwrap()
            .animation
            .rotation
            .as_ref()
            .unwrap();
        assert!(rotation.keys.iter().all(|key| {
            key.tangent_in_type == lc_anim::TangentType::Smooth
                && key.tangent_out_type == lc_anim::TangentType::Smooth
        }));
    }

    #[test]
    fn unsorted_and_duplicate_rotation_keys_are_repaired_on_load() {
        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "Rotation Null", None);
        {
            let node = doc.scene.get_mut(id).unwrap();
            let mut track = lc_anim::RotationTrack::default();
            for (time, angle) in [(2.0, 2.0), (1.0, 1.0), (1.0, 3.0), (-1.0, 0.5)] {
                track.keys.push(lc_anim::RotationKey::new(
                    time,
                    glam::Quat::from_rotation_y(angle),
                    lc_anim::Interpolation::Linear,
                ));
            }
            node.animation.rotation = Some(track);
        }

        let back = from_json(&to_json(&doc).unwrap()).unwrap();
        let rotation = back
            .scene
            .get(id)
            .unwrap()
            .animation
            .rotation
            .as_ref()
            .unwrap();
        let times: Vec<f32> = rotation.keys.iter().map(|key| key.time).collect();
        assert_eq!(times, vec![0.0, 1.0, 2.0]);
        assert!(rotation.sample(1.5).is_some());
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
    fn morph_targets_round_trip_and_sanitize_weights() {
        let sphere = crate::Primitive::Sphere {
            radius: 1.0,
            segments: 12,
            rings: 6,
        };
        // The format repairs editable meshes by compacting unused points, so
        // record the base against the same topology a reload will produce.
        let mut mesh = sphere.build();
        mesh.compact_points();
        let base = mesh.positions.clone();
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::Editable {
                    mesh,
                    origin: Some(sphere),
                },
            },
            "Morph Face",
            None,
        );
        let mut target = crate::MorphTarget::new("Smile", vec![glam::Vec3::Y * 0.25; base.len()]);
        target.weight = 2.0;
        doc.scene.get_mut(id).unwrap().morph = crate::MorphSet {
            base: base.clone(),
            targets: vec![target],
        };
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.animation
                .set_morph_weight_key(0, 0.0, 0.0, lc_anim::Interpolation::Linear);
            node.animation
                .set_morph_weight_key(0, 1.0, 0.8, lc_anim::Interpolation::Linear);
        }

        let json = to_json(&doc).unwrap();
        assert!(json.contains("\"morph\""));
        let back = from_json(&json).unwrap();
        let morph = &back.scene.get(id).unwrap().morph;
        assert_eq!(morph.base, base);
        assert_eq!(morph.targets.len(), 1);
        assert_eq!(morph.targets[0].name, "Smile");
        assert_eq!(morph.targets[0].deltas.len(), base.len());
        assert!((morph.targets[0].weight - 1.0).abs() < 1e-6);
        let animation = &back.scene.get(id).unwrap().animation;
        assert!((animation.sample_morph_weight(0, 0.5).unwrap() - 0.4).abs() < 1e-5);
    }

    #[test]
    fn stale_morph_targets_drop_shifted_weight_curves() {
        let mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        let base = mesh.positions.clone();
        let mut doc = Document::empty();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::editable(mesh),
            },
            "Stale Morph",
            None,
        );
        {
            let node = doc.scene.get_mut(id).unwrap();
            node.morph = crate::MorphSet {
                base: base.clone(),
                targets: vec![
                    crate::MorphTarget::new("Kept", vec![glam::Vec3::Y; base.len()]),
                    crate::MorphTarget::new("Stale", vec![glam::Vec3::Z; 1]),
                ],
            };
            node.animation
                .set_morph_weight_key(0, 0.0, 1.0, lc_anim::Interpolation::Linear);
            node.animation
                .set_morph_weight_key(1, 0.0, 1.0, lc_anim::Interpolation::Linear);
        }
        doc.sanitize();
        let node = doc.scene.get(id).unwrap();
        assert_eq!(node.morph.targets.len(), 1);
        assert!(node.animation.morph_weights.is_empty());
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
    fn loading_repairs_skin_lengths_indices_and_weights() {
        let mut doc = Document::empty();
        let joint = doc.append_node(
            NodeKind::Joint {
                length: 0.8,
                width: 0.08,
            },
            "Joint",
            None,
        );
        let mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        let vertex_count = mesh.positions.len();
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: crate::GeometrySource::editable(mesh),
            },
            "Skinned Cube",
            None,
        );
        let mut skin = crate::Skin::new(vec![joint], vec![glam::Mat4::IDENTITY]);
        skin.joint_indices = vec![[0, 7, 0, 0]; vertex_count];
        skin.weights = vec![[0.25, 0.75, -1.0, 0.0]; vertex_count];
        doc.scene.get_mut(id).unwrap().skin = Some(skin);

        let back = from_json(&to_json(&doc).unwrap()).unwrap();
        let skin = back.scene.get(id).unwrap().skin.as_ref().unwrap();
        assert_eq!(skin.joint_indices.len(), vertex_count);
        assert_eq!(skin.weights.len(), vertex_count);
        for (indices, weights) in skin.joint_indices.iter().zip(&skin.weights) {
            let total: f32 = weights.iter().sum();
            assert!((total - 1.0).abs() < 1e-6);
            for (index, weight) in indices.iter().zip(weights) {
                if *weight > 0.0 {
                    assert_eq!(*index, 0);
                }
            }
        }
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

    #[test]
    fn addressed_tracks_read_legacy_property_names_and_round_trip() {
        let legacy = r#"{
            "tracks": [
                {"property": "scale_z", "curve": {"keys": [{"time": 0.0, "value": 2.0}]}},
                {"property": "rotation_x", "curve": {"keys": [{"time": 0.0, "value": 30.0}]}}
            ]
        }"#;
        let clip: lc_anim::AnimationClip = serde_json::from_str(legacy).unwrap();
        assert_eq!(clip.tracks.len(), 1);
        assert_eq!(
            clip.tracks[0].address,
            ChannelAddress::owner_component(AnimChannel::Scale, 2)
        );
        assert_eq!(clip.legacy_euler_rotation.len(), 1);
        assert!(clip.legacy_euler_rotation.contains_key(&0));

        let json = serde_json::to_string(&clip).unwrap();
        assert!(json.contains("\"address\""), "new files write an address");
        assert!(
            !json.contains("scale_z"),
            "old property names are not written"
        );
        let back: lc_anim::AnimationClip = serde_json::from_str(&json).unwrap();
        assert_eq!(clip, back);
        assert_eq!(clip.signature(), back.signature());
        assert!(
            (back
                .sample(ChannelAddress::owner_component(AnimChannel::Scale, 2), 0.0)
                .unwrap()
                - 2.0)
                .abs()
                < 1e-6
        );
        let euler = back.apply_transform(lc_core::Transform::IDENTITY, 0.0);
        assert!((euler.euler_degrees().x - 30.0).abs() < 1e-3);
    }

    #[test]
    fn addressed_object_tracks_evaluate_and_survive_native_round_trip() {
        let mut doc = Document::empty();
        let id = doc.append_node(NodeKind::Null, "Addressed", None);
        {
            let address =
                ChannelAddress::new(ChannelTarget::Object { id }, AnimChannel::Translation, 1)
                    .unwrap();
            let node = doc.scene.get_mut(id).unwrap();
            node.animation
                .set_key(address, 0.0, 1.5, lc_anim::Interpolation::Linear);
            node.animation.set_rotation_key(
                ChannelTarget::Object { id },
                0.0,
                glam::Quat::from_rotation_y(0.5),
                lc_anim::Interpolation::Linear,
            );
        }
        let json = to_json(&doc).unwrap();
        let back = from_json(&json).unwrap();
        assert_eq!(doc.signature(), back.signature());
        let animation = &back.scene.get(id).unwrap().animation;
        assert_eq!(
            animation.tracks[0].address,
            ChannelAddress::new(ChannelTarget::Object { id }, AnimChannel::Translation, 1).unwrap()
        );
        assert_eq!(animation.tracks[0].address.label(), "Location Y");
        assert_eq!(
            animation.rotation.as_ref().unwrap().target,
            ChannelTarget::Object { id }
        );
        let evaluated = back.evaluate(0.0);
        assert!((evaluated.node(id).unwrap().world.w_axis.y - 1.5).abs() < 1e-4);
    }
}
