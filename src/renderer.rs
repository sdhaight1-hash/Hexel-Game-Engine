use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};

use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Mat4, Vec3};
use hexel_engine::{
    planetary_chunks::{chunk_keys_near_direction, CubeFace, PlanetChunkKey, PlanetChunkMesh},
    planetary_frame::PlanetaryFrame,
    planetary_world::PlanetaryWorld,
    planetary_chunk_world::PlanetaryChunkWorld,
    world_profile::PlanetaryWorldProfile,
    planetary_voxels::{PlanetVoxelPos, PlanetVoxelVolume},
    BlockId, CellPos, WorldScale,
    planet::{Biome, PlanetConfig, PlanetGenerator, PlanetSample},
    surface_mesh::SurfaceMesh,
    topology::SurfaceTopology,
};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};


const SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32>, world_offset: vec4<f32>, };
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
    output.clip_position = camera.view_proj * vec4<f32>(input.position + camera.world_offset.xyz, 1.0);
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
    world_offset: [f32; 4],
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
        // Match PlanetaryFrame's east = north × up convention. Keeping one
        // handedness is essential: otherwise camera yaw and local movement reverse
        // east/west relative to terrain rebasing and planetary chunk coordinates.
        let east = north.cross(up).normalize_or_zero();
        (east, north, up)
    }
    pub fn forward(&self) -> Vec3 {
        let (east, north, up) = self.tangent_frame();
        (east * self.yaw.sin() * self.pitch.cos() + north * self.yaw.cos() * self.pitch.cos() + up * self.pitch.sin()).normalize_or_zero()
    }
    fn projection(&self, size: PhysicalSize<u32>) -> Mat4 {
        let aspect = size.width.max(1) as f32 / size.height.max(1) as f32;
        Mat4::perspective_rh(70.0_f32.to_radians(), aspect, 0.05, 900.0)
    }
    fn view_projection(&self, size: PhysicalSize<u32>) -> Mat4 {
        self.projection(size)
            * Mat4::look_to_rh(self.position, self.forward(), self.up.normalize_or_zero())
    }
    /// View matrix for patch-local vertices. Their planetary origin is supplied
    /// separately in double precision, avoiding Earth-radius f32 quantization.
    fn relative_view_projection(&self, size: PhysicalSize<u32>) -> Mat4 {
        self.projection(size)
            * Mat4::look_to_rh(Vec3::ZERO, self.forward(), self.up.normalize_or_zero())
    }
}

struct PlanetGpuChunk {
    buffer: wgpu::Buffer,
    vertex_count: u32,
    origin: DVec3,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
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
    planet_cell_wall_ranges: HashMap<u32, (usize, usize)>,
    planet_world: PlanetaryWorld,
    planet_hexel_chunks: PlanetaryChunkWorld,
    planet_voxels: PlanetVoxelVolume,
    removed_planet_trees: HashSet<u32>,
    planet_stream_chunks: HashMap<PlanetChunkKey, PlanetGpuChunk>,
    planet_stream_pending: VecDeque<PlanetChunkKey>,
    planet_hexel_surface: Option<PlanetGpuChunk>,
    planet_hexel_anchor: Option<DVec3>,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_layout: wgpu::BindGroupLayout,
    foliage_bind_group: wgpu::BindGroup,
    size: PhysicalSize<u32>,
    _depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    seed: u32,
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
                world_offset: [0.0; 4],
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
            radius: 6_371_000.0,
            elevation_scale: 4_500.0,
            axial_tilt_degrees: 23.5,
        };
        let planet_world = PlanetaryWorld::new(seed, 16, planet_config)
            .map_err(|error| format!("Could not generate planetary world: {error:?}"))?;
        let planet_hexel_chunks = PlanetaryChunkWorld::new(
            PlanetGenerator::with_config(seed, planet_config),
            PlanetaryWorldProfile::default(),
        )
        .map_err(|error| format!("Could not initialize planetary Hexel chunks: {error:?}"))?;
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
        const CELL_WALL_DEPTH: f64 = 1.0;
        let mut planet_cell_wall_ranges = HashMap::new();
        for cell in topology.cells() {
            let wall_start = planet_vertices.len();
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
            planet_cell_wall_ranges.insert(cell.id.0, (wall_start, planet_vertices.len()));
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
            planet_cell_wall_ranges,
            planet_voxels: PlanetVoxelVolume::new(),
            removed_planet_trees: HashSet::new(),
            planet_world,
            planet_hexel_chunks,
            planet_stream_chunks: HashMap::new(),
            planet_stream_pending: VecDeque::new(),
            planet_hexel_surface: None,
            planet_hexel_anchor: None,
            camera_buffer,
            camera_bind_group,
            camera_layout,
            foliage_bind_group,
            size,
            _depth_texture: depth_texture,
            depth_view,
            seed,
        };
        Ok(renderer)
    }

    pub fn world_seed(&self) -> u32 {
        self.seed
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

    pub fn planet_voxel_edits_snapshot(&self) -> Vec<(u32, i32, BlockId)> {
        self.planet_voxels.edited_voxels().iter()
            .map(|(pos, block)| (pos.cell.0, pos.layer, *block))
            .collect()
    }

    pub fn restore_planet_voxel_edits(&mut self, edits: &[(u32, i32, BlockId)]) {
        for (id, layer, block) in edits {
            self.edit_planet_voxel(*id, *layer, *block);
        }
    }

    pub fn planet_removed_trees_snapshot(&self) -> Vec<u32> {
        let mut ids = self.removed_planet_trees.iter().copied().collect::<Vec<_>>();
        ids.sort_unstable();
        ids
    }

    pub fn restore_planet_removed_trees(&mut self, ids: &[u32]) {
        self.removed_planet_trees.extend(ids.iter().copied());
        self.planet_stream_chunks.clear();
        self.planet_stream_pending.clear();
    }

    pub fn remove_planet_tree(&mut self, id: u32) -> bool {
        let Some(cell) = self.planet_world.topology().cell(hexel_engine::topology::SurfaceCellId(id)) else { return false; };
        let sample = self.planet_world.terrain().sample_direction(cell.center, 0.25);
        let hash = vegetation_hash(self.seed ^ id.wrapping_mul(0x27D4_EB2D), id as i32, id.rotate_left(13) as i32);
        if planet_tree_spec(sample.biome, hash).is_none() || self.removed_planet_trees.contains(&id) {
            return false;
        }
        self.removed_planet_trees.insert(id);
        self.planet_stream_chunks.clear();
        self.planet_stream_pending.clear();
        true
    }

    pub fn planet_voxel_block_at(&self, id: u32, layer: i32) -> BlockId {
        self.planet_voxels.block_at(
            &self.planet_world,
            PlanetVoxelPos::new(hexel_engine::topology::SurfaceCellId(id), layer),
        ).unwrap_or(BlockId::Air)
    }

    /// The first solid voxel in the radial column, starting at the surface.
    pub fn planet_top_solid_voxel(&self, id: u32) -> (i32, BlockId) {
        let layer = self.planet_top_solid_layer(id);
        (layer, self.planet_voxel_block_at(id, layer))
    }

    fn planet_top_solid_layer(&self, id: u32) -> i32 {
        let cell = hexel_engine::topology::SurfaceCellId(id);
        let positive_start = PlanetVoxelPos::new(cell, 1);
        let positive_end = PlanetVoxelPos::new(cell, PlanetVoxelVolume::MAX_BUILD_HEIGHT);
        if let Some((pos, _)) = self.planet_voxels.edited_voxels()
            .range(positive_start..=positive_end).rev().find(|(_, block)| block.is_solid()) {
            return pos.layer;
        }
        if self.planet_world.edited_cells().get(&cell) != Some(&BlockId::Air) {
            return 0;
        }
        for layer in (PlanetVoxelVolume::BEDROCK_LAYER + 1..0).rev() {
            let pos = PlanetVoxelPos::new(cell, layer);
            match self.planet_voxels.edited_voxels().get(&pos) {
                Some(block) if !block.is_solid() => continue,
                Some(_) => return layer,
                None => return layer,
            }
        }
        PlanetVoxelVolume::BEDROCK_LAYER
    }

    pub fn planet_block_at(&self, id: u32) -> BlockId {
        self.planet_top_solid_voxel(id).1
    }

    pub fn edit_planet_voxel(&mut self, id: u32, layer: i32, block: BlockId) -> bool {
        let pos = PlanetVoxelPos::new(hexel_engine::topology::SurfaceCellId(id), layer);
        if self.planet_voxels.block_at(&self.planet_world, pos).is_none()
            || !self.planet_voxels.set_block(&self.planet_world, pos, block) {
            return false;
        }
        if layer == 0 && self.planet_world.set_block(pos.cell, block).is_err() {
            return false;
        }
        self.refresh_planet_cell_and_neighbors(id);
        true
    }

    pub fn edit_planet_cell(&mut self, id: u32, block: BlockId) -> bool {
        if block == BlockId::Foundation || !self.planet_cell_ranges.contains_key(&id) {
            return false;
        }
        let cell = hexel_engine::topology::SurfaceCellId(id);
        if self.planet_world.set_block(cell, block).is_err()
            || !self.planet_voxels.set_block(&self.planet_world, PlanetVoxelPos::new(cell, 0), block) {
            return false;
        }
        self.refresh_planet_cell_and_neighbors(id);
        true
    }

    fn refresh_planet_cell_and_neighbors(&mut self, id: u32) {
        self.refresh_planet_cell(id);
        let neighbors = self.planet_world.topology()
            .cell(hexel_engine::topology::SurfaceCellId(id))
            .map(|cell| cell.neighbors.iter().map(|neighbor| neighbor.0).collect::<Vec<_>>())
            .unwrap_or_default();
        for neighbor in neighbors {
            self.refresh_planet_cell(neighbor);
        }
        self.planet_stream_chunks.clear();
        self.planet_stream_pending.clear();
    }

    fn refresh_planet_cell(&mut self, id: u32) {
        let Some((start, end)) = self.planet_cell_ranges.get(&id).copied() else { return; };
        let (top_layer, block) = self.planet_top_solid_voxel(id);
        let vertex_size = std::mem::size_of::<Vertex>() as u64;
        let first = (start / vertex_size) as usize;
        let end_index = (end / vertex_size) as usize;
        let offset = -(top_layer as f64) * hexel_engine::world_profile::HEXEL_HEIGHT_METRES as f64;
        let color = planet_block_color(block);
        for vertex in &mut self.planet_vertices[first..end_index] {
            let direction = Vec3::from_array(vertex.position).normalize_or_zero();
            let radial = DVec3::new(direction.x as f64, direction.y as f64, direction.z as f64);
            let sample = self.planet_world.terrain().sample_direction(radial, 0.25);
            vertex.position = (sample.surface_position - sample.direction * offset).as_vec3().to_array();
            vertex.color = color;
        }
        self.queue.write_buffer(
            &self.planet_vertex_buffer,
            start,
            bytemuck::cast_slice(&self.planet_vertices[first..end_index]),
        );

        if let (Some(cell), Some((wall_start, wall_end))) = (
            self.planet_world.topology().cell(hexel_engine::topology::SurfaceCellId(id)),
            self.planet_cell_wall_ranges.get(&id).copied(),
        ) {
            const BLOCK_DEPTH: f64 = 1.0;
            let mut deepest_neighbor = (-top_layer).max(0) as f64 * BLOCK_DEPTH;
            for neighbor in &cell.neighbors {
                let neighbor_layer = self.planet_top_solid_layer(neighbor.0);
                deepest_neighbor = deepest_neighbor.max((-neighbor_layer).max(0) as f64 * BLOCK_DEPTH);
            }
            let top_offset = top_layer as f64 * BLOCK_DEPTH;
            let wall_depth = (top_offset + deepest_neighbor + BLOCK_DEPTH).max(BLOCK_DEPTH);
            let mut wall_positions = Vec::with_capacity(wall_end - wall_start);
            for index in 0..cell.boundary.len() {
                let next = (index + 1) % cell.boundary.len();
                let raw_a = self.planet_world.terrain().sample_direction(cell.boundary[index], 0.25).surface_position;
                let raw_b = self.planet_world.terrain().sample_direction(cell.boundary[next], 0.25).surface_position;
                let top_a = raw_a - raw_a.normalize() * offset;
                let top_b = raw_b - raw_b.normalize() * offset;
                let bottom_a = top_a - top_a.normalize() * wall_depth;
                let bottom_b = top_b - top_b.normalize() * wall_depth;
                let mut quad = [
                    top_a.as_vec3(), bottom_a.as_vec3(), top_b.as_vec3(),
                    top_b.as_vec3(), bottom_a.as_vec3(), bottom_b.as_vec3(),
                ];
                for tri in quad.as_chunks_mut::<3>().0 {
                    let a = tri[0];
                    let b = tri[1];
                    let c = tri[2];
                    if (b - a).cross(c - a).dot((a + b + c) / 3.0) < 0.0 {
                        tri.swap(1, 2);
                    }
                }
                wall_positions.extend(quad);
            }
            for (vertex, position) in self.planet_vertices[wall_start..wall_end].iter_mut().zip(wall_positions) {
                vertex.position = position.to_array();
            }
            let byte_offset = wall_start as u64 * std::mem::size_of::<Vertex>() as u64;
            self.queue.write_buffer(
                &self.planet_vertex_buffer,
                byte_offset,
                bytemuck::cast_slice(&self.planet_vertices[wall_start..wall_end]),
            );
        }
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
            if self.removed_planet_trees.contains(&id)
                || self.planet_world.edited_cells().get(&hexel_engine::topology::SurfaceCellId(id)) == Some(&BlockId::Air) {
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
            let trunk_fraction = if spec.species == 4 { 1.0 } else { 2.0 / 3.0 };
            let trunk_top = spec.height_layers as f64
                * WorldScale::default().hexel_height as f64
                * trunk_fraction;
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
        ).normalize_or_zero();
        let sample = self.planet_world.terrain().sample_direction(radial, 0.25);
        let surface = self.planet_world.topology().cell_id_for_direction(radial)
            .map(|id| {
                let (layer, _) = self.planet_top_solid_voxel(id.0);
                sample.surface_position + sample.direction * (layer as f64 * hexel_engine::world_profile::HEXEL_HEIGHT_METRES as f64)
            })
            .unwrap_or(sample.surface_position);
        surface.as_vec3()
    }

    pub fn planet_cell_at_ray(&self, origin: Vec3, direction: Vec3) -> Option<u32> {
        self.planet_cell_hit_at_ray(origin, direction).map(|(id, _)| id)
    }

    /// Intersect the live edited surface triangles and inward cell walls. The
    /// returned distance is the same geometry used for rendering, not an ideal sphere.
    pub fn planet_cell_hit_at_ray(&self, origin: Vec3, direction: Vec3) -> Option<(u32, f32)> {
        let dir = direction.normalize_or_zero();
        if dir == Vec3::ZERO { return None; }
        let mut best: Option<(u32, f32)> = None;
        let vertex_size = std::mem::size_of::<Vertex>() as u64;
        let radius = self.planet_world.descriptor().radius;
        let elevation = self.planet_world.descriptor().elevation_scale;
        for (id, (start, end)) in &self.planet_cell_ranges {
            let cell_id = hexel_engine::topology::SurfaceCellId(*id);
            let Some(cell) = self.planet_world.topology().cell(cell_id) else { continue; };
            let layer = self.planet_top_solid_layer(*id);
            let offset = layer as f64 * hexel_engine::world_profile::HEXEL_HEIGHT_METRES as f64;
            let mut max_extent = offset.abs();
            for neighbor in &cell.neighbors {
                let neighbor_extent = self.planet_top_solid_layer(neighbor.0).abs() as f64 * 1.0;
                max_extent = max_extent.max(neighbor_extent);
            }
            let center = (cell.center * radius + cell.center * (offset * 0.5)).as_vec3();
            let bound_radius = radius * 0.18 + elevation + max_extent + offset.abs() * 0.5 + 1.0;
            let Some(bound_hit) = ray_sphere_distance(origin, dir, center, bound_radius as f32) else {
                continue;
            };
            if best.is_some_and(|(_, distance)| bound_hit > distance) {
                continue;
            }
            let first = (*start / vertex_size) as usize;
            let last = (*end / vertex_size) as usize;
            if let Some(distance) = ray_triangle_range(&self.planet_vertices, origin, dir, first, last)
                && best.is_none_or(|(_, current)| distance < current) {
                best = Some((*id, distance));
            }
            if let Some((wall_start, wall_end)) = self.planet_cell_wall_ranges.get(id)
                && let Some(distance) = ray_triangle_range(&self.planet_vertices, origin, dir, *wall_start, *wall_end)
                && best.is_none_or(|(_, current)| distance < current) {
                best = Some((*id, distance));
            }
        }
        best.filter(|(_, distance)| *distance <= 8.0)
    }

    /// Ray-test deterministic trunk cylinders and canopy spheres before the terrain hit.
    /// Returned IDs are stable across streamed chunk rebuilds and face seams.
    pub fn planet_tree_at_ray(&self, origin: Vec3, direction: Vec3) -> Option<u32> {
        let dir = direction.normalize_or_zero();
        if dir == Vec3::ZERO { return None; }
        let (surface_id, surface_distance) = self.planet_cell_hit_at_ray(origin, dir)?;
        let surface_distance = surface_distance as f64;
        let origin_d = DVec3::new(origin.x as f64, origin.y as f64, origin.z as f64);
        let dir_d = DVec3::new(dir.x as f64, dir.y as f64, dir.z as f64);

        let surface_cell = self.planet_world.topology()
            .cell(hexel_engine::topology::SurfaceCellId(surface_id))?;
        let mut candidates = HashSet::new();
        candidates.insert(surface_id);
        candidates.extend(surface_cell.neighbors.iter().map(|id| id.0));
        let scale = WorldScale::default();
        let mut best: Option<(f64, u32)> = None;
        for id in candidates {
            if self.removed_planet_trees.contains(&id)
                || self.planet_world.edited_cells()
                    .get(&hexel_engine::topology::SurfaceCellId(id)) == Some(&BlockId::Air) {
                continue;
            }
            let Some(cell) = self.planet_world.topology()
                .cell(hexel_engine::topology::SurfaceCellId(id)) else { continue; };
            let sample = self.planet_world.terrain().sample_direction(cell.center, 0.25);
            let hash = vegetation_hash(
                self.seed ^ id.wrapping_mul(0x27D4_EB2D),
                id as i32,
                id.rotate_left(13) as i32,
            );
            let Some(spec) = planet_tree_spec(sample.biome, hash) else { continue; };
            let up = sample.direction;
            let rel = origin_d - sample.surface_position;
            let axial_origin = rel.dot(up);
            let axial_dir = dir_d.dot(up);
            let perpendicular_origin = rel - up * axial_origin;
            let perpendicular_dir = dir_d - up * axial_dir;
            let trunk_radius = (spec.trunk_radius as f64 * scale.hex_radius as f64 * 0.72).max(0.28);
            let trunk_fraction = if spec.species == 4 { 1.0 } else { 2.0 / 3.0 };
            let trunk_top = spec.height_layers as f64 * scale.hexel_height as f64 * trunk_fraction;
            let a = perpendicular_dir.length_squared();
            if a > 1.0e-10 {
                let bb = 2.0 * perpendicular_origin.dot(perpendicular_dir);
                let cc = perpendicular_origin.length_squared() - trunk_radius * trunk_radius;
                let discriminant = bb * bb - 4.0 * a * cc;
                if discriminant >= 0.0 {
                    for t in [(-bb - discriminant.sqrt()) / (2.0 * a), (-bb + discriminant.sqrt()) / (2.0 * a)] {
                        let height = axial_origin + t * axial_dir;
                        if t >= 0.0 && t < surface_distance && (0.0..=trunk_top).contains(&height)
                            && best.is_none_or(|(distance, _)| t < distance) {
                            best = Some((t, id));
                        }
                    }
                }
            }
            let crown_center = sample.surface_position + up * (trunk_top * 0.78);
            let crown_radius = (spec.crown_radius as f64 * scale.hex_radius as f64 * 1.35).max(0.8);
            let sphere_offset = origin_d - crown_center;
            let sphere_b = sphere_offset.dot(dir_d);
            let sphere_c = sphere_offset.length_squared() - crown_radius * crown_radius;
            let sphere_disc = sphere_b * sphere_b - sphere_c;
            if sphere_disc >= 0.0 {
                let t = -sphere_b - sphere_disc.sqrt();
                if t >= 0.0 && t < surface_distance
                    && best.is_none_or(|(distance, _)| t < distance) {
                    best = Some((t, id));
                }
            }
        }
        best.filter(|(distance, _)| *distance <= 8.0).map(|(_, id)| id)
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
        // At Earth's radius, 400,000 cube-face patches produce roughly
        // 30-metre patch spacing near face centers. Plan a generous bounded
        // neighborhood around the player; never enumerate the whole planet grid.
        // Stream a coarser outer ring quickly enough to close the visible gaps
        // in the distant surface, while retaining enough triangles for terrain
        // relief. At Earth's radius this grid is about 120 m per patch near a
        // cube-face centre; camera-relative vertex storage is the next precision
        // step before pushing the grid back toward metre-scale patches.
        // The previous 400k grid created tiny patches, but also magnified cracks
        // from planet-centred f32 vertices. Use a 200k grid for broader overlap,
        // a denser mesh per patch, and a bounded queue so the close surface fills
        // quickly without trying to build the whole neighbourhood in one frame.
        const CHUNKS_PER_FACE: u32 = 200_000;
        const MESH_RESOLUTION: u32 = 64;
        const LOCAL_PATCH_RADIUS: u32 = 5;
        const MAX_NEW_CHUNKS_PER_FRAME: usize = 32;

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
        for key in chunk_keys_near_direction(radial, CHUNKS_PER_FACE, LOCAL_PATCH_RADIUS) {
            let (normal, axis_u, axis_v) = key.face.basis();
            let u = -1.0 + 2.0 * (key.x as f64 + 0.5) / CHUNKS_PER_FACE as f64;
            let v = -1.0 + 2.0 * (key.y as f64 + 0.5) / CHUNKS_PER_FACE as f64;
            let direction = (normal + axis_u * u + axis_v * v).normalize();
            let dot = direction.dot(radial);
            wanted.insert(key);
            prioritized.push((dot, key));
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
            let patch_origin = mesh.vertices[0].position;
            let mut grid_vertices = Vec::with_capacity(mesh.vertices.len());
            for source in &mesh.vertices {
                let sample = self.planet_world.terrain().sample_direction(source.normal, 0.25);
                let cell_block = self
                    .planet_world.topology()
                    .cell_id_for_direction(source.normal)
                    .map(|id| self.planet_block_at(id.0));
                let surface_position = if cell_block == Some(BlockId::Air) {
                    source.position - source.normal * 1.0
                } else {
                    source.position
                };
                // Keep the streamed surface offset small; camera-relative rebasing is handled separately.
                let position = surface_position + source.normal * 0.035;
                let color = cell_block
                    .map(planet_block_color)
                    .unwrap_or_else(|| planet_vertex_color(sample));
                grid_vertices.push(Vertex {
                    position: (position - patch_origin).as_vec3().to_array(),
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

            // Add a radial skirt around each streamed patch. Adjacent patches
            // sample the same terrain in f64, but casting Earth-centred positions
            // to f32 can separate their borders by fractions of a metre. The
            // skirt extends below the terrain edge so those precision cracks
            // cannot expose the empty background at ground level.
            let side = mesh.resolution + 1;
            let mut perimeter = Vec::with_capacity((mesh.resolution * 4) as usize);
            for column in 0..mesh.resolution {
                perimeter.push(column as usize);
            }
            for row in 0..mesh.resolution {
                perimeter.push((row * side + mesh.resolution) as usize);
            }
            for column in (1..=mesh.resolution).rev() {
                perimeter.push((mesh.resolution * side + column) as usize);
            }
            for row in (1..=mesh.resolution).rev() {
                perimeter.push((row * side) as usize);
            }
            const PATCH_SKIRT_DEPTH: f32 = 12.0;
            for edge in 0..perimeter.len() {
                let a = grid_vertices[perimeter[edge]];
                let b = grid_vertices[perimeter[(edge + 1) % perimeter.len()]];
                let lower = |vertex: Vertex| {
                    let position = Vec3::from_array(vertex.position);
                    let radial = (patch_origin.as_vec3() + position).normalize_or_zero();
                    let mut skirt = vertex;
                    skirt.position = (position - radial * PATCH_SKIRT_DEPTH).to_array();
                    skirt.color = [vertex.color[0] * 0.72, vertex.color[1] * 0.72, vertex.color[2] * 0.72];
                    skirt
                };
                let lower_a = lower(a);
                let lower_b = lower(b);
                vertices.extend_from_slice(&[a, lower_a, b, b, lower_a, lower_b]);
            }

            let terrain_vertex_count = vertices.len();
            append_planetary_trees(
                &mut vertices,
                key,
                CHUNKS_PER_FACE,
                self.planet_world.topology(),
                self.planet_world.terrain(),
                self.seed,
                self.planet_world.edited_cells(),
                &self.removed_planet_trees,
            );
            for vertex in &mut vertices[terrain_vertex_count..] {
                let p = DVec3::new(vertex.position[0] as f64, vertex.position[1] as f64, vertex.position[2] as f64);
                vertex.position = (p - patch_origin).as_vec3().to_array();
            }
            if vertices.is_empty() {
                continue;
            }
            let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Streamed planetary terrain chunk"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let chunk_camera_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Camera-relative terrain patch uniform"),
                contents: bytemuck::bytes_of(&CameraUniform {
                    view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                    world_offset: [0.0; 4],
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let chunk_camera_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Camera-relative terrain patch bind group"),
                layout: &self.camera_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: chunk_camera_buffer.as_entire_binding(),
                }],
            });
            self.planet_stream_chunks.insert(
                key,
                PlanetGpuChunk {
                    buffer,
                    vertex_count: vertices.len() as u32,
                    origin: patch_origin,
                    camera_buffer: chunk_camera_buffer,
                    camera_bind_group: chunk_camera_bind_group,
                },
            );
        }
    }


    /// Build the near-field gameplay ground from real six-sided Hexel prisms.
    /// The smooth cube-sphere mesh remains a distant backdrop; this local mesh is
    /// camera-relative and regenerated only after the player has crossed its radius.
    fn update_planet_hexel_surface(&mut self, player_position: Vec3) {
        const GRID_RADIUS: i32 = 24;
        const REBUILD_DISTANCE: f64 = 7.0;
        let player_d = DVec3::new(
            player_position.x as f64,
            player_position.y as f64,
            player_position.z as f64,
        );
        let radial = player_d.normalize_or_zero();
        if radial == DVec3::ZERO { return; }

        let base_sample = self.planet_world.terrain().sample_direction(radial, 0.25);
        let origin = base_sample.surface_position;
        if self.planet_hexel_anchor.is_some_and(|old| (origin - old).length() < REBUILD_DISTANCE) {
            return;
        }
        let frame = PlanetaryFrame::at(origin);
        let scale = WorldScale::default();
        let radius = scale.hex_radius as f64;
        let step_height = scale.hexel_height as f64;
        let mut vertices: Vec<Vertex> = Vec::with_capacity(90_000);

        for r in -GRID_RADIUS..=GRID_RADIUS {
            for q in -GRID_RADIUS..=GRID_RADIUS {
                // Axial hex spacing: one-metre-wide cells, no square-grid approximation.
                let x = radius * 3.0_f64.sqrt() * (q as f64 + r as f64 * 0.5);
                let z = radius * 1.5 * r as f64;
                let approximate = origin + frame.east * x + frame.north * z;
                let direction = approximate.normalize_or_zero();
                let sample = self.planet_world.terrain().sample_direction(direction, 0.25);
                let layer = ((sample.elevation - base_sample.elevation) / step_height).round();
                // Lift slightly above the smooth backdrop to avoid z-fighting.
                let top_y = layer * step_height + 0.075;
                let bottom_y = top_y - step_height;
                // Keep neighbouring Hexels individually legible: a narrow dark
                // perimeter separates their top faces instead of blending into a blob.
                let variation = if (q - r).rem_euclid(2) == 0 { 1.035 } else { 0.965 };
                let color = planet_vertex_color(sample).map(|c| (c * variation).min(1.0));
                let edge_color = [color[0] * 0.34, color[1] * 0.34, color[2] * 0.34];
                let center = Vec3::new(x as f32, top_y as f32, z as f32);
                let mut corners = [Vec3::ZERO; 6];
                let mut inner_corners = [Vec3::ZERO; 6];
                // A 30-degree offset matches the pointy-top axial grid used by WorldScale.
                for i in 0..6 {
                    let angle = (i as f64 * 60.0 + 30.0).to_radians();
                    let dx = angle.cos();
                    let dz = angle.sin();
                    corners[i] = Vec3::new(
                        (x + radius * dx) as f32,
                        top_y as f32,
                        (z + radius * dz) as f32,
                    );
                    let inset = radius * 0.90;
                    inner_corners[i] = Vec3::new(
                        (x + inset * dx) as f32,
                        (top_y + 0.004) as f32,
                        (z + inset * dz) as f32,
                    );
                }
                for i in 0..6 {
                    let next = (i + 1) % 6;
                    // Dark bevel/rim on top makes each one-metre cell visible at a glance.
                    push_triangle(&mut vertices, corners[i], inner_corners[i], inner_corners[next], edge_color);
                    push_triangle(&mut vertices, corners[i], inner_corners[next], corners[next], edge_color);
                    push_triangle(&mut vertices, center, inner_corners[i], inner_corners[next], color);
                    let lower_a = Vec3::new(corners[i].x, bottom_y as f32, corners[i].z);
                    let lower_b = Vec3::new(corners[next].x, bottom_y as f32, corners[next].z);
                    let side_color = [color[0] * 0.58, color[1] * 0.58, color[2] * 0.58];
                    push_triangle(&mut vertices, corners[i], lower_a, corners[next], side_color);
                    push_triangle(&mut vertices, corners[next], lower_a, lower_b, side_color);
                }
            }
        }

        // The generated Hexel grid is expressed in tangent-local coordinates
        // (east=x, up=y, north=z). Rotate those offsets into planet space before
        // uploading; leaving them in global XYZ makes the local terrain stand
        // upright except at one special orientation, producing the wall/plane
        // split visible in gameplay.
        for vertex in &mut vertices {
            let local = Vec3::from_array(vertex.position);
            let planet_delta = frame.east * local.x as f64
                + frame.up * local.y as f64
                + frame.north * local.z as f64;
            vertex.position = planet_delta.as_vec3().to_array();
        }

        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Near-field six-sided Hexel terrain"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let camera_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Hexel terrain camera-relative uniform"),
            contents: bytemuck::bytes_of(&CameraUniform {
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                world_offset: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Hexel terrain camera-relative bind group"),
            layout: &self.camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        self.planet_hexel_surface = Some(PlanetGpuChunk {
            buffer,
            vertex_count: vertices.len() as u32,
            origin,
            camera_buffer,
            camera_bind_group,
        });
        self.planet_hexel_anchor = Some(origin);
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
        self.update_planet_streaming(camera.position);
        self.update_planet_hexel_surface(camera.position);
        let view_proj = camera.view_projection(self.size);
        let uniform = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            world_offset: [0.0; 4],
        };
        let relative_view_proj = camera.relative_view_projection(self.size);
        let camera_position_d = DVec3::new(
            camera.position.x as f64, camera.position.y as f64, camera.position.z as f64,
        );
        for chunk in self.planet_stream_chunks.values() {
            let offset = chunk.origin - camera_position_d;
            let patch_uniform = CameraUniform {
                view_proj: relative_view_proj.to_cols_array_2d(),
                world_offset: [offset.x as f32, offset.y as f32, offset.z as f32, 0.0],
            };
            self.queue.write_buffer(&chunk.camera_buffer, 0, bytemuck::bytes_of(&patch_uniform));
        }
        if let Some(hexels) = self.planet_hexel_surface.as_ref() {
            let offset = hexels.origin - camera_position_d;
            let hexel_uniform = CameraUniform {
                view_proj: relative_view_proj.to_cols_array_2d(),
                world_offset: [offset.x as f32, offset.y as f32, offset.z as f32, 0.0],
            };
            self.queue.write_buffer(&hexels.camera_buffer, 0, bytemuck::bytes_of(&hexel_uniform));
        }
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
                pass.set_bind_group(0, &chunk.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, chunk.buffer.slice(..));
                pass.draw(0..chunk.vertex_count, 0..1);
            }
            if let Some(hexels) = self.planet_hexel_surface.as_ref() {
                pass.set_bind_group(0, &hexels.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, hexels.buffer.slice(..));
                pass.draw(0..hexels.vertex_count, 0..1);
            }
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
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

#[allow(clippy::too_many_arguments)]
fn append_planetary_trees(
    vertices: &mut Vec<Vertex>,
    key: PlanetChunkKey,
    chunks_per_face: u32,
    topology: &SurfaceTopology,
    terrain: &PlanetGenerator,
    seed: u32,
    edits: &std::collections::BTreeMap<hexel_engine::topology::SurfaceCellId, BlockId>,
    removed_trees: &HashSet<u32>,
) {
    let scale = WorldScale::default();
    for cell in topology.cells() {
        if !planetary_chunk_owns_direction(cell.center, key, chunks_per_face)
            || removed_trees.contains(&cell.id.0)
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

fn ray_triangle_range(vertices: &[Vertex], origin: Vec3, direction: Vec3, start: usize, end: usize) -> Option<f32> {
    let mut nearest: Option<f32> = None;
    for index in (start..end).step_by(3) {
        let a = Vec3::from_array(vertices[index].position);
        let b = Vec3::from_array(vertices[index + 1].position);
        let c = Vec3::from_array(vertices[index + 2].position);
        if let Some(distance) = ray_triangle_distance(origin, direction, a, b, c)
            && nearest.is_none_or(|current| distance < current) {
            nearest = Some(distance);
        }
    }
    nearest
}

fn ray_sphere_distance(origin: Vec3, direction: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let relative = origin - center;
    let b = relative.dot(direction);
    let c = relative.length_squared() - radius * radius;
    let discriminant = b * b - c;
    if discriminant < 0.0 { return None; }
    let root = discriminant.sqrt();
    let near = -b - root;
    let far = -b + root;
    if far < 0.0 { None } else { Some(near.max(0.0)) }
}

fn ray_triangle_distance(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let edge_ab = b - a;
    let edge_ac = c - a;
    let p = direction.cross(edge_ac);
    let determinant = edge_ab.dot(p);
    if determinant.abs() < 1.0e-7 {
        return None;
    }
    let inverse = determinant.recip();
    let from_a = origin - a;
    let u = from_a.dot(p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = from_a.cross(edge_ab);
    let v = direction.dot(q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = edge_ac.dot(q) * inverse;
    (distance > 0.001 && distance.is_finite()).then_some(distance)
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
fn push_triangle(
    vertices: &mut Vec<Vertex>,
    a: Vec3,
    b: Vec3,
    c: Vec3,
    color: [f32; 3],
) {
    for position in [a, b, c] {
        vertices.push(Vertex {
            position: position.to_array(),
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
mod raycast_tests {
    use super::*;


    #[test]
    fn camera_tangent_frame_matches_planetary_frame_handedness() {
        for radial in [
            Vec3::X,
            Vec3::Y,
            -Vec3::Y,
            Vec3::Z,
            Vec3::new(1.0, 2.0, -3.0).normalize(),
        ] {
            let camera = Camera {
                up: radial,
                ..Camera::default()
            };
            let (east, north, up) = camera.tangent_frame();
            let planetary = PlanetaryFrame::at(DVec3::new(
                radial.x as f64,
                radial.y as f64,
                radial.z as f64,
            ));
            assert!((east.as_dvec3() - planetary.east).length() < 1.0e-5);
            assert!((north.as_dvec3() - planetary.north).length() < 1.0e-5);
            assert!((up.as_dvec3() - planetary.up).length() < 1.0e-5);
        }
    }

    #[test]
    fn camera_yaw_zero_faces_planetary_north() {
        let camera = Camera {
            up: Vec3::X,
            yaw: 0.0,
            pitch: 0.0,
            ..Camera::default()
        };
        let planetary = PlanetaryFrame::at(DVec3::X);
        assert!((camera.forward().as_dvec3() - planetary.north).length() < 1.0e-5);
    }

    #[test]
    fn ray_triangle_intersection_returns_nearest_positive_distance() {
        let hit = ray_triangle_distance(
            Vec3::new(0.2, 0.2, -1.0),
            Vec3::Z,
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
        );
        assert!((hit.unwrap() - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn ray_triangle_intersection_rejects_misses_and_behind_origin() {
        assert!(ray_triangle_distance(
            Vec3::new(2.0, 2.0, -1.0),
            Vec3::Z,
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
        ).is_none());
        assert!(ray_triangle_distance(
            Vec3::new(0.2, 0.2, 1.0),
            Vec3::Z,
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
        ).is_none());
    }
}
