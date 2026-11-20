//! Shared helpers for import/export.

use lc_geom::Mesh;
use lc_scene::{Document, GeometrySource, NodeId, NodeKind};
use lc_shading::{Material, ShaderNodeKind, ShaderValue, ShadingContext, Surface, TextureCache};

pub fn add_mesh_node(
    doc: &mut Document,
    name: &str,
    mesh: Mesh,
    parent: Option<NodeId>,
) -> Result<NodeId, String> {
    let node = doc.create_node(
        NodeKind::Mesh {
            geometry: GeometrySource::editable(mesh),
        },
        name,
    );
    let id = node.id;
    doc.scene
        .insert_node(node, parent, None)
        .map_err(|e| format!("could not add imported mesh: {e}"))?;
    Ok(id)
}

pub fn set_principled_input(material: &mut Material, index: usize, value: ShaderValue) {
    if let Some(p) = material
        .graph
        .nodes
        .iter_mut()
        .find(|n| matches!(n.kind, ShaderNodeKind::Principled))
    {
        if let Some(input) = p.inputs.get_mut(index) {
            *input = value;
        }
    }
}

pub fn add_texture_to_base_color(material: &mut Material, path: &str) {
    if path.trim().is_empty() {
        return;
    }
    let Some(principled) = material
        .graph
        .nodes
        .iter()
        .find(|n| matches!(n.kind, ShaderNodeKind::Principled))
        .map(|n| n.id)
    else {
        return;
    };
    let node = material.graph.add_node(
        ShaderNodeKind::Texture {
            path: path.to_string(),
        },
        glam::Vec2::new(40.0, 220.0),
    );
    material.graph.link(node, principled, 0);
}

pub fn preview_surface(material: &Material) -> Surface {
    let cache = TextureCache::default();
    material.evaluate(&ShadingContext::new(&cache))
}

pub fn safe_material_name(name: &str, used: &mut Vec<String>) -> String {
    let base: String = name
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .filter(|c| !matches!(c, '/' | '\\' | '#'))
        .collect();
    let base = if base.is_empty() {
        "Material".to_string()
    } else {
        base
    };
    if !used.iter().any(|n| n == &base) {
        used.push(base.clone());
        return base;
    }
    for i in 2..10_000 {
        let candidate = format!("{base}_{i}");
        if !used.iter().any(|n| n == &candidate) {
            used.push(candidate.clone());
            return candidate;
        }
    }
    format!("{base}_copy")
}

pub fn safe_object_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .collect();
    if cleaned.is_empty() {
        "Object".into()
    } else {
        cleaned
    }
}
