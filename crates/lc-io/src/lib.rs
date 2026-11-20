//! Import and export for LeopardCraft.
//!
//! Supported: native `.lcscene`, Wavefront OBJ/MTL, binary or ASCII STL,
//! glTF 2.0 JSON and GLB. Importers are defensive: malformed data is rejected
//! with a message or repaired by the document sanitizer instead of panicking.

pub mod common;
pub mod gltf;
pub mod native;
pub mod obj;
pub mod stl;

pub use native::{from_json, load as load_native, save as save_native, to_json};
pub use stl::StlFormat;

use lc_scene::Document;
use std::path::Path;

pub fn import(path: impl AsRef<Path>) -> Result<Document, String> {
    let path = path.as_ref();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "lcscene" | "json" => native::load(path),
        "obj" => obj::import(path),
        "stl" => stl::import(path),
        "glb" => gltf::import_glb(path),
        "gltf" => gltf::import_gltf(path),
        _ => Err(format!(
            "unsupported import format '.{ext}' (use .lcscene, .obj, .stl, .glb or .gltf)"
        )),
    }
}

pub fn export(path: impl AsRef<Path>, doc: &Document) -> Result<(), String> {
    let path = path.as_ref();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "lcscene" | "json" => native::save(path, doc),
        "obj" => obj::export(path, doc),
        "stl" => stl::export(path, doc, StlFormat::Binary),
        "glb" => gltf::export_glb(path, doc),
        "gltf" => gltf::export_gltf(path, doc),
        _ => Err(format!(
            "unsupported export format '.{ext}' (use .lcscene, .obj, .stl, .glb or .gltf)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lc_geom::Mesh;
    use lc_scene::{GeometrySource, NodeKind};
    use std::path::PathBuf;

    fn box_document() -> Document {
        let mut doc = Document::empty();
        let mesh = lc_geom::box_mesh(2.0, 1.0, 3.0);
        let id = doc.append_node(
            NodeKind::Mesh {
                geometry: GeometrySource::editable(mesh),
            },
            "Box",
            None,
        );
        if let Some(node) = doc.scene.get_mut(id) {
            node.material = Some(1);
        }
        doc.settings.ensure_valid_layers();
        doc
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "leopardcraft-io-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn triangles(doc: &Document) -> usize {
        doc.evaluate(0.0).triangle_count()
    }

    #[test]
    fn native_roundtrip_preserves_signature() {
        let doc = Document::demo();
        let dir = temp_dir();
        let path = dir.join("scene.lcscene");
        native::save(&path, &doc).unwrap();
        let back = native::load(&path).unwrap();
        assert_eq!(doc.signature(), back.signature());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn obj_roundtrip_keeps_geometry() {
        let doc = box_document();
        let dir = temp_dir();
        let path = dir.join("box.obj");
        obj::export(&path, &doc).unwrap();
        assert!(path.with_extension("mtl").exists());
        let back = obj::import(&path).unwrap();
        assert_eq!(triangles(&doc), triangles(&back));
        assert!(back.mesh_node_count() >= 1);
        assert!(back.validate().is_ok(), "{:?}", back.validate().err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stl_binary_and_ascii_roundtrip() {
        let doc = box_document();
        let dir = temp_dir();
        for (name, format) in [
            ("binary.stl", StlFormat::Binary),
            ("ascii.stl", StlFormat::Ascii),
        ] {
            let path = dir.join(name);
            stl::export(&path, &doc, format).unwrap();
            let back = stl::import(&path).unwrap();
            assert_eq!(triangles(&back), 12, "{name}");
            assert!(
                back.validate().is_ok(),
                "{name}: {:?}",
                back.validate().err()
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn glb_roundtrip_preserves_geometry_and_hierarchy() {
        let doc = box_document();
        let dir = temp_dir();
        let path = dir.join("box.glb");
        gltf::export_glb(&path, &doc).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], &0x4654_6C67u32.to_le_bytes());
        let back = gltf::import_glb(&path).unwrap();
        assert_eq!(triangles(&doc), triangles(&back));
        assert!(back.scene.find_by_name("Box").is_some());
        assert!(back.validate().is_ok(), "{:?}", back.validate().err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn embedded_gltf_roundtrip() {
        let doc = box_document();
        let dir = temp_dir();
        let path = dir.join("box.gltf");
        gltf::export_gltf(&path, &doc).unwrap();
        let back = gltf::import_gltf(&path).unwrap();
        assert_eq!(triangles(&doc), triangles(&back));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn malformed_imports_are_errors_not_panics() {
        let dir = temp_dir();
        let bad_obj = dir.join("bad.obj");
        std::fs::write(&bad_obj, "v 0 0 0\nf 1 2 99\n").unwrap();
        let doc = obj::import(&bad_obj).unwrap();
        assert_eq!(doc.mesh_node_count(), 0);
        assert!(stl::import(&bad_obj).is_err());
        let bad_glb = dir.join("bad.glb");
        std::fs::write(&bad_glb, b"not a glb").unwrap();
        assert!(gltf::import_glb(&bad_glb).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mesh_helper_is_available() {
        let mesh = Mesh::new("Empty");
        assert!(mesh.is_empty());
    }
}
