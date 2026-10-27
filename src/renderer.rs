use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Mat4, Vec3};
use hexel_engine::{
    planetary_chunks::{CubeFace, PlanetChunkKey, PlanetChunkMesh},
    planetary_frame::PlanetaryFrame,
    planetary_world::PlanetaryWorld,
    BlockId, CellPos, ChunkPos, WorldScale,
    planet::{Biome, PlanetConfig, PlanetGenerator, PlanetSample},
    raycast_prism,
    surface_mesh::SurfaceMesh,
    terrain_height,
    topology::SurfaceTopology,
};
use noise::{NoiseFn, Perlin};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

const CHUNK_SIZE: i32 = 16;
const MAX_EDIT_HEIGHT: i32 = 12;
const STREAM_RADIUS_CHUNKS: i32 = 5;

const SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32>, };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var tree_textures: texture_2d_array<f32>;
@group(1) @binding(1) var foliage_sampler: sampler;
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) textured: f32,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) textured: f32,
};
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = camera.view_proj * vec4<f32>(input.position, 1.0);
    output.color = input.color;
    output.uv = input.uv;
    output.textured = input.textured;
    return output;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.textured > 0.5 {
        if input.textured > 1.5 {
            let layer = i32(input.textured) - 2;
            let texel = textureSample(tree_textures, foliage_sampler, input.uv, layer);
            if layer < 15 && texel.a < 0.42 { discard; }
            return vec4<f32>(input.color * texel.rgb, 1.0);
        }
        let leaf = textureSample(tree_textures, foliage_sampler, input.uv, 0);
        if leaf.a < 0.42 { discard; }
        return vec4<f32>(input.color * leaf.rgb, 1.0);
    }
    return vec4<f32>(input.color, 1.0);
}
"#;

const OVERLAY_SHADER: &str = r#"
struct VertexInput { @location(0) position: vec3<f32>, @location(1) color: vec3<f32>, };
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) color: vec3<f32>, };
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = vec4<f32>(input.position, 1.0);
    output.color = input.color;
    return output;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> { return vec4<f32>(input.color, 1.0); }
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
    uv: [f32; 2],
    textured: f32,
}
impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32];
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

pub struct Camera {
    pub position: Vec3,
    pub up: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 15.0, -10.0),
            up: Vec3::Y,
            yaw: 0.0,
            pitch: -0.15,
            speed: 6.5,
        }
    }
}
impl Camera {
    fn tangent_frame(&self) -> (Vec3, Vec3, Vec3) {
        let up = self.up.normalize_or_zero();
        let up = if up == Vec3::ZERO { Vec3::Y } else { up };
        let reference = if up.dot(Vec3::Y).abs() < 0.99 { Vec3::Y } else { Vec3::Z };
        let north = (reference - up * reference.dot(up)).normalize_or_zero();
        let east = up.cross(north).normalize_or_zero();
        (east, north, up)
    }
    pub fn forward(&self) -> Vec3 {
        let (east, north, up) = self.tangent_frame();
        (east * self.yaw.sin() * self.pitch.cos() + north * self.yaw.cos() * self.pitch.cos() + up * self.pitch.sin()).normalize_or_zero()
    }
    fn view_projection(&self, size: PhysicalSize<u32>) -> Mat4 {
        let aspect = size.width.max(1) as f32 / size.height.max(1) as f32;
        Mat4::perspective_rh(70.0_f32.to_radians(), aspect, 0.05, 900.0)
            * Mat4::look_to_rh(self.position, self.forward(), self.up.normalize_or_zero())
    }
}

struct MeshJob {
    revision: u64,
    seed: u32,
    edits: HashMap<CellPos, BlockId>,
    chunks: Vec<ChunkPos>,
}
struct MeshResult {
    revision: u64,
    chunks: Vec<(ChunkPos, Vec<Vertex>)>,
}
struct ChunkMesh {
    buffer: wgpu::Buffer,
    vertex_count: u32,
}

struct PlanetGpuChunk {
    buffer: wgpu::Buffer,
    vertex_count: u32,
}
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    outline_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    crosshair_buffer: wgpu::Buffer,
    target_buffer: wgpu::Buffer,
    planet_vertex_buffer: wgpu::Buffer,
    planet_vertex_count: u32,
    planet_vertices: Vec<Vertex>,
    planet_cell_ranges: HashMap<u32, (u64, u64)>,
    planet_world: PlanetaryWorld,
    planet_stream_chunks: HashMap<PlanetChunkKey, PlanetGpuChunk>,
    planet_stream_pending: VecDeque<PlanetChunkKey>,
    chunks: HashMap<ChunkPos, ChunkMesh>,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    foliage_bind_group: wgpu::BindGroup,
    size: PhysicalSize<u32>,
    _depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    seed: u32,
    edits: HashMap<CellPos, BlockId>,
    mesh_job_tx: Sender<MeshJob>,
    mesh_result_rx: Receiver<MeshResult>,
    dirty_chunks: HashSet<ChunkPos>,
    edit_revision: u64,
    pending_mesh_chunks: VecDeque<(u64, ChunkPos, Vec<Vertex>)>,
    pending_mesh_count: usize,
    pending_mesh_revision: u64,
}
impl Renderer {
    pub async fn new(window: Arc<Window>, seed: u32) -> Result<Self, String> {
        let size = window.inner_size();
        let backends = if cfg!(target_os = "windows") {
            wgpu::Backends::DX12
        } else {
            wgpu::Backends::all()
        };
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        let surface = instance
            .create_surface(window)
            .map_err(|e| format!("Could not create graphics surface: {e}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "No compatible graphics adapter was found".to_string())?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Hexel Engine device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|e| format!("Could not create graphics device: {e}"))?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: caps
                .present_modes
                .iter()
                .copied()
                .find(|m| *m == wgpu::PresentMode::Fifo)
                .unwrap_or(caps.present_modes[0]),
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Hexel world shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera uniform"),
            contents: bytemuck::bytes_of(&CameraUniform {
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let foliage_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Tree texture-array layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let (tree_texture, foliage_sampler) = create_tree_texture_array(&device, &queue);
        let foliage_view = tree_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let foliage_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Imported species leaf and bark textures"),
            layout: &foliage_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&foliage_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&foliage_sampler),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Hexel pipeline layout"),
            bind_group_layouts: &[&camera_layout, &foliage_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Hexel prism pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
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
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let outline_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Voxel target outline pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: -2,
                    slope_scale: -1.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Crosshair overlay shader"),
            source: wgpu::ShaderSource::Wgsl(OVERLAY_SHADER.into()),
        });
        let overlay_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Crosshair overlay layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Crosshair overlay pipeline"),
            layout: Some(&overlay_layout),
            vertex: wgpu::VertexState {
                module: &overlay_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &overlay_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let crosshair_vertices = [
            Vertex {
                position: [-0.018, 0.0, 0.0],
                color: [0.02, 0.02, 0.02],
                uv: [0.0, 0.0],
                textured: 0.0,
            },
            Vertex {
                position: [0.018, 0.0, 0.0],
                color: [0.02, 0.02, 0.02],
                uv: [0.0, 0.0],
                textured: 0.0,
            },
            Vertex {
                position: [0.0, -0.025, 0.0],
                color: [0.02, 0.02, 0.02],
                uv: [0.0, 0.0],
                textured: 0.0,
            },
            Vertex {
                position: [0.0, 0.025, 0.0],
                color: [0.02, 0.02, 0.02],
                uv: [0.0, 0.0],
                textured: 0.0,
            },
            Vertex {
                position: [-0.010, 0.0, 0.0],
                color: [1.0, 1.0, 1.0],
                uv: [0.0, 0.0],
                textured: 0.0,
            },
            Vertex {
                position: [0.010, 0.0, 0.0],
                color: [1.0, 1.0, 1.0],
                uv: [0.0, 0.0],
                textured: 0.0,
            },
            Vertex {
                position: [0.0, -0.015, 0.0],
                color: [1.0, 1.0, 1.0],
                uv: [0.0, 0.0],
                textured: 0.0,
            },
            Vertex {
                position: [0.0, 0.015, 0.0],
                color: [1.0, 1.0, 1.0],
                uv: [0.0, 0.0],
                textured: 0.0,
            },
        ];
        let crosshair_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Centered crosshair"),
            contents: bytemuck::cast_slice(&crosshair_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let target_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Targeted voxel outline"),
            size: (36 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Build the globe's geodesic surface as the active world mesh. Nearby
        // streamed patches add detail while the whole-planet mesh remains visible.
        let planet_config = PlanetConfig {
            radius: 180.0,
            elevation_scale: 14.0,
            axial_tilt_degrees: 23.5,
        };
        let planet_world = PlanetaryWorld::new(seed, 16, planet_config)
            .map_err(|error| format!("Could not generate planetary world: {error:?}"))?;
        let topology = planet_world.topology();
        let terrain = planet_world.terrain();
        let planet_mesh = SurfaceMesh::generate(topology, terrain, 0.25);
        let mut planet_vertices = Vec::new();
        let mut planet_cell_ranges = HashMap::new();
        for triangle in planet_mesh.triangles {
            let is_pentagon = topology
                .cell(triangle.cell_id)
                .is_some_and(|cell| cell.boundary.len() == 5);
            let start =
                planet_vertices.len() as u64 * std::mem::size_of::<Vertex>() as u64;
            for vertex in triangle.vertices {
                let mut color = planet_vertex_color(vertex.sample);
                if is_pentagon {
                    color = [
                        color[0] * 0.78 + 0.22,
                        color[1] * 0.78 + 0.30,
                        color[2] * 0.78 + 0.08,
                    ];
                }
                planet_vertices.push(Vertex {
                    position: vertex.position.as_vec3().to_array(),
                    color,
                    uv: [0.0, 0.0],
                    textured: 0.0,
                });
            }
            let end =
                planet_vertices.len() as u64 * std::mem::size_of::<Vertex>() as u64;
            planet_cell_ranges
                .entry(triangle.cell_id.0)
                .and_modify(|range: &mut (u64, u64)| range.1 = end)
                .or_insert((start, end));
        }

        // Give every geodesic surface cell a visible inward-facing side wall.
        // The top remains terrain-displaced; the lower ring is inset radially so
        // adjacent cells read as hexel/pentagon prisms at ground level.
        const CELL_WALL_DEPTH: f64 = 2.4;
        for cell in topology.cells() {
            let side_color = [0.16_f32, 0.18, 0.17];
            for index in 0..cell.boundary.len() {
                let next = (index + 1) % cell.boundary.len();
                let top_a = terrain
                    .sample_direction(cell.boundary[index], 0.25)
                    .surface_position;
                let top_b = terrain
                    .sample_direction(cell.boundary[next], 0.25)
                    .surface_position;
                let bottom_a = top_a.normalize() * (top_a.length() - CELL_WALL_DEPTH).max(1.0);
                let bottom_b = top_b.normalize() * (top_b.length() - CELL_WALL_DEPTH).max(1.0);
                let mut quad = [
                    top_a.as_vec3(),
                    bottom_a.as_vec3(),
                    top_b.as_vec3(),
                    top_b.as_vec3(),
                    bottom_a.as_vec3(),
                    bottom_b.as_vec3(),
                ];
                // Orient each wall triangle consistently toward the outside.
                for tri in quad.as_chunks_mut::<3>().0 {
                    let a = tri[0];
                    let b = tri[1];
                    let c = tri[2];
                    if (b - a).cross(c - a).dot((a + b + c) / 3.0) < 0.0 {
                        tri.swap(1, 2);
                    }
                }
                for position in quad {
                    planet_vertices.push(Vertex {
                        position: position.to_array(),
                        color: side_color,
                        uv: [0.0, 0.0],
                        textured: 0.0,
                    });
                }
            }
        }

        // Draw a narrow, slightly raised ribbon along every topology edge. This
        // makes the actual pentagon/hexagon partition visible instead of showing
        // a smooth biome-colored sphere that only happens to use topology inside.
        for cell in topology.cells() {
            let pentagon = cell.boundary.len() == 5;
            let edge_color = if pentagon {
                [1.0, 0.48, 0.10]
            } else {
                [0.035, 0.075, 0.11]
            };
            for index in 0..cell.boundary.len() {
                let next = (index + 1) % cell.boundary.len();
                let a = terrain
                    .sample_direction(cell.boundary[index], 0.25)
                    .surface_position
                    .as_vec3()
                    * 1.002;
                let b = terrain
                    .sample_direction(cell.boundary[next], 0.25)
                    .surface_position
                    .as_vec3()
                    * 1.002;
                let radial = (a + b).normalize_or_zero();
                let side = (b - a).cross(radial).normalize_or_zero() * 0.035;
                push_triangle(&mut planet_vertices, a - side, b - side, b + side, edge_color);
                push_triangle(&mut planet_vertices, a - side, b + side, a + side, edge_color);
            }
        }

        let planet_vertex_count = planet_vertices.len() as u32;
        let planet_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Procedural planetary world surface"),
            contents: bytemuck::cast_slice(&planet_vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let (depth_texture, depth_view) = create_depth_target(&device, config.width, config.height);
        let (mesh_job_tx, mesh_job_rx) = mpsc::channel::<MeshJob>();
        let (mesh_result_tx, mesh_result_rx) = mpsc::channel::<MeshResult>();
        thread::Builder::new()
            .name("hexel-mesh-worker".into())
            .spawn(move || {
                while let Ok(mut job) = mesh_job_rx.recv() {
                    while let Ok(newer) = mesh_job_rx.try_recv() {
                        job = newer;
                    }
                    let chunks = job
                        .chunks
                        .iter()
                        .map(|pos| (*pos, build_chunk_mesh(job.seed, &job.edits, *pos)))
                        .collect();
                    if mesh_result_tx
                        .send(MeshResult {
                            revision: job.revision,
                            chunks,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| format!("Could not start mesh worker: {e}"))?;
        let renderer = Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            outline_pipeline,
            overlay_pipeline,
            crosshair_buffer,
            target_buffer,
            planet_vertex_buffer,
            planet_vertex_count,
            planet_vertices,
            planet_cell_ranges,
            planet_world,
            planet_stream_chunks: HashMap::new(),
            planet_stream_pending: VecDeque::new(),
            chunks: HashMap::new(),
            camera_buffer,
            camera_bind_group,
            foliage_bind_group,
            size,
            _depth_texture: depth_texture,
            depth_view,
            seed,
            edits: HashMap::new(),
            mesh_job_tx,
            mesh_result_rx,
            dirty_chunks: HashSet::new(),
            edit_revision: 0,
            pending_mesh_chunks: VecDeque::new(),
            pending_mesh_count: 0,
            pending_mesh_revision: 0,
        };
        Ok(renderer)
    }

    pub fn cell_from_world(position: Vec3) -> CellPos {
        let scale = WorldScale::default();
        let r_float = position.z / (scale.hex_radius * 1.5);
        let q_float = position.x / (scale.hex_radius * 3.0_f32.sqrt()) - r_float * 0.5;

        // Axial hex coordinates must be rounded together in cube space. Rounding q
        // and r independently misclassifies points near hex corners, which can make
        // both collision samples and block targeting select the wrong column.
        let s_float = -q_float - r_float;
        let mut q = q_float.round();
        let mut r = r_float.round();
        let s = s_float.round();
        let q_error = (q - q_float).abs();
        let r_error = (r - r_float).abs();
        let s_error = (s - s_float).abs();
        if q_error > r_error && q_error > s_error {
            q = -r - s;
        } else if r_error > s_error {
            r = -q - s;
        }
        CellPos::new(
            q as i32,
            r as i32,
            (position.y / scale.hexel_height).floor() as i32,
        )
    }
    pub fn world_seed(&self) -> u32 {
        self.seed
    }

    pub fn flat_edits_snapshot(&self) -> Vec<(CellPos, BlockId)> {
        let mut edits = self.edits.iter().map(|(cell, block)| (*cell, *block)).collect::<Vec<_>>();
        edits.sort_by_key(|(cell, _)| (cell.q, cell.r, cell.y));
        edits
    }

    pub fn restore_flat_edits(&mut self, edits: &[(CellPos, BlockId)]) {
        for (cell, block) in edits {
            self.edit_block(*cell, *block);
        }
    }

    pub fn planet_edits_snapshot(&self) -> Vec<(u32, BlockId)> {
        self.planet_world
            .save()
            .edits
            .into_iter()
            .map(|(id, block)| (id.0, block))
            .collect()
    }

    pub fn restore_planet_edits(&mut self, edits: &[(u32, BlockId)]) {
        for (id, block) in edits {
            self.edit_planet_cell(*id, *block);
        }
    }

    pub fn block_at(&self, cell: CellPos) -> BlockId {
        self.edits.get(&cell).copied().unwrap_or_else(|| {
            generated_block_from_height(cell, terrain_height(self.seed, cell.q, cell.r))
        })
    }

    /// Text inspection for the "What am I looking at?" debug/gameplay affordance.
    #[allow(dead_code)]
    pub fn describe_target(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> String {
        let Some((cell, _)) = self.raycast(origin, direction, max_distance) else {
            return "What am I looking at? Nothing identifiable in reach.".to_string();
        };
        let block = self.block_at(cell);
        let surface = terrain_height(self.seed, cell.q, cell.r);
        let layer = surface - cell.y;
        let (name, detail) = match block {
            BlockId::Grass => (
                "Grass-covered ground",
                "Surface cover over a soil layer. Soil fertility is not yet wired into the playable farming loop.",
            ),
            BlockId::Dirt if layer <= 3 => (
                "Topsoil",
                "Loose near-surface soil. Its color varies by procedural soil patch; nutrient state is not yet persisted per cell.",
            ),
            BlockId::Dirt => ("Subsoil", "Weathered material beneath the topsoil."),
            BlockId::Stone => match cell.y.rem_euclid(11) {
                0..=2 => (
                    "Sedimentary-looking rock",
                    "Pale, layered rock appearance; exact rock strata are not yet connected to world generation.",
                ),
                3..=5 => (
                    "Gray bedrock",
                    "Hard bedrock. Mining deposits and host-rock rules are not yet spawned into playable chunks.",
                ),
                6..=8 => (
                    "Tan bedrock",
                    "Weathered-looking bedrock; the geology module is currently a data foundation.",
                ),
                _ => (
                    "Dark bedrock",
                    "Dense-looking rock. Exact mineral identity and finite deposit state are not yet integrated.",
                ),
            },
            BlockId::Foundation => (
                "Unbreakable foundation",
                "Below the playable terrain floor.",
            ),
            BlockId::Air => ("Air", "An empty cell."),
        };
        format!(
            "What am I looking at?\\n{name}\\n{detail}\\nCell: q={}, r={}, y={} | Surface layer: {}",
            cell.q,
            cell.r,
            cell.y,
            layer.max(0)
        )
    }
    pub fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(CellPos, CellPos)> {
        let scale = WorldScale::default();
        let dir = direction.normalize_or_zero();
        if dir == Vec3::ZERO {
            return None;
        }

        // Walk the ray to find candidate cells, then use the exact prism intersection
        // face to choose the adjacent cell. The last sampled air cell can be diagonally
        // adjacent or even on the wrong layer near hex corners.
        let mut distance = 0.0;
        while distance <= max_distance {
            let cell = Self::cell_from_world(origin + dir * distance);
            if self.block_at(cell).is_solid() {
                let center = scale.prism_center(cell);
                if let Some((hit_distance, face)) = raycast_prism(
                    origin,
                    dir,
                    center,
                    scale.hex_radius,
                    scale.hexel_height,
                    max_distance,
                ) && hit_distance <= distance + 0.15
                {
                    let placement = match face {
                        0 => CellPos::new(cell.q + 1, cell.r, cell.y),
                        1 => CellPos::new(cell.q, cell.r + 1, cell.y),
                        2 => CellPos::new(cell.q - 1, cell.r + 1, cell.y),
                        3 => CellPos::new(cell.q - 1, cell.r, cell.y),
                        4 => CellPos::new(cell.q, cell.r - 1, cell.y),
                        5 => CellPos::new(cell.q + 1, cell.r - 1, cell.y),
                        6 => CellPos::new(cell.q, cell.r, cell.y + 1),
                        7 => CellPos::new(cell.q, cell.r, cell.y - 1),
                        _ => cell,
                    };
                    return Some((cell, placement));
                }
            }
            distance += 0.045;
        }
        None
    }
    pub fn edit_block(&mut self, cell: CellPos, block: BlockId) {
        if cell.y < 0 || cell.y > 255 {
            return;
        }
        if self.block_at(cell).is_indestructible() {
            return;
        }
        self.edits.insert(cell, block);

        // Invalidate the chunk containing this cell and every chunk containing one
        // of its actual neighbors. Axial hex neighbors can cross two chunk axes at
        // once, so checking q/r boundaries independently misses diagonal chunks.
        let affected = affected_chunks_for_cell(cell);
        self.dirty_chunks.extend(
            affected
                .iter()
                .copied()
                .filter(|chunk| self.chunks.contains_key(chunk)),
        );
        self.edit_revision = self.edit_revision.wrapping_add(1);

        // The worker intentionally coalesces queued jobs. Always include the union
        // of dirty loaded chunks so an earlier edit cannot be lost when its job is
        // superseded by a later edit.
        let mut chunks = self
            .dirty_chunks
            .iter()
            .copied()
            .filter(|chunk| self.chunks.contains_key(chunk))
            .collect::<Vec<_>>();
        chunks.sort_by_key(|p| (p.q, p.r, p.y));
        let _ = self.mesh_job_tx.send(MeshJob {
            revision: self.edit_revision,
            seed: self.seed,
            edits: self.edits.clone(),
            chunks,
        });
    }
    /// Active meshes are bounded to a moving neighborhood; edits persist separately.
    pub fn update_streaming(&mut self, player_position: Vec3) {
        let cell = Self::cell_from_world(player_position);
        let center = ChunkPos {
            q: cell.q.div_euclid(CHUNK_SIZE),
            r: cell.r.div_euclid(CHUNK_SIZE),
            y: cell.y.max(0).div_euclid(CHUNK_SIZE),
        };
        let mut wanted = HashSet::new();
        for dq in -STREAM_RADIUS_CHUNKS..=STREAM_RADIUS_CHUNKS {
            for dr in -STREAM_RADIUS_CHUNKS..=STREAM_RADIUS_CHUNKS {
                if dq * dq + dr * dr > STREAM_RADIUS_CHUNKS * STREAM_RADIUS_CHUNKS {
                    continue;
                }
                for dy in -1..=1 {
                    let cy = center.y + dy;
                    if cy >= 0 {
                        wanted.insert(ChunkPos {
                            q: center.q + dq,
                            r: center.r + dr,
                            y: cy,
                        });
                    }
                }
            }
        }
        self.chunks.retain(|pos, _| wanted.contains(pos));
        let missing: Vec<_> = wanted
            .iter()
            .copied()
            .filter(|pos| !self.chunks.contains_key(pos))
            .collect();
        for pos in missing {
            let mesh = self.create_chunk(pos);
            self.chunks.insert(pos, mesh);
        }
    }

    fn poll_mesh_results(&mut self) {
        while let Ok(result) = self.mesh_result_rx.try_recv() {
            if result.revision != self.edit_revision {
                continue;
            }
            let revision = result.revision;
            self.pending_mesh_chunks.clear();
            self.pending_mesh_chunks.extend(
                result
                    .chunks
                    .into_iter()
                    .map(|(index, vertices)| (revision, index, vertices)),
            );
            self.pending_mesh_count = self.pending_mesh_chunks.len();
            self.pending_mesh_revision = revision;
        }

        if self.pending_mesh_revision != self.edit_revision {
            self.pending_mesh_chunks.clear();
            self.pending_mesh_count = 0;
            return;
        }

        // Upload at most one rebuilt chunk per frame. CPU meshing is already on
        // the worker; spreading GPU buffer creation avoids multi-chunk edit spikes.
        if let Some((revision, index, vertices)) = self.pending_mesh_chunks.pop_front()
            && revision == self.edit_revision
        {
            let vertex_count = vertices.len() as u32;
            let buffer = if vertices.is_empty() {
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Empty hexel chunk"),
                    size: 4,
                    usage: wgpu::BufferUsages::VERTEX,
                    mapped_at_creation: false,
                })
            } else {
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Updated hexel chunk mesh"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    })
            };
            if let Some(chunk) = self.chunks.get_mut(&index) {
                chunk.buffer = buffer;
                chunk.vertex_count = vertex_count;
            }
            self.pending_mesh_count = self.pending_mesh_count.saturating_sub(1);
            if self.pending_mesh_count == 0 {
                self.dirty_chunks.clear();
            }
        }
    }
    fn create_chunk(&self, pos: ChunkPos) -> ChunkMesh {
        let vertices = build_chunk_mesh(self.seed, &self.edits, pos);
        let vertex_count = vertices.len() as u32;
        let buffer = if vertices.is_empty() {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Empty hexel chunk"),
                size: 4,
                usage: wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            })
        } else {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Hexel chunk mesh"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        };
        ChunkMesh {
            buffer,
            vertex_count,
        }
    }
    pub fn planet_block_at(&self, id: u32) -> BlockId {
        self.planet_world
            .block_at(hexel_engine::topology::SurfaceCellId(id))
            .unwrap_or(BlockId::Air)
    }

    pub fn edit_planet_cell(&mut self, id: u32, block: BlockId) {
        if !self.planet_cell_ranges.contains_key(&id) || block == BlockId::Foundation {
            return;
        }
        if self
            .planet_world
            .set_block(hexel_engine::topology::SurfaceCellId(id), block)
            .is_err()
        {
            return;
        }
        let Some((start, end)) = self.planet_cell_ranges.get(&id).copied() else {
            return;
        };
        let color = planet_block_color(block);
        let vertex_size = std::mem::size_of::<Vertex>() as u64;
        let first = (start / vertex_size) as usize;
        let end_index = (end / vertex_size) as usize;
        for vertex in &mut self.planet_vertices[first..end_index] {
            let direction = Vec3::from_array(vertex.position).normalize_or_zero();
            let radial = DVec3::new(
                direction.x as f64,
                direction.y as f64,
                direction.z as f64,
            );
            let sample = self.planet_world.terrain().sample_direction(radial, 0.25);
            let position = if block == BlockId::Air {
                sample.surface_position - sample.direction * 2.4
            } else {
                sample.surface_position
            };
            vertex.position = position.as_vec3().to_array();
            vertex.color = color;
        }
        self.queue.write_buffer(
            &self.planet_vertex_buffer,
            start,
            bytemuck::cast_slice(&self.planet_vertices[first..end_index]),
        );
        self.planet_stream_chunks.clear();
        self.planet_stream_pending.clear();
    }

    /// Collision against trunks generated by the same biome/hash rule as the
    /// streamed planetary vegetation meshes.
    pub fn planet_tree_blocks_player(
        &self,
        position: Vec3,
        player_radius: f32,
        player_height: f32,
    ) -> bool {
        let radial = DVec3::new(
            position.x as f64,
            position.y as f64,
            position.z as f64,
        )
        .normalize_or_zero();
        let Some(current_id) = self.planet_world.topology().cell_id_for_direction(radial) else {
            return false;
        };
        let Some(current_cell) = self.planet_world.topology().cell(current_id) else {
            return false;
        };
        let mut candidates = HashSet::new();
        candidates.insert(current_id.0);
        candidates.extend(current_cell.neighbors.iter().map(|id| id.0));
        for id in candidates {
            if self.planet_world.edited_cells().get(&hexel_engine::topology::SurfaceCellId(id)) == Some(&BlockId::Air) {
                continue;
            }
            let Some(cell) = self.planet_world.topology().cell(
                hexel_engine::topology::SurfaceCellId(id),
            ) else {
                continue;
            };
            let sample = self.planet_world.terrain().sample_direction(cell.center, 0.25);
            let hash = vegetation_hash(
                self.seed ^ id.wrapping_mul(0x27D4_EB2D),
                id as i32,
                id.rotate_left(13) as i32,
            );
            let Some(spec) = planet_tree_spec(sample.biome, hash) else {
                continue;
            };
            let up = sample.direction;
            let delta = DVec3::new(
                position.x as f64,
                position.y as f64,
                position.z as f64,
            ) - sample.surface_position;
            let vertical = delta.dot(up);
            let tangent = delta - up * vertical;
            let trunk_top = spec.height_layers as f64
                * WorldScale::default().hexel_height as f64
                * 2.0
                / 3.0;
            if tangent.length() < player_radius as f64 + 0.48
                && vertical + player_height as f64 > 0.0
                && vertical < trunk_top
            {
                return true;
            }
        }
        false
    }

    pub fn planet_surface_position(&self, direction: Vec3) -> Vec3 {
        let radial = DVec3::new(
            direction.x as f64,
            direction.y as f64,
            direction.z as f64,
        )
        .normalize_or_zero();
        let sample = self.planet_world.terrain().sample_direction(radial, 0.25);
        let block = self
            .planet_world.topology()
            .cell_id_for_direction(radial)
            .map(|id| self.planet_block_at(id.0));
        let surface = if block == Some(BlockId::Air) {
            sample.surface_position - sample.direction * 2.4
        } else {
            sample.surface_position
        };
        surface.as_vec3()
    }

    pub fn planet_cell_at_ray(&self, origin: Vec3, direction: Vec3) -> Option<u32> {
        let dir = direction.normalize_or_zero();
        if dir == Vec3::ZERO { return None; }
        // Intersect a local-radius sphere: terrain elevation varies around the
        // globe, so the configured base radius can put an ocean-level player inside
        // the ideal sphere and incorrectly select the far side.
        let local_radius = self.planet_world.terrain()
            .sample_direction(DVec3::new(origin.x as f64, origin.y as f64, origin.z as f64), 0.25)
            .surface_position.length() as f32;
        let b = origin.dot(dir);
        let c = origin.length_squared() - local_radius * local_radius;
        let discriminant = b * b - c;
        if discriminant < 0.0 { return None; }
        let root = discriminant.sqrt();
        let near = -b - root;
        let far = -b + root;
        let distance = if near >= 0.0 { near } else if far >= 0.0 { far } else { return None; };
        let hit = origin + dir * distance;
        self.planet_world.topology().cell_id_for_direction(DVec3::new(hit.x as f64, hit.y as f64, hit.z as f64)).map(|id| id.0)
    }

    fn build_planet_cell_outline(&self, id: u32) -> Vec<Vertex> {
        let Some(cell) = self.planet_world.topology().cell(hexel_engine::topology::SurfaceCellId(id)) else { return Vec::new(); };
        let mut vertices = Vec::with_capacity(cell.boundary.len() * 2);
        for i in 0..cell.boundary.len() {
            let a = self.planet_world.terrain().sample_direction(cell.boundary[i], 0.25).surface_position.as_vec3() * 1.006;
            let b = self.planet_world.terrain().sample_direction(cell.boundary[(i + 1) % cell.boundary.len()], 0.25).surface_position.as_vec3() * 1.006;
            for point in [a, b] {
                vertices.push(Vertex { position: point.to_array(), color: [1.0, 0.92, 0.22], uv: [0.0, 0.0], textured: 0.0 });
            }
        }
        vertices
    }

    /// Keep detailed cube-sphere terrain patches around the player's current location.
    /// The low-detail whole-planet mesh remains as a distant backdrop while these
    /// camera-near patches are rebuilt only when the desired neighborhood changes.
    fn update_planet_streaming(&mut self, player_position: Vec3) {
        const CHUNKS_PER_FACE: u32 = 24;
        const MESH_RESOLUTION: u32 = 10;
        const ANGULAR_RADIUS_COS: f64 = 0.9637708963658905; // cos(0.27 radians)
        const MAX_NEW_CHUNKS_PER_FRAME: usize = 3;

        let radial = DVec3::new(
            player_position.x as f64,
            player_position.y as f64,
            player_position.z as f64,
        )
        .normalize_or_zero();
        if radial == DVec3::ZERO {
            return;
        }

        let mut wanted = HashSet::new();
        let mut prioritized = Vec::new();
        for face in [
            CubeFace::PositiveX,
            CubeFace::NegativeX,
            CubeFace::PositiveY,
            CubeFace::NegativeY,
            CubeFace::PositiveZ,
            CubeFace::NegativeZ,
        ] {
            let (normal, axis_u, axis_v) = face.basis();
            for y in 0..CHUNKS_PER_FACE {
                let v = -1.0 + 2.0 * (y as f64 + 0.5) / CHUNKS_PER_FACE as f64;
                for x in 0..CHUNKS_PER_FACE {
                    let u = -1.0 + 2.0 * (x as f64 + 0.5) / CHUNKS_PER_FACE as f64;
                    let direction = (normal + axis_u * u + axis_v * v).normalize();
                    let dot = direction.dot(radial);
                    if dot >= ANGULAR_RADIUS_COS {
                        let key = PlanetChunkKey { face, x, y };
                        wanted.insert(key);
                        prioritized.push((dot, key));
                    }
                }
            }
        }

        self.planet_stream_chunks.retain(|key, _| wanted.contains(key));
        self.planet_stream_pending.retain(|key| {
            wanted.contains(key) && !self.planet_stream_chunks.contains_key(key)
        });

        prioritized.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, key) in prioritized {
            if !self.planet_stream_chunks.contains_key(&key)
                && !self.planet_stream_pending.contains(&key)
            {
                self.planet_stream_pending.push_back(key);
            }
        }

        for _ in 0..MAX_NEW_CHUNKS_PER_FRAME {
            let Some(key) = self.planet_stream_pending.pop_front() else {
                break;
            };
            if !wanted.contains(&key) || self.planet_stream_chunks.contains_key(&key) {
                continue;
            }
            let Ok(mesh) = PlanetChunkMesh::generate(
                self.planet_world.terrain(),
                key,
                CHUNKS_PER_FACE,
                MESH_RESOLUTION,
                0.25,
            ) else {
                continue;
            };
            // Resolve cell ownership once per patch-grid vertex, not once per
            // expanded triangle index. Nearest-cell lookup is linear in topology
            // size, so this avoids repeating that work for shared mesh vertices.
            let mut grid_vertices = Vec::with_capacity(mesh.vertices.len());
            for source in &mesh.vertices {
                let sample = self.planet_world.terrain().sample_direction(source.normal, 0.25);
                let cell_block = self
                    .planet_world.topology()
                    .cell_id_for_direction(source.normal)
                    .map(|id| self.planet_block_at(id.0));
                let surface_position = if cell_block == Some(BlockId::Air) {
                    source.position - source.normal * 2.4
                } else {
                    source.position
                };
                let position = surface_position + source.normal * 0.035;
                let color = cell_block
                    .map(planet_block_color)
                    .unwrap_or_else(|| planet_vertex_color(sample));
                grid_vertices.push(Vertex {
                    position: position.as_vec3().to_array(),
                    color,
                    uv: [0.0, 0.0],
                    textured: 0.0,
                });
            }
            let mut vertices = mesh
                .indices
                .iter()
                .map(|index| grid_vertices[*index as usize])
                .collect::<Vec<_>>();
            append_planetary_trees(
                &mut vertices,
                key,
                CHUNKS_PER_FACE,
                self.planet_world.topology(),
                &self.planet_world.terrain(),
                self.seed,
                self.planet_world.edited_cells(),
            );
            if vertices.is_empty() {
                continue;
            }
            let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Streamed planetary terrain chunk"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.planet_stream_chunks.insert(
                key,
                PlanetGpuChunk {
                    buffer,
                    vertex_count: vertices.len() as u32,
                },
            );
        }
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        let (depth_texture, depth_view) =
            create_depth_target(&self.device, self.config.width, self.config.height);
        self._depth_texture = depth_texture;
        self.depth_view = depth_view;
    }
    pub fn render(
        &mut self,
        camera: &Camera,
        selected_slot: usize,
        inventory_open: bool,
        item_counts: [u16; 36],
    ) -> Result<(), wgpu::SurfaceError> {
        self.poll_mesh_results();
        self.update_planet_streaming(camera.position);
        let view_proj = camera.view_projection(self.size);
        let uniform = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
        let target_vertices = self
            .planet_cell_at_ray(camera.position, camera.forward())
            .map(|id| self.build_planet_cell_outline(id))
            .unwrap_or_default();
        if !target_vertices.is_empty() {
            self.queue.write_buffer(
                &self.target_buffer,
                0,
                bytemuck::cast_slice(&target_vertices),
            );
        }
        let mut hud_vertices: Vec<Vertex> = Vec::with_capacity(600);
        let aspect = self.size.width.max(1) as f32 / self.size.height.max(1) as f32;
        let mut hud_line = |x1: f32, y1: f32, x2: f32, y2: f32, color: [f32; 3]| {
            hud_vertices.push(Vertex {
                position: [x1, y1, 0.0],
                color,
                uv: [0.0, 0.0],
                textured: 0.0,
            });
            hud_vertices.push(Vertex {
                position: [x2, y2, 0.0],
                color,
                uv: [0.0, 0.0],
                textured: 0.0,
            });
        };
        let sw = (0.075 / aspect).min(0.085);
        let sh = 0.105;
        let gap = 0.012 / aspect;
        let left = -(9.0 * sw + 8.0 * gap) * 0.5;
        let bottom = -0.91;
        for (i, count) in item_counts.iter().enumerate().take(9) {
            let x1 = left + i as f32 * (sw + gap);
            let x2 = x1 + sw;
            let y1 = bottom;
            let y2 = bottom + sh;
            let col = if i == selected_slot {
                [1.0, 0.78, 0.12]
            } else {
                [0.95, 0.97, 1.0]
            };
            hud_line(x1, y1, x2, y1, col);
            hud_line(x2, y1, x2, y2, col);
            hud_line(x2, y2, x1, y2, col);
            hud_line(x1, y2, x1, y1, col);
            for tick in 0..(*count).min(9) {
                let tx = x1 + 0.012 / aspect + tick as f32 * 0.005 / aspect;
                hud_line(tx, y1 + 0.012, tx, y1 + 0.028, [0.4, 1.0, 0.6]);
            }
        }
        if inventory_open {
            let w = (0.055 / aspect).min(0.065);
            let h = 0.075;
            let gx = 0.009 / aspect;
            let gy = 0.018;
            let start_x = -(9.0 * w + 8.0 * gx) * 0.5;
            let start_y = 0.48;
            for (i, count) in item_counts.iter().enumerate() {
                let x1 = start_x + (i % 9) as f32 * (w + gx);
                let y1 = start_y - (i / 9) as f32 * (h + gy);
                let x2 = x1 + w;
                let y2 = y1 + h;
                let col = if i == selected_slot {
                    [1.0, 0.78, 0.12]
                } else {
                    [0.88, 0.92, 0.98]
                };
                hud_line(x1, y1, x2, y1, col);
                hud_line(x2, y1, x2, y2, col);
                hud_line(x2, y2, x1, y2, col);
                hud_line(x1, y2, x1, y1, col);
                for tick in 0..(*count).min(5) {
                    let tx = x1 + 0.008 / aspect + tick as f32 * 0.004 / aspect;
                    hud_line(tx, y1 + 0.008, tx, y1 + 0.02, [0.4, 1.0, 0.6]);
                }
            }
        }
        drop(hud_line);
        let hud_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("HUD overlay"),
                contents: bytemuck::cast_slice(&hud_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Hexel frame encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Hexel world pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.008,
                            g: 0.012,
                            b: 0.035,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.foliage_bind_group, &[]);
            pass.set_vertex_buffer(0, self.planet_vertex_buffer.slice(..));
            pass.draw(0..self.planet_vertex_count, 0..1);
            for chunk in self.planet_stream_chunks.values() {
                if chunk.vertex_count == 0 { continue; }
                pass.set_vertex_buffer(0, chunk.buffer.slice(..));
                pass.draw(0..chunk.vertex_count, 0..1);
            }
            if !target_vertices.is_empty() {
                pass.set_pipeline(&self.outline_pipeline);
                pass.set_vertex_buffer(0, self.target_buffer.slice(..));
                pass.draw(0..target_vertices.len() as u32, 0..1);
            }
        }
        {
            let mut overlay = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Crosshair overlay pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            overlay.set_pipeline(&self.overlay_pipeline);
            overlay.set_vertex_buffer(0, self.crosshair_buffer.slice(..));
            overlay.draw(0..4, 0..1);
            overlay.draw(4..8, 0..1);
            // The survival HUD is part of the single planetary world too.
            overlay.set_vertex_buffer(0, hud_buffer.slice(..));
            overlay.draw(0..hud_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct PlanetTreeSpec {
    species: u8,
    height_layers: i32,
    trunk_radius: i32,
    crown_radius: i32,
    trunk_color: [f32; 3],
    foliage_color: [f32; 3],
    leaf_layer: u32,
    bark_layer: u32,
}

fn planet_tree_spec(biome: Biome, hash: u32) -> Option<PlanetTreeSpec> {
    let (frequency, species, min_height, height_span, crown_radius, foliage_color, leaf_layer, bark_layer) =
        match biome {
            Biome::BorealForest => (3, 2, 10, 9, 2, [0.12, 0.30, 0.15], 8, 23),
            Biome::TemperateForest => (3, 0, 9, 8, 3, [0.18, 0.46, 0.15], 7, 22),
            Biome::TropicalForest => (2, 3, 13, 9, 3, [0.10, 0.39, 0.13], 5, 19),
            Biome::Grassland => (9, 6, 6, 5, 2, [0.30, 0.39, 0.17], 6, 21),
            Biome::Savanna => (12, 3, 7, 7, 2, [0.18, 0.46, 0.15], 0, 15),
            _ => return None,
        };
    if !hash.is_multiple_of(frequency) {
        return None;
    }
    Some(PlanetTreeSpec {
        species,
        height_layers: min_height + (hash.rotate_left(11) % height_span) as i32,
        trunk_radius: 1,
        crown_radius,
        trunk_color: if species == 1 { [0.78, 0.75, 0.65] } else { [0.38, 0.25, 0.14] },
        foliage_color,
        leaf_layer,
        bark_layer,
    })
}

fn planetary_chunk_owns_direction(
    direction: DVec3,
    key: PlanetChunkKey,
    chunks_per_face: u32,
) -> bool {
    let direction = direction.normalize_or_zero();
    if direction == DVec3::ZERO || chunks_per_face == 0 {
        return false;
    }
    let faces = [
        CubeFace::PositiveX,
        CubeFace::NegativeX,
        CubeFace::PositiveY,
        CubeFace::NegativeY,
        CubeFace::PositiveZ,
        CubeFace::NegativeZ,
    ];
    let mut selected_face = CubeFace::PositiveX;
    let mut best_dot = f64::NEG_INFINITY;
    for face in faces {
        let (normal, _, _) = face.basis();
        let dot = direction.dot(normal);
        if dot > best_dot {
            best_dot = dot;
            selected_face = face;
        }
    }
    if selected_face != key.face {
        return false;
    }
    let (normal, axis_u, axis_v) = key.face.basis();
    let denominator = direction.dot(normal);
    if denominator <= 0.0 {
        return false;
    }
    let u = direction.dot(axis_u) / denominator;
    let v = direction.dot(axis_v) / denominator;
    let grid = chunks_per_face as f64;
    let x = (((u + 1.0) * 0.5 * grid).floor() as i64)
        .clamp(0, chunks_per_face as i64 - 1) as u32;
    let y = (((v + 1.0) * 0.5 * grid).floor() as i64)
        .clamp(0, chunks_per_face as i64 - 1) as u32;
    x == key.x && y == key.y
}

fn append_planetary_trees(
    vertices: &mut Vec<Vertex>,
    key: PlanetChunkKey,
    chunks_per_face: u32,
    topology: &SurfaceTopology,
    terrain: &PlanetGenerator,
    seed: u32,
    edits: &std::collections::BTreeMap<hexel_engine::topology::SurfaceCellId, BlockId>,
) {
    let scale = WorldScale::default();
    for cell in topology.cells() {
        if !planetary_chunk_owns_direction(cell.center, key, chunks_per_face)
            || edits.get(&cell.id) == Some(&BlockId::Air)
        {
            continue;
        }
        let sample = terrain.sample_direction(cell.center, 0.25);
        let hash = vegetation_hash(
            seed ^ cell.id.0.wrapping_mul(0x27D4_EB2D),
            cell.id.0 as i32,
            cell.id.0.rotate_left(13) as i32,
        );
        let Some(spec) = planet_tree_spec(sample.biome, hash) else {
            continue;
        };
        let mut tree = Vec::new();
        append_hexel_tree(
            &mut tree,
            CellPos::new(0, 0, 0),
            spec.height_layers,
            spec.trunk_radius,
            spec.crown_radius,
            (spec.trunk_color, spec.foliage_color),
            spec.species,
            hash,
            spec.leaf_layer,
            spec.bark_layer,
            scale,
        );
        let frame = PlanetaryFrame::at(sample.surface_position);
        for mut vertex in tree {
            let local = Vec3::from_array(vertex.position);
            let planet_position = frame.to_planet(DVec3::new(
                local.x as f64,
                local.y as f64,
                local.z as f64,
            ));
            vertex.position = planet_position.as_vec3().to_array();
            vertices.push(vertex);
        }
    }
}

fn planet_block_color(block: BlockId) -> [f32; 3] {
    match block {
        BlockId::Air => [0.025, 0.055, 0.10],
        BlockId::Grass => [0.20, 0.62, 0.22],
        BlockId::Dirt => [0.48, 0.29, 0.14],
        BlockId::Stone => [0.42, 0.45, 0.49],
        BlockId::Foundation => [0.75, 0.75, 0.78],
    }
}

fn planet_vertex_color(sample: PlanetSample) -> [f32; 3] {
    match sample.biome {
        Biome::DeepOcean => [0.025, 0.10, 0.32],
        Biome::ShallowOcean => [0.06, 0.30, 0.56],
        Biome::PolarIce => [0.88, 0.93, 0.96],
        Biome::Tundra => [0.54, 0.62, 0.53],
        Biome::BorealForest => [0.12, 0.32, 0.19],
        Biome::TemperateForest => [0.12, 0.48, 0.24],
        Biome::Grassland => [0.48, 0.65, 0.28],
        Biome::Desert => [0.78, 0.63, 0.35],
        Biome::Savanna => [0.69, 0.61, 0.28],
        Biome::TropicalForest => [0.04, 0.38, 0.17],
        Biome::Mountain => [0.47, 0.46, 0.43],
    }
}

fn generated_block_from_height(cell: CellPos, height: i32) -> BlockId {
    if cell.y < 0 {
        BlockId::Foundation
    } else if cell.y > height {
        BlockId::Air
    } else if cell.y == height {
        BlockId::Grass
    } else if cell.y >= height - 3 {
        BlockId::Dirt
    } else {
        BlockId::Stone
    }
}

fn create_depth_target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Reusable frame depth buffer"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth24Plus,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
fn affected_chunks_for_cell(cell: CellPos) -> Vec<ChunkPos> {
    let mut affected = cell
        .neighbors()
        .into_iter()
        .map(|neighbor| neighbor.chunk_and_local().0)
        .collect::<Vec<_>>();
    affected.push(cell.chunk_and_local().0);
    affected.sort_by_key(|p| (p.q, p.r, p.y));
    affected.dedup();
    affected
}

fn build_chunk_mesh(seed: u32, edits: &HashMap<CellPos, BlockId>, chunk: ChunkPos) -> Vec<Vertex> {
    let scale = WorldScale::default();
    let q_start = chunk.q * CHUNK_SIZE;
    let r_start = chunk.r * CHUNK_SIZE;
    let y_start = chunk.y * CHUNK_SIZE;
    let q_end = q_start + CHUNK_SIZE;
    let r_end = r_start + CHUNK_SIZE;
    let y_end = y_start + CHUNK_SIZE;
    let mut vertices = Vec::with_capacity(70_000);
    let noise = Perlin::new(seed);
    let mut heights = HashMap::with_capacity(((CHUNK_SIZE + 2) * (CHUNK_SIZE + 2)) as usize);
    for q in (q_start - 1)..=q_end {
        for r in (r_start - 1)..=r_end {
            let height = (8.0 + noise.get([q as f64 / 28.0, r as f64 / 28.0]) * 5.0).round() as i32;
            heights.insert((q, r), height);
        }
    }
    let block_at = |cell: CellPos| {
        edits.get(&cell).copied().unwrap_or_else(|| {
            if cell.y < 0 {
                return BlockId::Foundation;
            }
            let height = *heights.get(&(cell.q, cell.r)).unwrap_or(&0);
            if cell.y > height {
                BlockId::Air
            } else if cell.y == height {
                BlockId::Grass
            } else if cell.y >= height - 3 {
                BlockId::Dirt
            } else {
                BlockId::Stone
            }
        })
    };
    let mut edited_tops = HashMap::<(i32, i32), i32>::new();
    for cell in edits.keys() {
        if (q_start..q_end).contains(&cell.q) && (r_start..r_end).contains(&cell.r) {
            edited_tops
                .entry((cell.q, cell.r))
                .and_modify(|top| *top = (*top).max(cell.y))
                .or_insert(cell.y);
        }
    }
    for q in q_start..q_end {
        for r in r_start..r_end {
            let terrain_top = *heights.get(&(q, r)).unwrap_or(&0);
            let edited_top = edited_tops.get(&(q, r)).copied().unwrap_or(terrain_top);
            let min_y = y_start.max(0);
            let max_y = y_end.min(
                (terrain_top + MAX_EDIT_HEIGHT)
                    .max(edited_top)
                    .max(terrain_top)
                    .min(256)
                    + 1,
            );
            for y in min_y..max_y {
                let cell = CellPos::new(q, r, y);
                let block = block_at(cell);
                if block == BlockId::Air {
                    continue;
                }
                let center = scale.prism_center(cell);
                let bottom_y = y as f32 * scale.hexel_height;
                let top_y = (y + 1) as f32 * scale.hexel_height;
                let ring = std::array::from_fn::<_, 6, _>(|i| {
                    let angle = (30.0_f32 + i as f32 * 60.0).to_radians();
                    Vec3::new(
                        center.x + angle.cos() * scale.hex_radius,
                        center.y,
                        center.z + angle.sin() * scale.hex_radius,
                    )
                });
                let base = block_color_at(block, cell, seed);
                if !block_at(CellPos::new(q, r, y + 1)).is_solid() {
                    let top = [
                        (base[0] * 1.12).min(1.0),
                        (base[1] * 1.12).min(1.0),
                        (base[2] * 1.12).min(1.0),
                    ];
                    for i in 0..6 {
                        push_triangle(
                            &mut vertices,
                            Vec3::new(center.x, top_y, center.z),
                            Vec3::new(ring[i].x, top_y, ring[i].z),
                            Vec3::new(ring[(i + 1) % 6].x, top_y, ring[(i + 1) % 6].z),
                            top,
                        );
                    }
                }
                let neighbors = [(0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1), (1, 0)];
                for (i, (dq, dr)) in neighbors.into_iter().enumerate() {
                    let nq = q + dq;
                    let nr = r + dr;
                    if block_at(CellPos::new(nq, nr, y)).is_solid() {
                        continue;
                    }
                    let a = ring[i];
                    let b = ring[(i + 1) % 6];
                    let shade = 0.48 + ((i * 7 % 6) as f32) * 0.055;
                    let side = [base[0] * shade, base[1] * shade, base[2] * shade];
                    push_triangle(
                        &mut vertices,
                        Vec3::new(a.x, bottom_y, a.z),
                        Vec3::new(a.x, top_y, a.z),
                        Vec3::new(b.x, top_y, b.z),
                        side,
                    );
                    push_triangle(
                        &mut vertices,
                        Vec3::new(a.x, bottom_y, a.z),
                        Vec3::new(b.x, top_y, b.z),
                        Vec3::new(b.x, bottom_y, b.z),
                        side,
                    );
                }
            }
        }
    }
    // Coordinate-hashed trees are stable across chunk reloads.
    let vegetation_climate = Perlin::new(seed.wrapping_add(0x51ED_270B));
    for q in q_start..q_end {
        for r in r_start..r_end {
            let height = *heights.get(&(q, r)).unwrap_or(&0);
            if height < 0 || chunk.y != height.div_euclid(CHUNK_SIZE) {
                continue;
            }
            let hash = vegetation_hash(seed, q, r);
            if !hash.is_multiple_of(29) {
                continue;
            } // Reduced baseline density; biome patches are tuned below.
            let warmth = vegetation_climate.get([q as f64 / 90.0, r as f64 / 90.0]);
            let wetness = vegetation_climate.get([q as f64 / 37.0 + 31.0, r as f64 / 37.0 - 17.0]);
            let conifer = warmth < -0.25 || wetness < -0.32;
            let tropical = warmth > 0.38 && wetness > 0.05;
            let roll = (hash >> 16) % 1000;
            // Adult height ranges are metres, converted to hexel layers below.
            let (species, min_m, max_m, trunk_radius, crown_radius, giant) = if tropical {
                if roll < 22 {
                    (4_u8, 55.0_f32, 95.0, 2, 4, true)
                } else if roll < 105 {
                    (5, 25.0, 45.0, 1, 3, false)
                } else {
                    (3, 16.0, 32.0, 1, 3, false)
                }
            } else if conifer {
                if roll < 8 {
                    (4, 55.0_f32, 95.0, 2, 4, true)
                } else if roll < 570 {
                    (2, 18.0, 38.0, 1, 2, false)
                } else {
                    (1, 10.0, 24.0, 1, 2, false)
                }
            } else if wetness > 0.25 {
                if roll < 9 {
                    (4, 55.0_f32, 95.0, 2, 4, true)
                } else if roll < 590 {
                    (0, 18.0, 32.0, 1, 3, false)
                } else {
                    (1, 10.0, 24.0, 1, 2, false)
                }
            } else {
                (6, 6.0, 16.0, 1, 2, false)
            };
            let variance = ((hash >> 8) % 10_001) as f32 / 10_000.0;
            let mut height_m = min_m + (max_m - min_m) * variance;
            if giant && !(hash >> 28).is_multiple_of(4) {
                height_m = min_m * 0.55;
            }
            let layers = (height_m / scale.hexel_height).round().max(3.0) as i32;
            let trunk = if species == 1 {
                [0.78, 0.75, 0.65]
            } else {
                [0.38, 0.25, 0.14]
            };
            let foliage = match species {
                1 => [0.24, 0.40, 0.19],
                2 => [0.12, 0.30, 0.15],
                4 => [0.07, 0.31, 0.11],
                5 => [0.10, 0.39, 0.13],
                6 => [0.30, 0.39, 0.17],
                _ => [0.18, 0.46, 0.15],
            };
            let (leaf_layer, bark_layer) = match species {
                0 => (7, 22),             // oak
                1 => (3, 17),             // birch
                2 => (8, 23),             // pine
                3 if tropical => (5, 19), // kapok
                3 => (0, 15),             // acacia-like
                4 if tropical => (9, 24), // purpleheart
                4 => (7, 22),             // large temperate tree
                5 => (1, 16),             // bald cypress
                6 => (6, 21),             // maple
                _ => (7, 22),
            };
            append_hexel_tree(
                &mut vertices,
                CellPos::new(q, r, height + 1),
                layers,
                trunk_radius,
                crown_radius,
                (trunk, foliage),
                species,
                hash,
                leaf_layer,
                bark_layer,
                scale,
            );
        }
    }
    vertices
}
fn block_color_at(block: BlockId, cell: CellPos, seed: u32) -> [f32; 3] {
    let hash = vegetation_hash(
        seed ^ (cell.y as u32).wrapping_mul(0x27D4_EB2D),
        cell.q,
        cell.r,
    );
    match block {
        BlockId::Grass => match hash % 9 {
            0 => [0.34, 0.52, 0.24],
            1 => [0.43, 0.57, 0.29],
            _ => [0.25, 0.66, 0.25],
        },
        BlockId::Dirt => match hash % 5 {
            0 => [0.38, 0.25, 0.15],
            1 => [0.55, 0.39, 0.24],
            2 => [0.31, 0.22, 0.15],
            _ => [0.47, 0.29, 0.16],
        },
        BlockId::Stone => match cell.y.rem_euclid(11) {
            0..=2 => [0.49, 0.48, 0.43],
            3..=5 => [0.43, 0.46, 0.49],
            6..=8 => [0.53, 0.50, 0.45],
            _ => [0.38, 0.40, 0.43],
        },
        BlockId::Foundation => [0.22, 0.23, 0.27],
        BlockId::Air => [0.0, 0.0, 0.0],
    }
}

fn vegetation_hash(seed: u32, q: i32, r: i32) -> u32 {
    let mut x = seed ^ (q as u32).wrapping_mul(0x9E37_79B9) ^ (r as u32).wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}

#[allow(clippy::too_many_arguments)]
fn append_hexel_tree(
    vertices: &mut Vec<Vertex>,
    root: CellPos,
    height_layers: i32,
    trunk_radius: i32,
    crown_radius: i32,
    colors: ([f32; 3], [f32; 3]),
    species: u8,
    seed_hash: u32,
    leaf_layer: u32,
    bark_layer: u32,
    scale: WorldScale,
) {
    let (trunk_color, leaf_color) = colors;
    let trunk_top = height_layers * if species == 4 { 3 } else { 2 } / 3;
    for y in 0..trunk_top {
        let taper = y as f32 / trunk_top.max(1) as f32;
        let radius = if trunk_radius > 1 && taper > 0.42 {
            1
        } else {
            trunk_radius
        };
        for dq in -(radius - 1)..=radius - 1 {
            for dr in -(radius - 1)..=radius - 1 {
                if radius == 1 || (dq.abs() + dr.abs() + (dq + dr).abs()) / 2 < radius {
                    append_hexel_prism(
                        vertices,
                        CellPos::new(root.q + dq, root.r + dr, root.y + y),
                        scale,
                        trunk_color,
                        true,
                        bark_layer,
                    );
                }
            }
        }
    }
    let crown_start = (height_layers as f32
        * if species == 2 || species == 6 {
            0.30
        } else {
            0.48
        }) as i32;
    let crown_layers = (height_layers - crown_start).max(2);

    // Structural branches radiate from the trunk before leaf clusters are added.
    // Direction vectors use the six neighbors of the hex grid, keeping branch
    // placement aligned with the engine's hexagonal-prism world.
    let directions = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];
    let branch_levels = if species == 2 {
        5
    } else if species == 4 || species == 5 {
        3
    } else {
        4
    };
    for level in 0..branch_levels {
        let y = crown_start + crown_layers * (level + 1) / (branch_levels + 1);
        let dir_index = (vegetation_hash(seed_hash, root.q + level * 13, root.r - level * 7)
            as usize)
            % directions.len();
        let (dq, dr) = directions[dir_index];
        let length = if species == 2 {
            (crown_radius - level / 2).max(1)
        } else {
            (crown_radius - 1).max(1)
        };
        for step in 1..=length {
            let branch_cell = CellPos::new(
                root.q + dq * step,
                root.r + dr * step,
                root.y + y + step / 3,
            );
            append_hexel_prism(
                vertices,
                branch_cell,
                scale,
                [
                    trunk_color[0] * 0.94,
                    trunk_color[1] * 0.94,
                    trunk_color[2] * 0.94,
                ],
                true,
                bark_layer,
            );
        }
    }

    for cy in 0..crown_layers {
        if cy % 2 != 0 {
            continue;
        }
        let progress = cy as f32 / crown_layers as f32;
        let radius = if species == 2 {
            ((crown_radius as f32) * (1.0 - progress * 0.88)).ceil() as i32
        } else if species == 6 {
            ((crown_radius as f32) * (1.0 - progress * 0.72)).ceil() as i32
        } else if species == 4 || species == 5 {
            ((crown_radius as f32) * (0.55 + (1.0 - progress) * 0.45)).ceil() as i32
        } else {
            ((crown_radius as f32) * (0.72 + (1.0 - progress) * 0.28)).ceil() as i32
        };
        for dq in -radius..=radius {
            for dr in -radius..=radius {
                let distance = (dq.abs() + dr.abs() + (dq + dr).abs()) / 2;
                let irregular =
                    vegetation_hash(seed_hash, root.q + dq + cy * 17, root.r + dr - cy * 11) % 100;
                let edge_noise = if species == 2 { 8 } else { 34 };
                let keep = distance <= radius && (distance < radius || irregular > edge_noise);
                if !keep || (cy < crown_layers / 3 && dq == 0 && dr == 0) {
                    continue;
                }
                // Avoid a solid voxel-like leaf mass: only a deterministic subset of
                // canopy cells receives a textured, crossed leaf cluster. This gives
                // branches and sky gaps room to read through the silhouette.
                if irregular.is_multiple_of(if species == 2 { 4 } else { 3 }) || distance == radius
                {
                    let cluster_cell =
                        CellPos::new(root.q + dq, root.r + dr, root.y + crown_start + cy);
                    append_foliage_cluster(
                        vertices,
                        cluster_cell,
                        scale,
                        leaf_color,
                        seed_hash ^ irregular,
                        leaf_layer,
                    );
                }
            }
        }
    }
}

fn append_foliage_cluster(
    vertices: &mut Vec<Vertex>,
    cell: CellPos,
    scale: WorldScale,
    color: [f32; 3],
    seed: u32,
    leaf_layer: u32,
) {
    let center = scale.prism_center(cell);
    let bottom = cell.y as f32 * scale.hexel_height + scale.hexel_height * 0.12;
    let top = (cell.y + 1) as f32 * scale.hexel_height + scale.hexel_height * 0.82;
    let half_width = scale.hex_radius * (0.95 + (seed % 23) as f32 * 0.012);
    // Three intersecting planes provide volume from multiple viewing angles while
    // each plane uses the same alpha-cutout texture and preserves transparent gaps.
    for angle_deg in [0.0_f32, 60.0, 120.0] {
        let angle = angle_deg.to_radians();
        let axis = Vec3::new(angle.cos(), 0.0, angle.sin());
        let a = Vec3::new(center.x, bottom, center.z) - axis * half_width;
        let b = Vec3::new(center.x, bottom, center.z) + axis * half_width;
        let c = Vec3::new(center.x, top, center.z) + axis * half_width * 0.82;
        let d = Vec3::new(center.x, top, center.z) - axis * half_width * 0.82;
        let tint = [
            color[0] * (0.88 + angle_deg / 600.0),
            color[1],
            color[2] * (1.08 - angle_deg / 900.0),
        ];
        push_tree_textured_triangle(
            vertices,
            [(a, [0.0, 1.0]), (b, [1.0, 1.0]), (c, [1.0, 0.0])],
            tint,
            leaf_layer,
        );
        push_tree_textured_triangle(
            vertices,
            [(a, [0.0, 1.0]), (c, [1.0, 0.0]), (d, [0.0, 0.0])],
            tint,
            leaf_layer,
        );
        // Reverse winding makes the card visible from its back side without
        // relying on a double-sided material setting.
        push_tree_textured_triangle(
            vertices,
            [(b, [1.0, 1.0]), (a, [0.0, 1.0]), (d, [0.0, 0.0])],
            tint,
            leaf_layer,
        );
        push_tree_textured_triangle(
            vertices,
            [(b, [1.0, 1.0]), (d, [0.0, 0.0]), (c, [1.0, 0.0])],
            tint,
            leaf_layer,
        );
    }
}

fn append_hexel_prism(
    vertices: &mut Vec<Vertex>,
    cell: CellPos,
    scale: WorldScale,
    color: [f32; 3],
    wood: bool,
    texture_layer: u32,
) {
    let center = scale.prism_center(cell);
    let bottom = cell.y as f32 * scale.hexel_height;
    let top = (cell.y + 1) as f32 * scale.hexel_height;
    let radius = scale.hex_radius * if wood { 0.42 } else { 0.50 };
    let ring: [Vec3; 6] = std::array::from_fn(|i| {
        let angle = (30.0_f32 + i as f32 * 60.0).to_radians();
        Vec3::new(
            center.x + angle.cos() * radius,
            center.y,
            center.z + angle.sin() * radius,
        )
    });
    let top_color = [
        (color[0] * 1.10).min(1.0),
        (color[1] * 1.10).min(1.0),
        (color[2] * 1.10).min(1.0),
    ];
    for i in 0..6 {
        let j = (i + 1) % 6;
        if wood {
            push_triangle(
                vertices,
                Vec3::new(center.x, top, center.z),
                Vec3::new(ring[i].x, top, ring[i].z),
                Vec3::new(ring[j].x, top, ring[j].z),
                top_color,
            );
        } else {
            push_textured_triangle(
                vertices,
                [
                    (Vec3::new(center.x, top, center.z), [0.5, 0.5]),
                    (Vec3::new(ring[i].x, top, ring[i].z), [0.0, 0.0]),
                    (Vec3::new(ring[j].x, top, ring[j].z), [1.0, 0.0]),
                ],
                top_color,
            );
        }
        let shade = 0.55 + i as f32 * 0.045;
        let side = [color[0] * shade, color[1] * shade, color[2] * shade];
        if wood {
            let a_bottom = Vec3::new(ring[i].x, bottom, ring[i].z);
            let a_top = Vec3::new(ring[i].x, top, ring[i].z);
            let b_bottom = Vec3::new(ring[j].x, bottom, ring[j].z);
            let b_top = Vec3::new(ring[j].x, top, ring[j].z);
            push_tree_textured_triangle(
                vertices,
                [
                    (a_bottom, [0.0, cell.y as f32 * 0.35 + 1.0]),
                    (a_top, [0.0, cell.y as f32 * 0.35]),
                    (b_top, [1.0, cell.y as f32 * 0.35]),
                ],
                side,
                texture_layer,
            );
            push_tree_textured_triangle(
                vertices,
                [
                    (a_bottom, [0.0, cell.y as f32 * 0.35 + 1.0]),
                    (b_top, [1.0, cell.y as f32 * 0.35]),
                    (b_bottom, [1.0, cell.y as f32 * 0.35 + 1.0]),
                ],
                side,
                texture_layer,
            );
        } else {
            let a_bottom = Vec3::new(ring[i].x, bottom, ring[i].z);
            let a_top = Vec3::new(ring[i].x, top, ring[i].z);
            let b_bottom = Vec3::new(ring[j].x, bottom, ring[j].z);
            let b_top = Vec3::new(ring[j].x, top, ring[j].z);
            push_textured_triangle(
                vertices,
                [
                    (a_bottom, [0.0, 1.0]),
                    (a_top, [0.0, 0.0]),
                    (b_top, [1.0, 0.0]),
                ],
                side,
            );
            push_textured_triangle(
                vertices,
                [
                    (a_bottom, [0.0, 1.0]),
                    (b_top, [1.0, 0.0]),
                    (b_bottom, [1.0, 1.0]),
                ],
                side,
            );
        }
    }
}
fn push_triangle(vertices: &mut Vec<Vertex>, a: Vec3, b: Vec3, c: Vec3, color: [f32; 3]) {
    for point in [a, b, c] {
        vertices.push(Vertex {
            position: point.to_array(),
            color,
            uv: [0.0, 0.0],
            textured: 0.0,
        });
    }
}

fn push_textured_triangle(
    vertices: &mut Vec<Vertex>,
    points: [(Vec3, [f32; 2]); 3],
    color: [f32; 3],
) {
    for (point, uv) in points {
        vertices.push(Vertex {
            position: point.to_array(),
            color,
            uv,
            textured: 1.0,
        });
    }
}

fn push_tree_textured_triangle(
    vertices: &mut Vec<Vertex>,
    points: [(Vec3, [f32; 2]); 3],
    color: [f32; 3],
    layer: u32,
) {
    for (point, uv) in points {
        vertices.push(Vertex {
            position: point.to_array(),
            color,
            uv,
            textured: (layer + 2) as f32,
        });
    }
}

fn create_tree_texture_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::Sampler) {
    const SIZE: u32 = 64;
    const PATHS: [&str; 30] = [
        "assets/textures/leaves/small/acacia1.png",
        "assets/textures/leaves/small/baldcypress1.png",
        "assets/textures/leaves/small/bamboo.png",
        "assets/textures/leaves/small/birch.png",
        "assets/textures/leaves/small/ebony1.png",
        "assets/textures/leaves/small/kapok1.png",
        "assets/textures/leaves/small/maple.png",
        "assets/textures/leaves/small/oak.png",
        "assets/textures/leaves/small/pine1.png",
        "assets/textures/leaves/small/purpleheartwood1.png",
        "assets/textures/leaves/small/walnut1.png",
        "assets/textures/leaves/small/acacia2.png",
        "assets/textures/leaves/small/kapok2.png",
        "assets/textures/leaves/small/pine2.png",
        "assets/textures/leaves/small/fallen.png",
        "assets/textures/trunks/acacia.png",
        "assets/textures/trunks/baldcypress.png",
        "assets/textures/trunks/birch.png",
        "assets/textures/trunks/ebony.png",
        "assets/textures/trunks/kapok.png",
        "assets/textures/trunks/larch.png",
        "assets/textures/trunks/maple.png",
        "assets/textures/trunks/oak.png",
        "assets/textures/trunks/pine.png",
        "assets/textures/trunks/purpleheart.png",
        "assets/textures/trunks/redwood.png",
        "assets/textures/trunks/walnut.png",
        "assets/textures/trunks/aged.png",
        "assets/textures/trunks/veryaged.png",
        "assets/textures/trunks/veryagedrotten.png",
    ];
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Imported species leaf and bark texture array"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: PATHS.len() as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, relative_path) in PATHS.iter().enumerate() {
        let full_path = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), relative_path);
        let pixels = match std::fs::read(&full_path)
            .ok()
            .and_then(|bytes| image::load_from_memory(&bytes).ok())
        {
            Some(image) => image
                .resize_exact(SIZE, SIZE, image::imageops::FilterType::Nearest)
                .to_rgba8()
                .into_raw(),
            None => {
                eprintln!(
                    "Hexel Engine: fallback texture used for missing/unreadable asset {}",
                    relative_path
                );
                fallback_tree_texture(layer, SIZE)
            }
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer as u32,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 4),
                rows_per_image: Some(SIZE),
            },
            wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
        );
    }
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Nearest-filtered tree texture sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    (texture, sampler)
}

fn fallback_tree_texture(layer: usize, size: u32) -> Vec<u8> {
    let mut pixels = vec![0_u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            if layer < 15 {
                let dx = x as f32 / size as f32 - 0.5;
                let dy = y as f32 / size as f32 - 0.5;
                let leaf = dx * dx * 2.0 + dy * dy * 1.4 < 0.20
                    && !(x + y * 3 + layer as u32).is_multiple_of(5);
                pixels[i..i + 4].copy_from_slice(if leaf {
                    &[43, 115, 39, 255]
                } else {
                    &[0, 0, 0, 0]
                });
            } else {
                let stripe = ((x + layer as u32 * 3) % 13) as u8;
                let grain = ((x * 7 + y * 3 + layer as u32) % 17) as u8;
                pixels[i] = 76_u8.saturating_add(stripe.saturating_mul(3));
                pixels[i + 1] = 45_u8.saturating_add(grain);
                pixels[i + 2] = 25_u8.saturating_add(grain / 2);
                pixels[i + 3] = 255;
            }
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_invalidates_diagonal_axial_neighbor_chunk() {
        // The (q + 1, r - 1) neighbor crosses both chunk axes here.
        let chunks = affected_chunks_for_cell(CellPos::new(15, 16, 4));
        assert!(chunks.contains(&ChunkPos { q: 1, r: 0, y: 0 }));
    }

    #[test]
    fn edit_invalidates_vertical_chunks_at_layer_boundaries() {
        let below = affected_chunks_for_cell(CellPos::new(4, 4, 16));
        let above = affected_chunks_for_cell(CellPos::new(4, 4, 15));
        assert!(below.contains(&ChunkPos { q: 0, r: 0, y: 0 }));
        assert!(above.contains(&ChunkPos { q: 0, r: 0, y: 1 }));
    }
}
