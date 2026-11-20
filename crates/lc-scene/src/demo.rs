//! Built-in studio scene used on first launch and by headless tests.

use crate::{
    Camera, Document, Environment, GeometrySource, Light, LightKind, Modifier, Node, NodeId,
    NodeKind, Primitive, ProjectionMode, Tag,
};
use glam::Vec3;
use lc_anim::{AnimProperty, Interpolation};
use lc_core::{look_at_quat, Transform};
use lc_shading::{MaterialLibrary, ShaderNodeKind, ShaderValue};

impl Document {
    /// A small, fully lit studio with a camera, three lights, primitives,
    /// materials and two animated objects.
    pub fn demo() -> Self {
        let mut doc = Document::empty();
        doc.settings.environment = Environment::Sky {
            top: Vec3::new(0.055, 0.09, 0.17),
            horizon: Vec3::new(0.48, 0.55, 0.64),
            ground: Vec3::new(0.075, 0.07, 0.06),
            sun_direction: Vec3::new(-0.35, -0.78, -0.5).normalize(),
            sun_color: Vec3::new(1.0, 0.94, 0.84),
            sun_intensity: 1.1,
        };
        doc.settings.frame_end = 96.0;
        doc.settings.fps = 24.0;
        doc.settings.background = Vec3::new(0.025, 0.03, 0.04);

        let add = |doc: &mut Document, mut node: Node, parent: Option<NodeId>| -> NodeId {
            if let Some(p) = parent {
                if let Some(pn) = doc.scene.get(p) {
                    node.name = doc.scene.unique_name(&node.name);
                    let _ = pn;
                }
            }
            let id = node.id;
            doc.scene
                .insert_node(node, parent, None)
                .expect("demo scene hierarchy is valid");
            id
        };

        let studio = {
            let node = doc.create_node(NodeKind::Null, "Studio");
            add(&mut doc, node, None)
        };
        let geometry_root = {
            let node = doc.create_node(NodeKind::Null, "Geometry");
            add(&mut doc, node, None)
        };

        let target = {
            let mut node = doc.create_node(NodeKind::Null, "Light Target");
            node.transform = Transform::from_translation(Vec3::new(0.0, 1.0, 0.0));
            add(&mut doc, node, Some(studio))
        };

        let key = {
            let eye = Vec3::new(5.0, 7.5, 5.5);
            let mut node = doc.create_node(
                NodeKind::Light {
                    light: Light {
                        kind: LightKind::Directional,
                        color: Vec3::new(1.0, 0.90, 0.78),
                        intensity: 3.4,
                        cast_shadows: true,
                    },
                },
                "Key Light",
            );
            node.transform = Transform::new(
                eye,
                look_at_quat(eye, Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
                Vec3::ONE,
            );
            node.tags.push(Tag::LookAt {
                target,
                up: Vec3::Y,
            });
            add(&mut doc, node, Some(studio))
        };
        let _ = key;

        {
            let mut node = doc.create_node(
                NodeKind::Light {
                    light: Light {
                        kind: LightKind::Point { radius: 1.4 },
                        color: Vec3::new(0.62, 0.78, 1.0),
                        intensity: 42.0,
                        cast_shadows: true,
                    },
                },
                "Fill Light",
            );
            node.transform = Transform::from_translation(Vec3::new(-4.6, 3.8, 3.2));
            add(&mut doc, node, Some(studio));
        }

        {
            let mut node = doc.create_node(
                NodeKind::Light {
                    light: Light {
                        kind: LightKind::Point { radius: 0.7 },
                        color: Vec3::new(1.0, 0.62, 0.32),
                        intensity: 30.0,
                        cast_shadows: false,
                    },
                },
                "Rim Light",
            );
            node.transform = Transform::from_translation(Vec3::new(1.0, 3.4, -5.2));
            add(&mut doc, node, Some(studio));
        }

        let camera = {
            let eye = Vec3::new(6.4, 4.0, 8.2);
            let look = Vec3::new(0.1, 1.15, 0.0);
            let mut node = doc.create_node(
                NodeKind::Camera {
                    camera: Camera {
                        fov_y_deg: 42.0,
                        projection: ProjectionMode::Perspective,
                        ortho_scale: 5.0,
                        near: 0.05,
                        far: 500.0,
                        aperture: 0.035,
                        focus_distance: eye.distance(look),
                    },
                },
                "Camera",
            );
            node.transform = Transform::new(eye, look_at_quat(eye, look, Vec3::Y), Vec3::ONE);
            add(&mut doc, node, Some(studio))
        };
        doc.settings.active_camera = Some(camera);

        // Ground
        {
            let mut node = doc.create_node(
                NodeKind::Mesh {
                    geometry: GeometrySource::from_primitive(Primitive::Plane {
                        width: 30.0,
                        depth: 30.0,
                        segments_x: 1,
                        segments_z: 1,
                    }),
                },
                "Ground",
            );
            node.material = Some(0);
            add(&mut doc, node, Some(geometry_root));
        }

        // Beveled cube
        {
            let mut node = doc.create_node(
                NodeKind::Mesh {
                    geometry: GeometrySource::from_primitive(Primitive::Box {
                        width: 1.8,
                        height: 1.8,
                        depth: 1.8,
                    }),
                },
                "Cube",
            );
            node.transform = Transform::new(
                Vec3::new(-2.0, 0.9, 0.1),
                glam::Quat::from_rotation_y(24f32.to_radians()),
                Vec3::ONE,
            );
            node.material = Some(4);
            node.modifiers.push(Modifier::BevelFaces {
                amount: 0.09,
                depth: 0.025,
                faces: Vec::new(),
            });
            add(&mut doc, node, Some(geometry_root));
        }

        // Animated sphere
        {
            let mut node = doc.create_node(
                NodeKind::Mesh {
                    geometry: GeometrySource::from_primitive(Primitive::Sphere {
                        radius: 1.02,
                        segments: 48,
                        rings: 32,
                    }),
                },
                "Sphere",
            );
            node.transform = Transform::from_translation(Vec3::new(1.35, 1.05, -1.15));
            node.material = Some(3);
            for (t, y) in [
                (0.0, 1.05),
                (24.0, 2.15),
                (48.0, 1.05),
                (72.0, 1.55),
                (96.0, 1.05),
            ] {
                node.animation
                    .set_key(AnimProperty::TranslationY, t, y, Interpolation::Bezier);
            }
            add(&mut doc, node, Some(geometry_root));
        }

        // Animated torus
        {
            let mut node = doc.create_node(
                NodeKind::Mesh {
                    geometry: GeometrySource::from_primitive(Primitive::Torus {
                        major_radius: 1.0,
                        minor_radius: 0.32,
                        segments: 48,
                        rings: 24,
                    }),
                },
                "Torus",
            );
            node.transform = Transform::new(
                Vec3::new(2.9, 1.05, 1.1),
                glam::Quat::from_euler(glam::EulerRot::XYZ, 25f32.to_radians(), 0.0, 0.0),
                Vec3::ONE,
            );
            node.material = Some(1);
            node.animation
                .set_key(AnimProperty::RotationY, 0.0, 0.0, Interpolation::Linear);
            node.animation
                .set_key(AnimProperty::RotationY, 96.0, 360.0, Interpolation::Linear);
            add(&mut doc, node, Some(geometry_root));
        }

        // Glass bauble
        {
            let mut node = doc.create_node(
                NodeKind::Mesh {
                    geometry: GeometrySource::from_primitive(Primitive::Sphere {
                        radius: 0.62,
                        segments: 40,
                        rings: 24,
                    }),
                },
                "Glass Sphere",
            );
            node.transform = Transform::from_translation(Vec3::new(-2.85, 0.62, 2.35));
            node.material = Some(2);
            add(&mut doc, node, Some(geometry_root));
        }

        // Checker panel
        {
            let mut node = doc.create_node(
                NodeKind::Mesh {
                    geometry: GeometrySource::from_primitive(Primitive::Box {
                        width: 2.4,
                        height: 0.06,
                        depth: 1.5,
                    }),
                },
                "Checker Panel",
            );
            node.transform = Transform::from_translation(Vec3::new(0.2, 0.03, 2.6));
            node.material = Some(5);
            add(&mut doc, node, Some(geometry_root));
        }

        doc.settings.ensure_valid_layers();
        doc
    }

    /// Add a node with a generated id and unique name, returning the id.
    pub fn append_node(&mut self, kind: NodeKind, name: &str, parent: Option<NodeId>) -> NodeId {
        let node = self.create_node(kind, name);
        let id = node.id;
        let _ = self.scene.insert_node(node, parent, None);
        id
    }

    /// A new default PBR material with a unique name, returning its id.
    pub fn add_default_material(&mut self, name: &str) -> lc_shading::MaterialId {
        let unique = self
            .materials
            .iter()
            .map(|m| m.name.clone())
            .collect::<Vec<_>>();
        let mut candidate = name.to_string();
        let mut n = 2;
        while unique.contains(&candidate) {
            candidate = format!("{name} {n}");
            n += 1;
        }
        self.materials.add(lc_shading::Material::new(0, candidate))
    }

    /// Clear animation from every node that has some.
    pub fn clear_all_animation(&mut self) {
        for node in self.scene.nodes.values_mut() {
            node.animation = lc_anim::AnimationClip::default();
        }
    }

    /// Assign a material to the currently selected object's slot.
    pub fn set_slot_material(
        &mut self,
        node: NodeId,
        slot: u16,
        material: Option<lc_shading::MaterialId>,
    ) {
        if let Some(n) = self.scene.get_mut(node) {
            match material {
                Some(m) => {
                    n.material_slots.insert(slot, m);
                }
                None => {
                    n.material_slots.remove(&slot);
                }
            }
        }
    }

    /// Ensure a material named `name` exists, returning its id.
    pub fn ensure_material(&mut self, name: &str, base: Vec3) -> lc_shading::MaterialId {
        if let Some(m) = self.materials.iter().find(|m| m.name == name) {
            return m.id;
        }
        let id = self
            .materials
            .add(lc_shading::Material::pbr(0, name, base, 0.5, 0.0));
        id
    }

    /// Set a Principled input on a material by index (helper for demos/tests).
    pub fn set_material_input(
        &mut self,
        material: lc_shading::MaterialId,
        input: usize,
        value: ShaderValue,
    ) -> bool {
        let Some(m) = self.materials.get_mut(material) else {
            return false;
        };
        let Some(p) = m
            .graph
            .nodes
            .iter_mut()
            .find(|n| matches!(n.kind, ShaderNodeKind::Principled))
        else {
            return false;
        };
        if let Some(slot) = p.inputs.get_mut(input) {
            *slot = value;
            true
        } else {
            false
        }
    }

    /// Replace the material library with the standard studio set.
    pub fn reset_materials(&mut self) {
        self.materials = MaterialLibrary::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_has_camera_lights_and_meshes() {
        let doc = Document::demo();
        assert!(doc.scene.len() >= 9);
        assert!(doc.active_camera_id().is_some());
        assert!(doc.mesh_node_count() >= 5);
        assert!(doc
            .scene
            .nodes
            .values()
            .any(|n| matches!(n.kind, NodeKind::Light { .. })));
    }

    #[test]
    fn demo_animation_has_keys() {
        let doc = Document::demo();
        let animated = doc
            .scene
            .nodes
            .values()
            .filter(|n| !n.animation.is_empty())
            .count();
        assert_eq!(animated, 2);
    }
}
