//! Real-time wgpu viewport rendered offscreen and shown as an egui image.

use eframe::egui_wgpu::{self, wgpu};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt as _;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
    pub fn new(position: Vec3, normal: Vec3, color: Vec3) -> Self {
        Self {
            position: position.to_array(),
            normal: normal.to_array(),
            color: color.to_array(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub light_dir: [f32; 4],
    pub eye: [f32; 4],
}

#[derive(Clone, Debug, Default)]
pub struct FrameData {
    pub triangles: Vec<Vertex>,
    pub lines: Vec<Vertex>,
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub light_dir: Vec3,
}

const SHADER: &str = r#"
struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    eye: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};
struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

struct BackgroundOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_background(@builtin(vertex_index) index: u32) -> BackgroundOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: BackgroundOut;
    out.clip = vec4<f32>(positions[index], 0.0, 1.0);
    out.uv = positions[index] * 0.5 + 0.5;
    return out;
}

@fragment
fn fs_background(in: BackgroundOut) -> @location(0) vec4<f32> {
    let top = vec3<f32>(0.30, 0.34, 0.39);
    let bottom = vec3<f32>(0.58, 0.61, 0.64);
    let t = smoothstep(0.0, 1.0, in.uv.y);
    return vec4<f32>(mix(bottom, top, t), 1.0);
}

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = globals.view_proj * vec4<f32>(in.position, 1.0);
    out.world = in.position;
    out.normal = in.normal;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var n = normalize(in.normal);
    if (!front) { n = -n; }
    let l = normalize(globals.light_dir.xyz);
    let v = normalize(globals.eye.xyz - in.world);
    let ndl = max(dot(n, l), 0.0);
    let h = normalize(l + v);
    let spec = pow(max(dot(n, h), 0.0), 40.0) * 0.22;
    let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.08;
    let color = in.color * (0.32 + 0.75 * ndl) + vec3<f32>(spec + rim);
    return vec4<f32>(color, 1.0);
}
"#;

struct Target {
    width: u32,
    height: u32,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    depth: wgpu::TextureView,
    id: egui::TextureId,
}

struct GpuResources {
    background_pipeline: wgpu::RenderPipeline,
    triangle_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub struct ViewportRenderer {
    render_state: Option<egui_wgpu::RenderState>,
    gpu: Option<GpuResources>,
    target: Option<Target>,
}

impl ViewportRenderer {
    pub fn new(render_state: Option<egui_wgpu::RenderState>) -> Self {
        let gpu = render_state.as_ref().map(create_resources).map(
            |(background_pipeline, triangle_pipeline, line_pipeline, uniform, bind_group)| {
                GpuResources {
                    background_pipeline,
                    triangle_pipeline,
                    line_pipeline,
                    uniform,
                    bind_group,
                }
            },
        );
        Self {
            render_state,
            gpu,
            target: None,
        }
    }

    pub fn available(&self) -> bool {
        self.render_state.is_some()
    }

    pub fn render(
        &mut self,
        frame: &FrameData,
        width: u32,
        height: u32,
    ) -> Option<egui::TextureId> {
        let rs = self.render_state.clone()?;
        let width = width.clamp(16, 4096);
        let height = height.clamp(16, 4096);
        self.ensure_target(&rs, width, height);
        let gpu = self.gpu.as_ref()?;
        let rs = &rs;

        let globals = Globals {
            view_proj: frame.view_proj.to_cols_array_2d(),
            light_dir: [frame.light_dir.x, frame.light_dir.y, frame.light_dir.z, 0.0],
            eye: [frame.eye.x, frame.eye.y, frame.eye.z, 1.0],
        };
        rs.queue
            .write_buffer(&gpu.uniform, 0, bytemuck::bytes_of(&globals));

        let triangle_buffer = if frame.triangles.is_empty() {
            None
        } else {
            Some(
                rs.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("viewport triangles"),
                        contents: bytemuck::cast_slice(&frame.triangles),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
            )
        };
        let line_buffer = if frame.lines.is_empty() {
            None
        } else {
            Some(
                rs.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("viewport lines"),
                        contents: bytemuck::cast_slice(&frame.lines),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
            )
        };

        let mut encoder = rs
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("viewport encoder"),
            });
        {
            let target = self.target.as_ref()?;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewport pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.355,
                            g: 0.390,
                            b: 0.420,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &gpu.bind_group, &[]);
            pass.set_pipeline(&gpu.background_pipeline);
            pass.draw(0..3, 0..1);
            if let Some(buffer) = &triangle_buffer {
                pass.set_pipeline(&gpu.triangle_pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..frame.triangles.len() as u32, 0..1);
            }
            if let Some(buffer) = &line_buffer {
                pass.set_pipeline(&gpu.line_pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..frame.lines.len() as u32, 0..1);
            }
        }
        rs.queue.submit(Some(encoder.finish()));
        self.target.as_ref().map(|t| t.id)
    }

    fn ensure_target(&mut self, rs: &egui_wgpu::RenderState, width: u32, height: u32) {
        if let Some(target) = &self.target {
            if target.width == width && target.height == height {
                return;
            }
            let old = target.id;
            rs.renderer.write().free_texture(&old);
        }
        let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = rs
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("viewport depth"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let id = rs.renderer.write().register_native_texture(
            &rs.device,
            &view,
            wgpu::FilterMode::Linear,
        );
        self.target = Some(Target {
            width,
            height,
            _texture: texture,
            view,
            depth,
            id,
        });
    }
}

fn create_resources(
    rs: &egui_wgpu::RenderState,
) -> (
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
    wgpu::Buffer,
    wgpu::BindGroup,
) {
    let shader = rs
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
    let bind_group_layout = rs
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport globals layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
    let uniform = rs.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("viewport globals"),
        size: std::mem::size_of::<Globals>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = rs.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("viewport globals"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let layout = rs
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
    let attributes = [
        wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x3,
        },
        wgpu::VertexAttribute {
            offset: 12,
            shader_location: 1,
            format: wgpu::VertexFormat::Float32x3,
        },
        wgpu::VertexAttribute {
            offset: 24,
            shader_location: 2,
            format: wgpu::VertexFormat::Float32x3,
        },
    ];
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &attributes,
    };
    let background_pipeline = rs
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport background pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_background"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_background"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
    let create = |topology: wgpu::PrimitiveTopology| {
        rs.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("viewport pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(vertex_layout.clone())],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
    };
    (
        background_pipeline,
        create(wgpu::PrimitiveTopology::TriangleList),
        create(wgpu::PrimitiveTopology::LineList),
        uniform,
        bind_group,
    )
}
