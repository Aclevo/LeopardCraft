//! Wavefront OBJ + MTL import and export.
//!
//! Deliberately covers the widely implemented subset: vertices, per-corner
//! texture coordinates, normals, polygons/objects, material libraries and the
//! standard diffuse/specular/roughness/opacity/emission entries.

use crate::common::{
    add_mesh_node, add_texture_to_base_color, preview_surface, safe_material_name,
    safe_object_name, set_principled_input,
};
use glam::{Mat3, Vec2, Vec3};
use lc_geom::{Corner, Mesh, Polygon};
use lc_scene::Document;
use lc_shading::{Material, MaterialId, ShaderValue};
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
thread_local! {
    /// Test-only injection: fail the OBJ commit after both files have been
    /// staged and the old OBJ has been backed up. Compiled out of normal builds.
    static FAIL_NEXT_OBJ_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Test-only injection: fail the MTL commit after the new OBJ has been
    /// installed and the old MTL has been backed up.
    static FAIL_NEXT_MTL_COMMIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Clone, Debug, Default)]
struct Mtl {
    name: String,
    kd: Vec3,
    ks: Vec3,
    ns: f32,
    alpha: f32,
    ke: Vec3,
    map_kd: Option<String>,
}

impl Mtl {
    fn new(name: String) -> Self {
        Self {
            name,
            kd: Vec3::splat(0.8),
            ks: Vec3::ZERO,
            ns: 250.0,
            alpha: 1.0,
            ke: Vec3::ZERO,
            map_kd: None,
        }
    }
}

#[derive(Clone, Debug)]
struct Face {
    points: Vec<u32>,
    uvs: Vec<u32>,
    material: Option<String>,
}

#[derive(Clone, Debug)]
struct Group {
    name: String,
    faces: Vec<Face>,
}

pub fn export(path: impl AsRef<Path>, doc: &Document) -> Result<(), String> {
    let path = path.as_ref();
    recover_interrupted_export(path)?;
    let mtl_path = path.with_extension("mtl");
    let mtl_name = mtl_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "materials.mtl".into());

    let evaluated = doc.evaluate(0.0);
    let mut material_names: HashMap<MaterialId, String> = HashMap::new();
    let mut used_names = Vec::new();
    let mut mtl = String::new();
    writeln!(mtl, "# LeopardCraft material library").ok();
    for material in doc.materials.iter() {
        let name = safe_material_name(&material.name, &mut used_names);
        material_names.insert(material.id, name.clone());
        let surface = preview_surface(material);
        writeln!(mtl, "\nnewmtl {name}").ok();
        writeln!(mtl, "Ka 0 0 0").ok();
        writeln!(
            mtl,
            "Kd {:.6} {:.6} {:.6}",
            surface.base_color.x, surface.base_color.y, surface.base_color.z
        )
        .ok();
        let spec = Vec3::splat(0.04).lerp(surface.base_color, surface.metallic);
        writeln!(mtl, "Ks {:.6} {:.6} {:.6}", spec.x, spec.y, spec.z).ok();
        writeln!(mtl, "Ns {:.3}", (1.0 - surface.roughness) * 1000.0).ok();
        writeln!(mtl, "d {:.6}", surface.alpha.clamp(0.0, 1.0)).ok();
        writeln!(mtl, "Tr {:.6}", 1.0 - surface.alpha.clamp(0.0, 1.0)).ok();
        writeln!(
            mtl,
            "Ke {:.6} {:.6} {:.6}",
            surface.emission.x, surface.emission.y, surface.emission.z
        )
        .ok();
        if let Some(tex) = material.graph.texture_paths().into_iter().next() {
            writeln!(mtl, "map_Kd {}", tex).ok();
        }
    }
    let mut out = String::new();
    writeln!(out, "# LeopardCraft OBJ export").ok();
    let skipped_uv_sets: usize = evaluated
        .mesh_nodes()
        .filter_map(|node| node.mesh())
        .map(|mesh| mesh.extra_uvs.len())
        .sum();
    if skipped_uv_sets > 0 {
        // OBJ has one texture-coordinate channel; say so instead of silently
        // dropping the lightmap sets a document carries.
        writeln!(
            out,
            "# skipped {skipped_uv_sets} additional UV set(s); OBJ writes UV0 only"
        )
        .ok();
    }
    writeln!(out, "mtllib {mtl_name}").ok();
    let mut v_offset = 1usize;
    let mut vt_offset = 1usize;
    let mut vn_offset = 1usize;
    let fallback_name = material_names
        .values()
        .next()
        .cloned()
        .unwrap_or_else(|| "default".into());

    for node in evaluated.mesh_nodes() {
        let Some(mesh) = node.mesh() else {
            continue;
        };
        if mesh.is_empty() {
            continue;
        }
        let name = safe_object_name(&node.name);
        writeln!(out, "\no {name}").ok();
        let world = node.world;
        let normal_matrix = Mat3::from_mat4(world).inverse().transpose();
        for p in &mesh.positions {
            let p = world.transform_point3(*p);
            writeln!(out, "v {:.6} {:.6} {:.6}", p.x, p.y, p.z).ok();
        }
        for uv in &mesh.uvs {
            writeln!(out, "vt {:.6} {:.6}", uv.x, uv.y).ok();
        }
        let normals = mesh.corner_normals(40.0);
        let mut normal_count = 0usize;
        for (pi, poly) in mesh.polygons.iter().enumerate() {
            for (ci, _) in poly.corners.iter().enumerate() {
                let n = normals
                    .get(pi)
                    .and_then(|face| face.get(ci))
                    .copied()
                    .unwrap_or(Vec3::Y);
                let n = (normal_matrix * n).normalize_or(Vec3::Y);
                writeln!(out, "vn {:.6} {:.6} {:.6}", n.x, n.y, n.z).ok();
                normal_count += 1;
            }
        }

        let mut current_material: Option<String> = None;
        let mut corner_base = 0usize;
        for poly in &mesh.polygons {
            let mat_id = node.material_for_slot(poly.material_slot).unwrap_or(0);
            let name = material_names
                .get(&mat_id)
                .cloned()
                .unwrap_or_else(|| fallback_name.clone());
            if current_material.as_deref() != Some(name.as_str()) {
                writeln!(out, "usemtl {name}").ok();
                current_material = Some(name);
            }
            let has_uv = !mesh.uvs.is_empty();
            let mut face = String::new();
            for (ci, corner) in poly.corners.iter().enumerate() {
                let v = v_offset + corner.point as usize;
                let vn = vn_offset + corner_base + ci;
                if has_uv {
                    let vt = vt_offset + (corner.uv as usize).min(mesh.uvs.len() - 1);
                    write!(face, " {v}/{vt}/{vn}").ok();
                } else {
                    write!(face, " {v}//{vn}").ok();
                }
            }
            writeln!(out, "f{}", face).ok();
            corner_base += poly.corners.len();
        }
        v_offset += mesh.positions.len();
        vt_offset += mesh.uvs.len();
        vn_offset += normal_count;
    }
    write_obj_and_mtl(path, out.as_bytes(), &mtl_path, mtl.as_bytes())
}

/// Reconcile artifacts left by an interrupted OBJ/MTL commit for `obj_path`.
///
/// A completed pair (both targets present and no staged files) keeps the new
/// files and drops the backups. Otherwise the previous files are restored from
/// their backups, a newly installed OBJ with no backup is removed, and all
/// staged files are dropped. Recovery is idempotent and only touches siblings
/// whose names match this writer's transaction format.
pub fn recover_interrupted_export(obj_path: impl AsRef<Path>) -> Result<(), String> {
    let obj_path = obj_path.as_ref();
    let mtl_path = obj_path.with_extension("mtl");
    recover_pair(obj_path, &mtl_path)
}

/// Reconcile every interrupted OBJ/MTL commit represented in `dir`.
/// Returns the number of pairs whose leftover artifacts were cleaned.
pub fn recover_interrupted_exports(dir: impl AsRef<Path>) -> Result<usize, String> {
    let dir = dir.as_ref();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(format!(
                "failed to scan {} for interrupted exports: {error}",
                dir.display()
            ))
        }
    };

    let mut targets = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to scan {} for interrupted exports: {error}",
                dir.display()
            )
        })?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect {}: {error}", entry.path().display()))?;
        if !file_type.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let Some(target_name) = interrupted_export_target(&file_name) else {
            continue;
        };
        let target = dir.join(target_name);
        let Some(extension) = target.extension().and_then(|extension| extension.to_str()) else {
            continue;
        };
        let obj = if extension.eq_ignore_ascii_case("obj") {
            target
        } else if extension.eq_ignore_ascii_case("mtl") {
            target.with_extension("obj")
        } else {
            continue;
        };
        targets.insert(obj);
    }

    let mut recovered = 0;
    let mut errors = Vec::new();
    for obj in targets {
        match recover_pair(&obj, &obj.with_extension("mtl")) {
            Ok(()) => recovered += 1,
            Err(error) => errors.push(error),
        }
    }
    if errors.is_empty() {
        Ok(recovered)
    } else {
        Err(errors.join("; "))
    }
}

fn recover_pair(obj_path: &Path, mtl_path: &Path) -> Result<(), String> {
    let obj_temps = artifacts(obj_path, "tmp")?;
    let mtl_temps = artifacts(mtl_path, "tmp")?;
    let mut obj_backups = artifacts(obj_path, "bak")?;
    let mut mtl_backups = artifacts(mtl_path, "bak")?;
    let has_temps = !obj_temps.is_empty() || !mtl_temps.is_empty();
    let has_backups = !obj_backups.is_empty() || !mtl_backups.is_empty();
    if !has_temps && !has_backups {
        return Ok(());
    }

    let obj_exists = path_exists(obj_path)?;
    let mtl_exists = path_exists(mtl_path)?;
    if obj_exists && mtl_exists && !has_temps && has_backups {
        remove_artifacts(&obj_backups)?;
        remove_artifacts(&mtl_backups)?;
        return Ok(());
    }

    if let Some(backup) = newest_backup(&mut obj_backups) {
        restore_backup(&backup, obj_path)?;
    } else if obj_exists && obj_temps.is_empty() && !mtl_temps.is_empty() {
        // No old OBJ existed, so the target was installed by the interrupted
        // transaction. Remove it to restore the pre-export state.
        std::fs::remove_file(obj_path)
            .map_err(|error| format!("failed to remove {}: {error}", obj_path.display()))?;
    }
    if let Some(backup) = newest_backup(&mut mtl_backups) {
        restore_backup(&backup, mtl_path)?;
    }
    remove_artifacts(&obj_temps)?;
    remove_artifacts(&mtl_temps)?;
    remove_artifacts(&obj_backups)?;
    remove_artifacts(&mtl_backups)?;
    Ok(())
}

fn artifacts(target: &Path, tag: &str) -> Result<Vec<PathBuf>, String> {
    let parent = match target.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid output path {}", target.display()))?;
    let prefix = format!(".{name}.{tag}-");
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "failed to scan {} for interrupted exports: {error}",
                parent.display()
            ))
        }
    };

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to scan {} for interrupted exports: {error}",
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
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect {}: {error}", entry.path().display()))?;
        if file_type.is_file() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn interrupted_export_target(file_name: &str) -> Option<&str> {
    let name = file_name.strip_prefix('.')?;
    for tag in ["bak", "tmp"] {
        let marker = format!(".{tag}-");
        if let Some((target, suffix)) = name.rsplit_once(marker.as_str()) {
            if !target.is_empty() && is_artifact_suffix(suffix) {
                return Some(target);
            }
        }
    }
    None
}

fn is_artifact_suffix(suffix: &str) -> bool {
    let Some((pid, nonce)) = suffix.split_once('-') else {
        return false;
    };
    pid.parse::<u32>().is_ok() && nonce.parse::<u64>().is_ok()
}

fn newest_backup(backups: &mut Vec<PathBuf>) -> Option<PathBuf> {
    backups.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    });
    backups.pop()
}

fn restore_backup(backup: &Path, target: &Path) -> Result<(), String> {
    std::fs::rename(backup, target).map_err(|error| {
        format!(
            "failed to restore {} from {}: {error}",
            target.display(),
            backup.display()
        )
    })
}

fn remove_artifacts(paths: &[PathBuf]) -> Result<(), String> {
    for path in paths {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("failed to remove {}: {error}", path.display())),
        }
    }
    Ok(())
}

fn path_exists(path: &Path) -> Result<bool, String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("failed to inspect {}: {error}", path.display())),
    }
}

/// Commit an `.obj` and its `.mtl` sidecar as one unit.
///
/// Both files are staged before either target is touched. The OBJ is
/// committed first, so a failed OBJ write leaves an existing MTL untouched.
/// If the MTL commit fails after the OBJ commit, the OBJ is restored from its
/// backup. Temporary and backup files are cleaned up on every path; a process
/// crash is reconciled by `recover_interrupted_export` on the next export or
/// app startup.
fn write_obj_and_mtl(
    obj_path: &Path,
    obj_bytes: &[u8],
    mtl_path: &Path,
    mtl_bytes: &[u8],
) -> Result<(), String> {
    ensure_replaceable(obj_path)?;
    ensure_replaceable(mtl_path)?;

    let obj_temp = temp_sibling(obj_path, "tmp")?;
    let mtl_temp = temp_sibling(mtl_path, "tmp")?;
    let discard_temps = || {
        let _ = std::fs::remove_file(&obj_temp);
        let _ = std::fs::remove_file(&mtl_temp);
    };

    if let Err(error) = stage(&obj_temp, obj_bytes) {
        discard_temps();
        return Err(format!("failed to write {}: {error}", obj_path.display()));
    }
    if let Err(error) = stage(&mtl_temp, mtl_bytes) {
        discard_temps();
        return Err(format!("failed to write {}: {error}", mtl_path.display()));
    }

    let obj_backup = match backup_existing(obj_path) {
        Ok(backup) => backup,
        Err(error) => {
            discard_temps();
            return Err(error);
        }
    };
    #[cfg(test)]
    if FAIL_NEXT_OBJ_WRITE.with(|cell| cell.replace(false)) {
        assert!(
            !obj_path.exists(),
            "the old OBJ must have been backed up before the injected OBJ failure"
        );
        if let Some(backup) = &obj_backup {
            let _ = std::fs::rename(backup, obj_path);
        }
        discard_temps();
        return Err(format!(
            "failed to write {}: induced test failure",
            obj_path.display()
        ));
    }
    if let Err(error) = std::fs::rename(&obj_temp, obj_path) {
        if let Some(backup) = &obj_backup {
            let _ = std::fs::rename(backup, obj_path);
        }
        discard_temps();
        return Err(format!("failed to write {}: {error}", obj_path.display()));
    }

    let mtl_backup = match backup_existing(mtl_path) {
        Ok(backup) => backup,
        Err(error) => {
            // The old MTL was never moved; roll back only the installed OBJ.
            rollback_pair(obj_path, &obj_backup, mtl_path, &None, &mtl_temp);
            return Err(error);
        }
    };
    #[cfg(test)]
    if FAIL_NEXT_MTL_COMMIT.with(|cell| cell.replace(false)) {
        assert_eq!(
            std::fs::read(obj_path).ok().as_deref(),
            Some(obj_bytes),
            "the OBJ commit must have completed before the injected MTL failure"
        );
        assert!(
            !mtl_path.exists(),
            "the old MTL must have been backed up before the injected MTL failure"
        );
        rollback_pair(obj_path, &obj_backup, mtl_path, &mtl_backup, &mtl_temp);
        return Err(format!(
            "failed to write {}: induced test failure",
            mtl_path.display()
        ));
    }
    if let Err(error) = std::fs::rename(&mtl_temp, mtl_path) {
        rollback_pair(obj_path, &obj_backup, mtl_path, &mtl_backup, &mtl_temp);
        return Err(format!("failed to write {}: {error}", mtl_path.display()));
    }

    if let Some(backup) = obj_backup {
        let _ = std::fs::remove_file(backup);
    }
    if let Some(backup) = mtl_backup {
        let _ = std::fs::remove_file(backup);
    }
    Ok(())
}

fn ensure_replaceable(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Err(format!(
            "failed to write {}: target is a directory",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to write {}: {error}", path.display())),
    }
}

fn backup_existing(path: &Path) -> Result<Option<PathBuf>, String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => {
            let backup = temp_sibling(path, "bak")?;
            std::fs::rename(path, &backup)
                .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
            Ok(Some(backup))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("failed to write {}: {error}", path.display())),
    }
}

fn rollback_pair(
    obj_path: &Path,
    obj_backup: &Option<PathBuf>,
    mtl_path: &Path,
    mtl_backup: &Option<PathBuf>,
    mtl_temp: &Path,
) {
    if let Some(backup) = obj_backup {
        // `rename` replaces the newly installed OBJ atomically.
        let _ = std::fs::rename(backup, obj_path);
    } else {
        let _ = std::fs::remove_file(obj_path);
    }
    if let Some(backup) = mtl_backup {
        let _ = std::fs::rename(backup, mtl_path);
    }
    let _ = std::fs::remove_file(mtl_temp);
}

fn stage(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn temp_sibling(path: &Path, tag: &str) -> Result<PathBuf, String> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    artifact_sibling(path, tag, nonce)
}

fn artifact_sibling(path: &Path, tag: &str, nonce: u64) -> Result<PathBuf, String> {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid output path {}", path.display()))?;
    Ok(parent.join(format!(".{name}.{tag}-{}-{nonce}", std::process::id())))
}

pub fn import(path: impl AsRef<Path>) -> Result<Document, String> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let base = path.parent().unwrap_or(Path::new("."));
    let mut mtls = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.to_ascii_lowercase().starts_with("mtllib ") {
            let name = line[7..].trim();
            if !name.is_empty() {
                let mtl_path = resolve_path(base, name);
                if let Ok(mtl_text) = std::fs::read_to_string(&mtl_path) {
                    parse_mtl_text(&mtl_text, mtl_path.parent().unwrap_or(base), &mut mtls);
                }
            }
        }
    }

    let mut positions: Vec<Vec3> = Vec::new();
    let mut uvs: Vec<Vec2> = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    let mut group_index: HashMap<String, usize> = HashMap::new();
    let mut current_group = 0usize;
    groups.push(Group {
        name: "Imported OBJ".into(),
        faces: Vec::new(),
    });
    group_index.insert("Imported OBJ".into(), 0);
    let mut current_material: Option<String> = None;

    let ensure_group =
        |name: &str, groups: &mut Vec<Group>, group_index: &mut HashMap<String, usize>| -> usize {
            if let Some(&i) = group_index.get(name) {
                return i;
            }
            let i = groups.len();
            groups.push(Group {
                name: name.to_string(),
                faces: Vec::new(),
            });
            group_index.insert(name.to_string(), i);
            i
        };

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(tag) = parts.next() else {
            continue;
        };
        match tag {
            "v" => {
                let values: Vec<f32> = parts.filter_map(|v| v.parse().ok()).collect();
                if values.len() >= 3 && values.iter().all(|v| v.is_finite()) {
                    positions.push(Vec3::new(values[0], values[1], values[2]));
                }
            }
            "vt" => {
                let values: Vec<f32> = parts.filter_map(|v| v.parse().ok()).collect();
                if values.len() >= 2 && values.iter().all(|v| v.is_finite()) {
                    uvs.push(Vec2::new(values[0], values[1]));
                }
            }
            "o" | "g" => {
                let name = parts.collect::<Vec<_>>().join(" ");
                if !name.is_empty() {
                    current_group = ensure_group(&name, &mut groups, &mut group_index);
                }
            }
            "usemtl" => {
                let name = parts.collect::<Vec<_>>().join(" ");
                current_material = if name.is_empty() { None } else { Some(name) };
            }
            "f" => {
                let mut points = Vec::new();
                let mut face_uvs = Vec::new();
                for token in parts {
                    let pieces: Vec<&str> = token.split('/').collect();
                    let Some(v) = pieces
                        .first()
                        .and_then(|s| resolve_index(s, positions.len()))
                    else {
                        continue;
                    };
                    points.push(v);
                    let uv = pieces
                        .get(1)
                        .and_then(|s| resolve_index(s, uvs.len()))
                        .unwrap_or(0);
                    face_uvs.push(uv);
                }
                if points.len() >= 3 {
                    groups[current_group].faces.push(Face {
                        points,
                        uvs: face_uvs,
                        material: current_material.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    let mut doc = Document::empty();
    let mut material_ids: HashMap<String, MaterialId> = HashMap::new();
    for mtl in &mtls {
        if material_ids.contains_key(&mtl.name) {
            continue;
        }
        let id = doc.materials.add(material_from_mtl(mtl));
        material_ids.insert(mtl.name.clone(), id);
    }

    for group in groups {
        if group.faces.is_empty() {
            continue;
        }
        let mut mesh = Mesh::new(group.name.clone());
        mesh.positions = positions.clone();
        mesh.uvs = uvs.clone();
        let mut slots: HashMap<String, u16> = HashMap::new();
        let mut slot_ids: Vec<MaterialId> = Vec::new();
        for face in &group.faces {
            let material_name = face.material.clone().unwrap_or_default();
            let slot = if let Some(s) = slots.get(&material_name) {
                *s
            } else {
                if slot_ids.len() >= u16::MAX as usize {
                    return Err("too many materials in OBJ".into());
                }
                let slot = slot_ids.len() as u16;
                let id = material_ids
                    .get(&material_name)
                    .copied()
                    .or_else(|| {
                        doc.materials
                            .iter()
                            .find(|m| m.name == material_name)
                            .map(|m| m.id)
                    })
                    .unwrap_or(0);
                slots.insert(material_name, slot);
                slot_ids.push(id);
                slot
            };
            let corners: Vec<Corner> = face
                .points
                .iter()
                .enumerate()
                .map(|(i, p)| Corner::new(*p, face.uvs.get(i).copied().unwrap_or(0)))
                .collect();
            mesh.polygons.push(Polygon {
                corners,
                material_slot: slot,
            });
        }
        if mesh.uvs.is_empty() {
            for poly in &mut mesh.polygons {
                for c in &mut poly.corners {
                    c.uv = 0;
                }
            }
        }
        let id = add_mesh_node(&mut doc, &group.name, mesh, None)?;
        if let Some(node) = doc.scene.get_mut(id) {
            for (slot, mat) in slot_ids.iter().enumerate() {
                node.material_slots.insert(slot as u16, *mat);
            }
            node.material = slot_ids.first().copied().or(Some(0));
        }
    }
    doc.settings.ensure_valid_layers();
    doc.sanitize();
    Ok(doc)
}

fn material_from_mtl(mtl: &Mtl) -> Material {
    let roughness = (1.0 - (mtl.ns / 1000.0)).clamp(0.04, 1.0);
    let mut material = Material::pbr(0, mtl.name.clone(), mtl.kd.max(Vec3::ZERO), roughness, 0.0);
    let metallic = if mtl.ks.max_element() > 0.5 { 0.3 } else { 0.0 };
    set_principled_input(&mut material, 1, ShaderValue::Float(metallic));
    set_principled_input(
        &mut material,
        7,
        ShaderValue::Float(mtl.alpha.clamp(0.0, 1.0)),
    );
    if mtl.ke.max_element() > 0.0 {
        set_principled_input(&mut material, 5, ShaderValue::Color(mtl.ke));
        set_principled_input(&mut material, 6, ShaderValue::Float(1.0));
    }
    if let Some(path) = &mtl.map_kd {
        add_texture_to_base_color(&mut material, path);
    }
    material
}

fn parse_mtl_text(text: &str, base: &Path, out: &mut Vec<Mtl>) {
    let mut current: Option<Mtl> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(tag) = parts.next() else {
            continue;
        };
        match tag.to_ascii_lowercase().as_str() {
            "newmtl" => {
                if let Some(m) = current.take() {
                    out.push(m);
                }
                let name = parts.collect::<Vec<_>>().join(" ");
                current = Some(Mtl::new(if name.is_empty() {
                    "Material".into()
                } else {
                    name
                }));
            }
            "kd" | "ks" | "ke" => {
                let Some(m) = current.as_mut() else {
                    continue;
                };
                let values: Vec<f32> = parts.filter_map(|v| v.parse().ok()).collect();
                let color = match values.as_slice() {
                    [r, g, b, ..] => Vec3::new(*r, *g, *b),
                    [v] => Vec3::splat(*v),
                    _ => continue,
                };
                match tag.to_ascii_lowercase().as_str() {
                    "kd" => m.kd = color,
                    "ks" => m.ks = color,
                    "ke" => m.ke = color,
                    _ => {}
                }
            }
            "ns" => {
                if let (Some(m), Some(v)) = (current.as_mut(), parts.next()) {
                    if let Ok(v) = v.parse::<f32>() {
                        m.ns = v.clamp(0.0, 1000.0);
                    }
                }
            }
            "d" => {
                if let (Some(m), Some(v)) = (current.as_mut(), parts.next()) {
                    if let Ok(v) = v.parse::<f32>() {
                        m.alpha = v.clamp(0.0, 1.0);
                    }
                }
            }
            "tr" => {
                if let (Some(m), Some(v)) = (current.as_mut(), parts.next()) {
                    if let Ok(v) = v.parse::<f32>() {
                        m.alpha = (1.0 - v).clamp(0.0, 1.0);
                    }
                }
            }
            "map_kd" => {
                let Some(m) = current.as_mut() else {
                    continue;
                };
                let name = parts.collect::<Vec<_>>().join(" ");
                if !name.is_empty() {
                    let resolved = resolve_path(base, &name);
                    m.map_kd = Some(resolved.to_string_lossy().to_string());
                }
            }
            _ => {}
        }
    }
    if let Some(m) = current {
        out.push(m);
    }
}

fn resolve_index(token: &str, len: usize) -> Option<u32> {
    if token.is_empty() || len == 0 {
        return None;
    }
    let value: i64 = token.parse().ok()?;
    if value > 0 {
        let idx = (value - 1) as usize;
        if idx < len {
            Some(idx as u32)
        } else {
            None
        }
    } else if value < 0 {
        let idx = len as i64 + value;
        if idx >= 0 && (idx as usize) < len {
            Some(idx as u32)
        } else {
            None
        }
    } else {
        None
    }
}

fn resolve_path(base: &Path, name: &str) -> PathBuf {
    let p = Path::new(name);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("leopardcraft-obj-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn assert_exact_dir_contents(dir: &Path, expected: &[&str]) {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let mut expected: Vec<String> = expected.iter().map(|name| name.to_string()).collect();
        expected.sort();
        assert_eq!(names, expected);
    }

    #[test]
    fn failed_obj_commit_leaves_existing_obj_and_mtl_untouched() {
        let dir = temp_dir("transaction");
        let obj = dir.join("model.obj");
        let mtl = dir.join("model.mtl");
        std::fs::write(&obj, b"old obj").unwrap();
        std::fs::write(&mtl, b"old mtl").unwrap();

        FAIL_NEXT_OBJ_WRITE.with(|cell| cell.set(true));
        let error = export(&obj, &Document::demo())
            .expect_err("an induced OBJ write failure must be reported");
        assert!(error.contains("induced test failure"), "{error}");
        assert_eq!(std::fs::read(&obj).unwrap(), b"old obj");
        assert_eq!(std::fs::read(&mtl).unwrap(), b"old mtl");
        assert_exact_dir_contents(&dir, &["model.mtl", "model.obj"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_mtl_commit_restores_both_files_after_the_obj_was_installed() {
        let dir = temp_dir("mtl-transaction");
        let obj = dir.join("model.obj");
        let mtl = dir.join("model.mtl");
        std::fs::write(&obj, b"seeded obj bytes").unwrap();
        std::fs::write(&mtl, b"seeded mtl bytes").unwrap();

        FAIL_NEXT_MTL_COMMIT.with(|cell| cell.set(true));
        let error = export(&obj, &Document::demo())
            .expect_err("an induced MTL commit failure must be reported");
        assert!(error.contains("induced test failure"), "{error}");
        assert_eq!(std::fs::read(&obj).unwrap(), b"seeded obj bytes");
        assert_eq!(std::fs::read(&mtl).unwrap(), b"seeded mtl bytes");
        assert_exact_dir_contents(&dir, &["model.mtl", "model.obj"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_restores_both_backups_after_an_interrupted_mtl_commit() {
        let dir = temp_dir("recover-mtl");
        let obj = dir.join("model.obj");
        let mtl = dir.join("model.mtl");
        std::fs::write(&obj, b"old obj").unwrap();
        std::fs::write(&mtl, b"old mtl").unwrap();

        let obj_backup = artifact_sibling(&obj, "bak", 1).unwrap();
        let mtl_backup = artifact_sibling(&mtl, "bak", 2).unwrap();
        std::fs::rename(&obj, &obj_backup).unwrap();
        std::fs::rename(&mtl, &mtl_backup).unwrap();
        std::fs::write(&obj, b"new obj").unwrap();
        std::fs::write(artifact_sibling(&mtl, "tmp", 3).unwrap(), b"new mtl").unwrap();

        recover_interrupted_export(&obj).unwrap();
        assert_eq!(std::fs::read(&obj).unwrap(), b"old obj");
        assert_eq!(std::fs::read(&mtl).unwrap(), b"old mtl");
        assert_exact_dir_contents(&dir, &["model.mtl", "model.obj"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_rolls_back_an_installed_obj_before_the_mtl_was_backed_up() {
        let dir = temp_dir("recover-obj");
        let obj = dir.join("model.obj");
        let mtl = dir.join("model.mtl");
        std::fs::write(&obj, b"old obj").unwrap();
        std::fs::write(&mtl, b"old mtl").unwrap();

        let obj_backup = artifact_sibling(&obj, "bak", 4).unwrap();
        std::fs::rename(&obj, &obj_backup).unwrap();
        std::fs::write(&obj, b"new obj").unwrap();
        std::fs::write(artifact_sibling(&mtl, "tmp", 5).unwrap(), b"new mtl").unwrap();

        recover_interrupted_export(&obj).unwrap();
        assert_eq!(std::fs::read(&obj).unwrap(), b"old obj");
        assert_eq!(std::fs::read(&mtl).unwrap(), b"old mtl");
        assert_exact_dir_contents(&dir, &["model.mtl", "model.obj"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_removes_a_new_obj_when_no_backup_existed() {
        let dir = temp_dir("recover-new-obj");
        let obj = dir.join("model.obj");
        let mtl = dir.join("model.mtl");
        std::fs::write(&mtl, b"old mtl").unwrap();
        std::fs::write(&obj, b"new obj").unwrap();
        std::fs::write(artifact_sibling(&mtl, "tmp", 12).unwrap(), b"new mtl").unwrap();

        recover_interrupted_export(&obj).unwrap();
        assert!(!obj.exists());
        assert_eq!(std::fs::read(&mtl).unwrap(), b"old mtl");
        assert_exact_dir_contents(&dir, &["model.mtl"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_keeps_a_completed_pair_and_removes_backups() {
        let dir = temp_dir("recover-complete");
        let obj = dir.join("model.obj");
        let mtl = dir.join("model.mtl");
        std::fs::write(&obj, b"new obj").unwrap();
        std::fs::write(&mtl, b"new mtl").unwrap();
        std::fs::write(artifact_sibling(&obj, "bak", 6).unwrap(), b"old obj").unwrap();
        std::fs::write(artifact_sibling(&mtl, "bak", 7).unwrap(), b"old mtl").unwrap();

        recover_interrupted_export(&obj).unwrap();
        assert_eq!(std::fs::read(&obj).unwrap(), b"new obj");
        assert_eq!(std::fs::read(&mtl).unwrap(), b"new mtl");
        assert_exact_dir_contents(&dir, &["model.mtl", "model.obj"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn startup_recovery_removes_staged_files_and_ignores_unrelated_artifacts() {
        let dir = temp_dir("recover-startup");
        let obj = dir.join("model.obj");
        let mtl = dir.join("model.mtl");
        std::fs::write(&obj, b"old obj").unwrap();
        std::fs::write(&mtl, b"old mtl").unwrap();
        std::fs::write(artifact_sibling(&obj, "tmp", 8).unwrap(), b"new obj").unwrap();
        std::fs::write(artifact_sibling(&mtl, "tmp", 9).unwrap(), b"new mtl").unwrap();
        std::fs::write(dir.join("notes.txt"), b"keep").unwrap();
        std::fs::write(dir.join(".notes.txt.tmp-123-1"), b"keep").unwrap();

        assert_eq!(recover_interrupted_exports(&dir).unwrap(), 1);
        assert_eq!(std::fs::read(&obj).unwrap(), b"old obj");
        assert_eq!(std::fs::read(&mtl).unwrap(), b"old mtl");
        assert_exact_dir_contents(
            &dir,
            &[
                ".notes.txt.tmp-123-1",
                "model.mtl",
                "model.obj",
                "notes.txt",
            ],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn next_export_reconciles_leftover_transaction_files_before_writing() {
        let dir = temp_dir("recover-export");
        let obj = dir.join("model.obj");
        let mtl = dir.join("model.mtl");
        std::fs::write(&obj, b"interrupted obj").unwrap();
        std::fs::write(&mtl, b"old mtl").unwrap();
        std::fs::write(artifact_sibling(&obj, "bak", 10).unwrap(), b"old obj").unwrap();
        std::fs::write(artifact_sibling(&mtl, "tmp", 11).unwrap(), b"new mtl").unwrap();

        export(&obj, &Document::demo()).unwrap();
        assert!(obj.is_file());
        assert!(mtl.is_file());
        assert_exact_dir_contents(&dir, &["model.mtl", "model.obj"]);
        assert!(import(&obj).unwrap().mesh_node_count() > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn obj_export_reports_skipped_additional_uv_sets() {
        let dir = temp_dir("skipped-uv-sets");
        let obj = dir.join("model.obj");
        let mut doc = Document::empty();
        let mut mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        lc_geom::box_projection(&mut mesh, 1.0);
        mesh.add_uv_set();
        doc.append_node(
            lc_scene::NodeKind::Mesh {
                geometry: lc_scene::GeometrySource::editable(mesh),
            },
            "Lightmapped",
            None,
        );
        export(&obj, &doc).unwrap();
        let text = std::fs::read_to_string(&obj).unwrap();
        assert!(
            text.contains("# skipped 1 additional UV set(s); OBJ writes UV0 only"),
            "OBJ names the set it cannot carry"
        );
        let plain = dir.join("plain.obj");
        let mut plain_mesh = lc_geom::box_mesh(1.0, 1.0, 1.0);
        lc_geom::box_projection(&mut plain_mesh, 1.0);
        let mut plain_doc = Document::empty();
        plain_doc.append_node(
            lc_scene::NodeKind::Mesh {
                geometry: lc_scene::GeometrySource::editable(plain_mesh),
            },
            "Plain",
            None,
        );
        export(&plain, &plain_doc).unwrap();
        let plain_text = std::fs::read_to_string(&plain).unwrap();
        assert!(!plain_text.contains("skipped"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn blocked_obj_target_does_not_create_an_mtl_sidecar() {
        let dir = temp_dir("blocked-target");
        let obj = dir.join("model.obj");
        std::fs::create_dir(&obj).unwrap();

        let error =
            export(&obj, &Document::demo()).expect_err("a directory OBJ target must be rejected");
        assert!(error.contains("target is a directory"), "{error}");
        assert!(obj.is_dir());
        assert_exact_dir_contents(&dir, &["model.obj"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
