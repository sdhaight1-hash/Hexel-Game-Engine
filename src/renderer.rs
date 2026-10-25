use std::{collections::{HashMap, HashSet, VecDeque}, sync::{Arc, mpsc::{self, Receiver, Sender}}, thread};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use hexel_engine::{BlockId, CellPos, ChunkPos, WorldScale, raycast_prism, terrain_height};
use noise::{NoiseFn, Perlin};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

const CHUNK_SIZE: i32 = 16;
const MAX_EDIT_HEIGHT: i32 = 12;
const STREAM_RADIUS_CHUNKS: i32 = 5;
const RENDER_DISTANCE: f32 = 110.0;

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
struct Vertex { position: [f32; 3], color: [f32; 3], uv: [f32; 2], textured: f32 }
impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32];
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress, step_mode: wgpu::VertexStepMode::Vertex, attributes: &Self::ATTRIBUTES }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform { view_proj: [[f32; 4]; 4] }

pub struct Camera { pub position: Vec3, pub yaw: f32, pub pitch: f32, pub speed: f32 }
impl Default for Camera {
    fn default() -> Self { Self { position: Vec3::new(0.0, 15.0, -10.0), yaw: 0.0, pitch: -0.15, speed: 6.5 } }
}
impl Camera {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin() * self.pitch.cos(), self.pitch.sin(), self.yaw.cos() * self.pitch.cos()).normalize_or_zero()
    }
    fn view_projection(&self, size: PhysicalSize<u32>) -> Mat4 {
        let aspect = size.width.max(1) as f32 / size.height.max(1) as f32;
        Mat4::perspective_rh(70.0_f32.to_radians(), aspect, 0.1, 900.0) * Mat4::look_to_rh(self.position, self.forward(), Vec3::Y)
    }
}

struct MeshJob { revision: u64, seed: u32, edits: HashMap<CellPos, BlockId>, chunks: Vec<ChunkPos> }
struct MeshResult { revision: u64, chunks: Vec<(ChunkPos, Vec<Vertex>)> }
struct ChunkMesh {
    buffer: wgpu::Buffer,
    vertex_count: u32,
    bounds_min: Vec3,
    bounds_max: Vec3,
}
pub struct Renderer {
    surface: wgpu::Surface<'static>, device: wgpu::Device, queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration, pipeline: wgpu::RenderPipeline,
    outline_pipeline: wgpu::RenderPipeline, overlay_pipeline: wgpu::RenderPipeline,
    crosshair_buffer: wgpu::Buffer, target_buffer: wgpu::Buffer,
    chunks: HashMap<ChunkPos, ChunkMesh>, camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup, foliage_bind_group: wgpu::BindGroup, size: PhysicalSize<u32>,
    _depth_texture: wgpu::Texture, depth_view: wgpu::TextureView,
    seed: u32, edits: HashMap<CellPos, BlockId>,
    mesh_job_tx: Sender<MeshJob>, mesh_result_rx: Receiver<MeshResult>, dirty_chunks: HashSet<ChunkPos>, edit_revision: u64,
    pending_mesh_chunks: VecDeque<(u64, ChunkPos, Vec<Vertex>)>, pending_mesh_count: usize, pending_mesh_revision: u64,
}
impl Renderer {
    pub async fn new(window: Arc<Window>, seed: u32) -> Result<Self, String> {
        let size = window.inner_size();
        let backends = if cfg!(target_os = "windows") { wgpu::Backends::DX12 } else { wgpu::Backends::all() };
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends, ..Default::default() });
        let surface = instance.create_surface(window).map_err(|e| format!("Could not create graphics surface: {e}"))?;
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: Some(&surface), force_fallback_adapter: false,
        }).await.ok_or_else(|| "No compatible graphics adapter was found".to_string())?;
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Hexel Engine device"), required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(), memory_hints: wgpu::MemoryHints::Performance,
        }, None).await.map_err(|e| format!("Could not create graphics device: {e}"))?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format, width: size.width.max(1), height: size.height.max(1),
            present_mode: caps.present_modes.iter().copied().find(|m| *m == wgpu::PresentMode::Fifo).unwrap_or(caps.present_modes[0]),
            alpha_mode: caps.alpha_modes[0], view_formats: vec![], desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("Hexel world shader"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera uniform"), contents: bytemuck::bytes_of(&CameraUniform { view_proj: Mat4::IDENTITY.to_cols_array_2d() }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera layout"), entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera bind group"), layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() }],
        });
        let foliage_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Tree texture-array layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false }, count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None,
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
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&foliage_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&foliage_sampler) },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("Hexel pipeline layout"), bind_group_layouts: &[&camera_layout, &foliage_layout], push_constant_ranges: &[] });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Hexel prism pipeline"), layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: wgpu::PipelineCompilationOptions::default(), buffers: &[Vertex::layout()] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs_main"), compilation_options: wgpu::PipelineCompilationOptions::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, strip_index_format: None, front_face: wgpu::FrontFace::Ccw, cull_mode: None, unclipped_depth: false, polygon_mode: wgpu::PolygonMode::Fill, conservative: false },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth24Plus, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState::default() }),
            multisample: wgpu::MultisampleState::default(), multiview: None, cache: None,
        });
        let outline_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Voxel target outline pipeline"), layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: wgpu::PipelineCompilationOptions::default(), buffers: &[Vertex::layout()] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs_main"), compilation_options: wgpu::PipelineCompilationOptions::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::LineList, strip_index_format: None, front_face: wgpu::FrontFace::Ccw, cull_mode: None, unclipped_depth: false, polygon_mode: wgpu::PolygonMode::Fill, conservative: false },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth24Plus, depth_write_enabled: false, depth_compare: wgpu::CompareFunction::LessEqual, stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState { constant: -2, slope_scale: -1.0, clamp: 0.0 } }),
            multisample: wgpu::MultisampleState::default(), multiview: None, cache: None,
        });
        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("Crosshair overlay shader"), source: wgpu::ShaderSource::Wgsl(OVERLAY_SHADER.into()) });
        let overlay_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("Crosshair overlay layout"), bind_group_layouts: &[], push_constant_ranges: &[] });
        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Crosshair overlay pipeline"), layout: Some(&overlay_layout),
            vertex: wgpu::VertexState { module: &overlay_shader, entry_point: Some("vs_main"), compilation_options: wgpu::PipelineCompilationOptions::default(), buffers: &[Vertex::layout()] },
            fragment: Some(wgpu::FragmentState { module: &overlay_shader, entry_point: Some("fs_main"), compilation_options: wgpu::PipelineCompilationOptions::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::LineList, strip_index_format: None, front_face: wgpu::FrontFace::Ccw, cull_mode: None, unclipped_depth: false, polygon_mode: wgpu::PolygonMode::Fill, conservative: false },
            depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None, cache: None,
        });
        let crosshair_vertices = [
            Vertex { position: [-0.018, 0.0, 0.0], color: [0.02, 0.02, 0.02], uv: [0.0, 0.0], textured: 0.0 },
            Vertex { position: [ 0.018, 0.0, 0.0], color: [0.02, 0.02, 0.02], uv: [0.0, 0.0], textured: 0.0 },
            Vertex { position: [0.0, -0.025, 0.0], color: [0.02, 0.02, 0.02], uv: [0.0, 0.0], textured: 0.0 },
            Vertex { position: [0.0,  0.025, 0.0], color: [0.02, 0.02, 0.02], uv: [0.0, 0.0], textured: 0.0 },
            Vertex { position: [-0.010, 0.0, 0.0], color: [1.0, 1.0, 1.0], uv: [0.0, 0.0], textured: 0.0 },
            Vertex { position: [ 0.010, 0.0, 0.0], color: [1.0, 1.0, 1.0], uv: [0.0, 0.0], textured: 0.0 },
            Vertex { position: [0.0, -0.015, 0.0], color: [1.0, 1.0, 1.0], uv: [0.0, 0.0], textured: 0.0 },
            Vertex { position: [0.0,  0.015, 0.0], color: [1.0, 1.0, 1.0], uv: [0.0, 0.0], textured: 0.0 },
        ];
        let crosshair_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Centered crosshair"), contents: bytemuck::cast_slice(&crosshair_vertices), usage: wgpu::BufferUsages::VERTEX,
        });
        let target_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Targeted voxel outline"), size: (36 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let (depth_texture, depth_view) = create_depth_target(&device, config.width, config.height);
        let (mesh_job_tx, mesh_job_rx) = mpsc::channel::<MeshJob>();
        let (mesh_result_tx, mesh_result_rx) = mpsc::channel::<MeshResult>();
        thread::Builder::new().name("hexel-mesh-worker".into()).spawn(move || {
            while let Ok(mut job) = mesh_job_rx.recv() {
                while let Ok(newer) = mesh_job_rx.try_recv() { job = newer; }
                let chunks = job.chunks.iter().map(|pos| (*pos, build_chunk_mesh(job.seed, &job.edits, *pos))).collect();
                if mesh_result_tx.send(MeshResult { revision: job.revision, chunks }).is_err() { break; }
            }
        }).map_err(|e| format!("Could not start mesh worker: {e}"))?;
        let mut renderer = Self {
            surface, device, queue, config, pipeline, outline_pipeline, overlay_pipeline, crosshair_buffer, target_buffer, chunks: HashMap::new(),
            camera_buffer, camera_bind_group, foliage_bind_group, size, _depth_texture: depth_texture, depth_view,
            seed, edits: HashMap::new(), mesh_job_tx, mesh_result_rx, dirty_chunks: HashSet::new(), edit_revision: 0,
            pending_mesh_chunks: VecDeque::new(), pending_mesh_count: 0, pending_mesh_revision: 0,
        };
        renderer.update_streaming(Vec3::ZERO);
        Ok(renderer)
    }

    /// True when the player capsule would overlap a generated tree's solid trunk.
    /// Tree roots use the same deterministic hash and reduced-density threshold as meshing.
    pub fn tree_blocks_player(&self, position: Vec3, player_radius: f32, player_height: f32) -> bool {
        let scale = WorldScale::default();
        let cell = Self::cell_from_world(position);
        for q in (cell.q - 2)..=(cell.q + 2) {
            for r in (cell.r - 2)..=(cell.r + 2) {
                let hash = vegetation_hash(self.seed, q, r);
                if hash % 29 != 0 { continue; }
                let terrain = terrain_height(self.seed, q, r);
                let warmth = Perlin::new(self.seed.wrapping_add(0x51ED_270B))
                    .get([q as f64 / 90.0, r as f64 / 90.0]);
                let wetness = Perlin::new(self.seed.wrapping_add(0x51ED_270B))
                    .get([q as f64 / 37.0 + 31.0, r as f64 / 37.0 - 17.0]);
                let conifer = warmth < -0.25 || wetness < -0.32;
                let tropical = warmth > 0.38 && wetness > 0.05;
                let roll = (hash >> 16) % 1000;
                let (min_m, max_m, trunk_radius, giant) = if tropical {
                    if roll < 22 { (55.0_f32, 95.0, 2, true) }
                    else if roll < 105 { (25.0, 45.0, 1, false) }
                    else { (16.0, 32.0, 1, false) }
                } else if conifer {
                    if roll < 8 { (55.0_f32, 95.0, 2, true) }
                    else if roll < 570 { (18.0, 38.0, 1, false) }
                    else { (10.0, 24.0, 1, false) }
                } else if wetness > 0.25 {
                    if roll < 9 { (55.0_f32, 95.0, 2, true) }
                    else if roll < 590 { (18.0, 32.0, 1, false) }
                    else { (10.0, 24.0, 1, false) }
                } else { (6.0, 16.0, 1, false) };
                let variance = ((hash >> 8) % 10_001) as f32 / 10_000.0;
                let mut height_m = min_m + (max_m - min_m) * variance;
                if giant && (hash >> 28) % 4 != 0 { height_m = min_m * 0.55; }
                let layers = (height_m / scale.hexel_height).round().max(3.0) as i32;
                let trunk_top = layers * if tropical && roll < 22 { 3 } else { 2 } / 3;
                let radius_cells = if trunk_radius > 1 { 0.70 } else { 0.43 };
                let root = scale.prism_center(CellPos::new(q, r, terrain + 1));
                let dx = position.x - root.x;
                let dz = position.z - root.z;
                let horizontal_limit = player_radius + scale.hex_radius * radius_cells;
                let player_bottom = position.y;
                let player_top = position.y + player_height;
                let trunk_bottom = (terrain + 1) as f32 * scale.hexel_height;
                let trunk_end = (terrain + 1 + trunk_top) as f32 * scale.hexel_height;
                if dx * dx + dz * dz < horizontal_limit * horizontal_limit
                    && player_top > trunk_bottom && player_bottom < trunk_end {
                    return true;
                }
            }
        }
        false
    }

    pub fn ground_height(&self, q: i32, r: i32) -> f32 {
        let scale = WorldScale::default();
        let terrain_top = terrain_height(self.seed, q, r);
        let has_column_edits = self.edits.keys().any(|cell| cell.q == q && cell.r == r);
        if !has_column_edits {
            return (terrain_top + 1) as f32 * scale.hexel_height;
        }
        let highest = self.edits.keys().filter(|cell| cell.q == q && cell.r == r)
            .map(|cell| cell.y).max().unwrap_or(terrain_top).max(terrain_top);
        for y in (0..=highest).rev() {
            if self.block_at(CellPos::new(q, r, y)).is_solid() {
                return (y + 1) as f32 * scale.hexel_height;
            }
        }
        0.0
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
        CellPos::new(q as i32, r as i32, (position.y / scale.hexel_height).floor() as i32)
    }
    pub fn block_at(&self, cell: CellPos) -> BlockId {
        self.edits.get(&cell).copied().unwrap_or_else(|| generated_block_from_height(cell, terrain_height(self.seed, cell.q, cell.r)))
    }

    /// Text inspection for the "What am I looking at?" debug/gameplay affordance.
    pub fn describe_target(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> String {
        let Some((cell, _)) = self.raycast(origin, direction, max_distance) else {
            return "What am I looking at? Nothing identifiable in reach.".to_string();
        };
        let block = self.block_at(cell);
        let surface = terrain_height(self.seed, cell.q, cell.r);
        let layer = surface - cell.y;
        let (name, detail) = match block {
            BlockId::Grass => ("Grass-covered ground", "Surface cover over a soil layer. Soil fertility is not yet wired into the playable farming loop."),
            BlockId::Dirt if layer <= 3 => ("Topsoil", "Loose near-surface soil. Its color varies by procedural soil patch; nutrient state is not yet persisted per cell."),
            BlockId::Dirt => ("Subsoil", "Weathered material beneath the topsoil."),
            BlockId::Stone => {
                match cell.y.rem_euclid(11) {
                    0..=2 => ("Sedimentary-looking rock", "Pale, layered rock appearance; exact rock strata are not yet connected to world generation."),
                    3..=5 => ("Gray bedrock", "Hard bedrock. Mining deposits and host-rock rules are not yet spawned into playable chunks."),
                    6..=8 => ("Tan bedrock", "Weathered-looking bedrock; the geology module is currently a data foundation."),
                    _ => ("Dark bedrock", "Dense-looking rock. Exact mineral identity and finite deposit state are not yet integrated."),
                }
            },
            BlockId::Foundation => ("Unbreakable foundation", "Below the playable terrain floor."),
            BlockId::Air => ("Air", "An empty cell."),
        };
        format!("What am I looking at?\\n{name}\\n{detail}\\nCell: q={}, r={}, y={} | Surface layer: {}", cell.q, cell.r, cell.y, layer.max(0))
    }
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<(CellPos, CellPos)> {
        let scale = WorldScale::default();
        let dir = direction.normalize_or_zero();
        if dir == Vec3::ZERO { return None; }

        // Walk the ray to find candidate cells, then use the exact prism intersection
        // face to choose the adjacent cell. The last sampled air cell can be diagonally
        // adjacent or even on the wrong layer near hex corners.
        let mut distance = 0.0;
        while distance <= max_distance {
            let cell = Self::cell_from_world(origin + dir * distance);
            if self.block_at(cell).is_solid() {
                let center = scale.prism_center(cell);
                if let Some((hit_distance, face)) = raycast_prism(
                    origin, dir, center, scale.hex_radius, scale.hexel_height, max_distance,
                ) && hit_distance <= distance + 0.15 {
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
        if cell.y < 0 || cell.y > 255 { return; }
        if self.block_at(cell).is_indestructible() { return; }
        self.edits.insert(cell, block);
        let (chunk_pos, local) = cell.chunk_and_local();
        let mut affected = vec![chunk_pos];
        if local[0] == 0 { affected.push(ChunkPos { q: chunk_pos.q - 1, ..chunk_pos }); }
        if local[0] == 15 { affected.push(ChunkPos { q: chunk_pos.q + 1, ..chunk_pos }); }
        if local[1] == 0 { affected.push(ChunkPos { r: chunk_pos.r - 1, ..chunk_pos }); }
        if local[1] == 15 { affected.push(ChunkPos { r: chunk_pos.r + 1, ..chunk_pos }); }
        if local[2] == 0 { affected.push(ChunkPos { y: chunk_pos.y - 1, ..chunk_pos }); }
        if local[2] == 15 { affected.push(ChunkPos { y: chunk_pos.y + 1, ..chunk_pos }); }
        affected.sort_by_key(|p| (p.q, p.r, p.y));
        affected.dedup();
        self.dirty_chunks.extend(affected.iter().copied());
        self.edit_revision = self.edit_revision.wrapping_add(1);
        let chunks = affected.into_iter().filter(|p| self.chunks.contains_key(p)).collect();
        let _ = self.mesh_job_tx.send(MeshJob { revision: self.edit_revision, seed: self.seed, edits: self.edits.clone(), chunks });
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
                if dq * dq + dr * dr > STREAM_RADIUS_CHUNKS * STREAM_RADIUS_CHUNKS { continue; }
                for dy in -1..=1 {
                    let cy = center.y + dy;
                    if cy >= 0 { wanted.insert(ChunkPos { q: center.q + dq, r: center.r + dr, y: cy }); }
                }
            }
        }
        self.chunks.retain(|pos, _| wanted.contains(pos));
        let missing: Vec<_> = wanted.iter().copied().filter(|pos| !self.chunks.contains_key(pos)).collect();
        for pos in missing {
            let mesh = self.create_chunk(pos);
            self.chunks.insert(pos, mesh);
        }
    }

    fn poll_mesh_results(&mut self) {
        while let Ok(result) = self.mesh_result_rx.try_recv() {
            if result.revision != self.edit_revision { continue; }
            let revision = result.revision;
            self.pending_mesh_chunks.clear();
            self.pending_mesh_chunks.extend(result.chunks.into_iter().map(|(index, vertices)| (revision, index, vertices)));
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
                    label: Some("Empty hexel chunk"), size: 4,
                    usage: wgpu::BufferUsages::VERTEX, mapped_at_creation: false,
                })
            } else {
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Updated hexel chunk mesh"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            };
            if let Some(chunk) = self.chunks.get_mut(&index) { chunk.buffer = buffer; chunk.vertex_count = vertex_count; }
            self.pending_mesh_count = self.pending_mesh_count.saturating_sub(1);
            if self.pending_mesh_count == 0 {
                self.dirty_chunks.clear();
            }
        }
    }
    fn create_chunk(&self, pos: ChunkPos) -> ChunkMesh {
        let q_start = pos.q * CHUNK_SIZE;
        let r_start = pos.r * CHUNK_SIZE;
        let q_end = q_start + CHUNK_SIZE;
        let r_end = r_start + CHUNK_SIZE;
        let vertices = build_chunk_mesh(self.seed, &self.edits, pos);
        let vertex_count = vertices.len() as u32;
        let buffer = if vertices.is_empty() {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Empty hexel chunk"), size: 4, usage: wgpu::BufferUsages::VERTEX, mapped_at_creation: false,
            })
        } else {
            self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Hexel chunk mesh"), contents: bytemuck::cast_slice(&vertices), usage: wgpu::BufferUsages::VERTEX,
            })
        };
        let scale = WorldScale::default();
        let mut min = Vec3::new(f32::INFINITY, 0.0, f32::INFINITY);
        let y_start = pos.y * CHUNK_SIZE;
        let y_end = y_start + CHUNK_SIZE;
        let mut max = Vec3::new(f32::NEG_INFINITY, if pos.y == 0 { (y_end as f32 * scale.hexel_height).max(105.0) } else { y_end as f32 * scale.hexel_height }, f32::NEG_INFINITY);
        for q in [q_start, q_end - 1] {
            for r in [r_start, r_end - 1] {
                let center = scale.prism_center(CellPos::new(q, r, 0));
                min.x = min.x.min(center.x); min.z = min.z.min(center.z);
                max.x = max.x.max(center.x); max.z = max.z.max(center.z);
            }
        }
        min.x -= scale.hex_radius * 2.0; min.z -= scale.hex_radius * 2.0;
        max.x += scale.hex_radius * 2.0; max.z += scale.hex_radius * 2.0;
        min.y = y_start as f32 * scale.hexel_height;
        ChunkMesh { buffer, vertex_count, bounds_min: min, bounds_max: max }
    }
    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 { return; }
        self.size = size; self.config.width = size.width; self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        let (depth_texture, depth_view) = create_depth_target(&self.device, self.config.width, self.config.height);
        self._depth_texture = depth_texture;
        self.depth_view = depth_view;
    }
    pub fn render(&mut self, camera: &Camera, selected_slot: usize, inventory_open: bool, item_counts: [u16; 36]) -> Result<(), wgpu::SurfaceError> {
        self.poll_mesh_results();
        let uniform = CameraUniform { view_proj: camera.view_projection(self.size).to_cols_array_2d() };
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
        let target_vertices = self.raycast(camera.position, camera.forward(), 8.0).map(|(cell, _)| build_target_outline(cell)).unwrap_or_default();
        if !target_vertices.is_empty() { self.queue.write_buffer(&self.target_buffer, 0, bytemuck::cast_slice(&target_vertices)); }
        let mut hud_vertices: Vec<Vertex> = Vec::with_capacity(600);
        let aspect = self.size.width.max(1) as f32 / self.size.height.max(1) as f32;
        let mut hud_line = |x1: f32, y1: f32, x2: f32, y2: f32, color: [f32; 3]| {
            hud_vertices.push(Vertex { position: [x1, y1, 0.0], color, uv: [0.0, 0.0], textured: 0.0 });
            hud_vertices.push(Vertex { position: [x2, y2, 0.0], color, uv: [0.0, 0.0], textured: 0.0 });
        };
        let sw = (0.075 / aspect).min(0.085); let sh = 0.105; let gap = 0.012 / aspect;
        let left = -(9.0 * sw + 8.0 * gap) * 0.5; let bottom = -0.91;
        for i in 0..9 {
            let x1 = left + i as f32 * (sw + gap); let x2 = x1 + sw; let y1 = bottom; let y2 = bottom + sh;
            let col = if i == selected_slot { [1.0, 0.78, 0.12] } else { [0.95, 0.97, 1.0] };
            hud_line(x1,y1,x2,y1,col); hud_line(x2,y1,x2,y2,col); hud_line(x2,y2,x1,y2,col); hud_line(x1,y2,x1,y1,col);
            for tick in 0..item_counts[i].min(9) { let tx=x1+0.012/aspect+tick as f32*0.005/aspect; hud_line(tx,y1+0.012,tx,y1+0.028,[0.4,1.0,0.6]); }
        }
        if inventory_open {
            let w=(0.055/aspect).min(0.065); let h=0.075; let gx=0.009/aspect; let gy=0.018;
            let start_x=-(9.0*w+8.0*gx)*0.5; let start_y=0.48;
            for i in 0..36 {
                let x1=start_x+(i%9) as f32*(w+gx); let y1=start_y-(i/9) as f32*(h+gy); let x2=x1+w; let y2=y1+h;
                let col=if i==selected_slot {[1.0,0.78,0.12]} else {[0.88,0.92,0.98]};
                hud_line(x1,y1,x2,y1,col); hud_line(x2,y1,x2,y2,col); hud_line(x2,y2,x1,y2,col); hud_line(x1,y2,x1,y1,col);
                for tick in 0..item_counts[i].min(5) { let tx=x1+0.008/aspect+tick as f32*0.004/aspect; hud_line(tx,y1+0.008,tx,y1+0.02,[0.4,1.0,0.6]); }
            }
        }
        drop(hud_line);
        let hud_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("HUD overlay"), contents: bytemuck::cast_slice(&hud_vertices), usage: wgpu::BufferUsages::VERTEX });
        let frame = self.surface.get_current_texture()?;
        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Hexel frame encoder") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Hexel world pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.52, g: 0.72, b: 0.88, a: 1.0 }), store: wgpu::StoreOp::Store,
                } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &self.depth_view, depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store,
                }), stencil_ops: None }), occlusion_query_set: None, timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.foliage_bind_group, &[]);
            let view_proj = camera.view_projection(self.size);
            let forward = camera.forward();
            for chunk in self.chunks.values() {
                if chunk.vertex_count == 0 || !chunk_visible(view_proj, camera.position, forward, chunk) { continue; }
                pass.set_vertex_buffer(0, chunk.buffer.slice(..));
                pass.draw(0..chunk.vertex_count, 0..1);
            }
            if !target_vertices.is_empty() {
                pass.set_pipeline(&self.outline_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.target_buffer.slice(..));
                pass.draw(0..target_vertices.len() as u32, 0..1);
            }
        }
        {
            let mut overlay = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Crosshair overlay pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None, occlusion_query_set: None, timestamp_writes: None,
            });
            overlay.set_pipeline(&self.overlay_pipeline);
            overlay.set_vertex_buffer(0, self.crosshair_buffer.slice(..));
            overlay.draw(0..4, 0..1);
            overlay.draw(4..8, 0..1);
            overlay.set_vertex_buffer(0, hud_buffer.slice(..));
            overlay.draw(0..hud_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish())); frame.present(); Ok(())
    }
}


fn generated_block_from_height(cell: CellPos, height: i32) -> BlockId {
    if cell.y < 0 { BlockId::Foundation }
    else if cell.y > height { BlockId::Air }
    else if cell.y == height { BlockId::Grass }
    else if cell.y >= height - 3 { BlockId::Dirt }
    else { BlockId::Stone }
}

fn build_target_outline(cell: CellPos) -> Vec<Vertex> {
    let scale = WorldScale::default();
    let center = scale.prism_center(cell);
    let radius = scale.hex_radius * 1.025;
    let bottom = cell.y as f32 * scale.hexel_height - 0.002;
    let top = (cell.y + 1) as f32 * scale.hexel_height + 0.002;
    let ring: [Vec3; 6] = std::array::from_fn(|i| {
        let angle = (30.0_f32 + i as f32 * 60.0).to_radians();
        Vec3::new(center.x + angle.cos() * radius, 0.0, center.z + angle.sin() * radius)
    });
    let color = [1.0, 0.92, 0.12];
    let mut vertices = Vec::with_capacity(36);
    let mut edge = |a: Vec3, b: Vec3| {
        vertices.push(Vertex { position: a.to_array(), color, uv: [0.0, 0.0], textured: 0.0 });
        vertices.push(Vertex { position: b.to_array(), color, uv: [0.0, 0.0], textured: 0.0 });
    };
    for i in 0..6 {
        let next = (i + 1) % 6;
        edge(Vec3::new(ring[i].x, bottom, ring[i].z), Vec3::new(ring[next].x, bottom, ring[next].z));
        edge(Vec3::new(ring[i].x, top, ring[i].z), Vec3::new(ring[next].x, top, ring[next].z));
        edge(Vec3::new(ring[i].x, bottom, ring[i].z), Vec3::new(ring[i].x, top, ring[i].z));
    }
    vertices
}

fn create_depth_target(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Reusable frame depth buffer"),
        size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth24Plus, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
fn chunk_visible(view_proj: Mat4, camera_position: Vec3, forward: Vec3, chunk: &ChunkMesh) -> bool {
    let center = (chunk.bounds_min + chunk.bounds_max) * 0.5;
    let radius = (chunk.bounds_max - center).length();
    let to_center = center - camera_position;
    if to_center.length() - radius > RENDER_DISTANCE || to_center.dot(forward) < -radius { return false; }
    let corners = [
        Vec3::new(chunk.bounds_min.x, chunk.bounds_min.y, chunk.bounds_min.z),
        Vec3::new(chunk.bounds_min.x, chunk.bounds_min.y, chunk.bounds_max.z),
        Vec3::new(chunk.bounds_min.x, chunk.bounds_max.y, chunk.bounds_min.z),
        Vec3::new(chunk.bounds_min.x, chunk.bounds_max.y, chunk.bounds_max.z),
        Vec3::new(chunk.bounds_max.x, chunk.bounds_min.y, chunk.bounds_min.z),
        Vec3::new(chunk.bounds_max.x, chunk.bounds_min.y, chunk.bounds_max.z),
        Vec3::new(chunk.bounds_max.x, chunk.bounds_max.y, chunk.bounds_min.z),
        Vec3::new(chunk.bounds_max.x, chunk.bounds_max.y, chunk.bounds_max.z),
    ];
    let clip = corners.map(|p| view_proj * p.extend(1.0));
    if clip.iter().all(|p| p.x < -p.w) || clip.iter().all(|p| p.x > p.w)
        || clip.iter().all(|p| p.y < -p.w) || clip.iter().all(|p| p.y > p.w)
        || clip.iter().all(|p| p.z < 0.0) || clip.iter().all(|p| p.z > p.w) { return false; }
    true
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
            if cell.y < 0 { return BlockId::Foundation; }
            let height = *heights.get(&(cell.q, cell.r)).unwrap_or(&0);
            if cell.y > height { BlockId::Air }
            else if cell.y == height { BlockId::Grass }
            else if cell.y >= height - 3 { BlockId::Dirt }
            else { BlockId::Stone }
        })
    };
    let mut edited_tops = HashMap::<(i32, i32), i32>::new();
    for cell in edits.keys() {
        if (q_start..q_end).contains(&cell.q) && (r_start..r_end).contains(&cell.r) {
            edited_tops.entry((cell.q, cell.r))
                .and_modify(|top| *top = (*top).max(cell.y))
                .or_insert(cell.y);
        }
    }
    for q in q_start..q_end {
        for r in r_start..r_end {
            let terrain_top = *heights.get(&(q, r)).unwrap_or(&0);
            let edited_top = edited_tops.get(&(q, r)).copied().unwrap_or(terrain_top);
            let min_y = y_start.max(0);
            let max_y = y_end.min((terrain_top + MAX_EDIT_HEIGHT).max(edited_top).max(terrain_top).min(256) + 1);
            for y in min_y..max_y {
                let cell = CellPos::new(q, r, y);
                let block = block_at(cell);
                if block == BlockId::Air { continue; }
                let center = scale.prism_center(cell);
                let bottom_y = y as f32 * scale.hexel_height;
                let top_y = (y + 1) as f32 * scale.hexel_height;
                let ring = std::array::from_fn::<_, 6, _>(|i| {
                    let angle = (30.0_f32 + i as f32 * 60.0).to_radians();
                    Vec3::new(center.x + angle.cos() * scale.hex_radius, center.y, center.z + angle.sin() * scale.hex_radius)
                });
                let base = block_color_at(block, cell, seed);
                if !block_at(CellPos::new(q, r, y + 1)).is_solid() {
                    let top = [(base[0] * 1.12).min(1.0), (base[1] * 1.12).min(1.0), (base[2] * 1.12).min(1.0)];
                    for i in 0..6 {
                        push_triangle(&mut vertices, Vec3::new(center.x, top_y, center.z), Vec3::new(ring[i].x, top_y, ring[i].z), Vec3::new(ring[(i + 1) % 6].x, top_y, ring[(i + 1) % 6].z), top);
                    }
                }
                let neighbors = [(0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1), (1, 0)];
                for (i, (dq, dr)) in neighbors.into_iter().enumerate() {
                    let nq = q + dq;
                    let nr = r + dr;
                    if block_at(CellPos::new(nq, nr, y)).is_solid() { continue; }
                    let a = ring[i]; let b = ring[(i + 1) % 6];
                    let shade = 0.48 + ((i * 7 % 6) as f32) * 0.055;
                    let side = [base[0] * shade, base[1] * shade, base[2] * shade];
                    push_triangle(&mut vertices, Vec3::new(a.x, bottom_y, a.z), Vec3::new(a.x, top_y, a.z), Vec3::new(b.x, top_y, b.z), side);
                    push_triangle(&mut vertices, Vec3::new(a.x, bottom_y, a.z), Vec3::new(b.x, top_y, b.z), Vec3::new(b.x, bottom_y, b.z), side);
                }
            }
        }
    }
    // Coordinate-hashed trees are stable across chunk reloads.
    let vegetation_climate = Perlin::new(seed.wrapping_add(0x51ED_270B));
    for q in q_start..q_end {
        for r in r_start..r_end {
            let height = *heights.get(&(q, r)).unwrap_or(&0);
            if height < 0 || chunk.y != height.div_euclid(CHUNK_SIZE) { continue; }
            let hash = vegetation_hash(seed, q, r);
            if hash % 29 != 0 { continue; } // Reduced baseline density; biome patches are tuned below.
            let warmth = vegetation_climate.get([q as f64 / 90.0, r as f64 / 90.0]);
            let wetness = vegetation_climate.get([q as f64 / 37.0 + 31.0, r as f64 / 37.0 - 17.0]);
            let conifer = warmth < -0.25 || wetness < -0.32;
            let tropical = warmth > 0.38 && wetness > 0.05;
            let roll = (hash >> 16) % 1000;
            // Adult height ranges are metres, converted to hexel layers below.
            let (species, min_m, max_m, trunk_radius, crown_radius, giant) = if tropical {
                if roll < 22 { (4_u8, 55.0_f32, 95.0, 2, 4, true) }
                else if roll < 105 { (5, 25.0, 45.0, 1, 3, false) }
                else { (3, 16.0, 32.0, 1, 3, false) }
            } else if conifer {
                if roll < 8 { (4, 55.0_f32, 95.0, 2, 4, true) }
                else if roll < 570 { (2, 18.0, 38.0, 1, 2, false) }
                else { (1, 10.0, 24.0, 1, 2, false) }
            } else if wetness > 0.25 {
                if roll < 9 { (4, 55.0_f32, 95.0, 2, 4, true) }
                else if roll < 590 { (0, 18.0, 32.0, 1, 3, false) }
                else { (1, 10.0, 24.0, 1, 2, false) }
            } else { (6, 6.0, 16.0, 1, 2, false) };
            let variance = ((hash >> 8) % 10_001) as f32 / 10_000.0;
            let mut height_m = min_m + (max_m - min_m) * variance;
            if giant && (hash >> 28) % 4 != 0 { height_m = min_m * 0.55; }
            let layers = (height_m / scale.hexel_height).round().max(3.0) as i32;
            let trunk = if species == 1 { [0.78, 0.75, 0.65] } else { [0.38, 0.25, 0.14] };
            let foliage = match species { 1 => [0.24, 0.40, 0.19], 2 => [0.12, 0.30, 0.15], 4 => [0.07, 0.31, 0.11], 5 => [0.10, 0.39, 0.13], 6 => [0.30, 0.39, 0.17], _ => [0.18, 0.46, 0.15] };
            let (leaf_layer, bark_layer) = match species {
                0 => (7, 22), // oak
                1 => (3, 17), // birch
                2 => (8, 23), // pine
                3 if tropical => (5, 19), // kapok
                3 => (0, 15), // acacia-like
                4 if tropical => (9, 24), // purpleheart
                4 => (7, 22), // large temperate tree
                5 => (1, 16), // bald cypress
                6 => (6, 21), // maple
                _ => (7, 22),
            };
            append_hexel_tree(&mut vertices, CellPos::new(q, r, height + 1), layers, trunk_radius, crown_radius, (trunk, foliage), species, hash, leaf_layer, bark_layer, scale);
        }
    }
    vertices
}
fn block_color_at(block: BlockId, cell: CellPos, seed: u32) -> [f32; 3] {
    let hash = vegetation_hash(seed ^ (cell.y as u32).wrapping_mul(0x27D4_EB2D), cell.q, cell.r);
    match block {
        BlockId::Grass => match hash % 9 { 0 => [0.34, 0.52, 0.24], 1 => [0.43, 0.57, 0.29], _ => [0.25, 0.66, 0.25] },
        BlockId::Dirt => match hash % 5 { 0 => [0.38, 0.25, 0.15], 1 => [0.55, 0.39, 0.24], 2 => [0.31, 0.22, 0.15], _ => [0.47, 0.29, 0.16] },
        BlockId::Stone => match cell.y.rem_euclid(11) { 0..=2 => [0.49, 0.48, 0.43], 3..=5 => [0.43, 0.46, 0.49], 6..=8 => [0.53, 0.50, 0.45], _ => [0.38, 0.40, 0.43] },
        BlockId::Foundation => [0.22, 0.23, 0.27], BlockId::Air => [0.0, 0.0, 0.0]
    }
}

fn vegetation_hash(seed: u32, q: i32, r: i32) -> u32 {
    let mut x = seed ^ (q as u32).wrapping_mul(0x9E37_79B9) ^ (r as u32).wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16; x = x.wrapping_mul(0x7FEB_352D); x ^= x >> 15; x = x.wrapping_mul(0x846C_A68B); x ^ (x >> 16)
}

fn append_hexel_tree(vertices: &mut Vec<Vertex>, root: CellPos, height_layers: i32, trunk_radius: i32, crown_radius: i32, colors: ([f32; 3], [f32; 3]), species: u8, seed_hash: u32, leaf_layer: u32, bark_layer: u32, scale: WorldScale) {
    let (trunk_color, leaf_color) = colors;
    let trunk_top = height_layers * if species == 4 { 3 } else { 2 } / 3;
    for y in 0..trunk_top {
        let taper = y as f32 / trunk_top.max(1) as f32;
        let radius = if trunk_radius > 1 && taper > 0.42 { 1 } else { trunk_radius };
        for dq in -(radius - 1)..=radius - 1 {
            for dr in -(radius - 1)..=radius - 1 {
                if radius == 1 || (dq.abs() + dr.abs() + (dq + dr).abs()) / 2 < radius {
                    append_hexel_prism(vertices, CellPos::new(root.q + dq, root.r + dr, root.y + y), scale, trunk_color, true, bark_layer);
                }
            }
        }
    }
    let crown_start = (height_layers as f32 * if species == 2 || species == 6 { 0.30 } else { 0.48 }) as i32;
    let crown_layers = (height_layers - crown_start).max(2);

    // Structural branches radiate from the trunk before leaf clusters are added.
    // Direction vectors use the six neighbors of the hex grid, keeping branch
    // placement aligned with the engine's hexagonal-prism world.
    let directions = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];
    let branch_levels = if species == 2 { 5 } else if species == 4 || species == 5 { 3 } else { 4 };
    for level in 0..branch_levels {
        let y = crown_start + crown_layers * (level + 1) / (branch_levels + 1);
        let dir_index = (vegetation_hash(seed_hash, root.q + level * 13, root.r - level * 7) as usize) % directions.len();
        let (dq, dr) = directions[dir_index];
        let length = if species == 2 { (crown_radius - level / 2).max(1) } else { (crown_radius - 1).max(1) };
        for step in 1..=length {
            let branch_cell = CellPos::new(root.q + dq * step, root.r + dr * step, root.y + y + step / 3);
            append_hexel_prism(vertices, branch_cell, scale, [trunk_color[0] * 0.94, trunk_color[1] * 0.94, trunk_color[2] * 0.94], true, bark_layer);
        }
    }

    for cy in 0..crown_layers {
        if cy % 2 != 0 { continue; }
        let progress = cy as f32 / crown_layers as f32;
        let radius = if species == 2 { ((crown_radius as f32) * (1.0 - progress * 0.88)).ceil() as i32 }
            else if species == 6 { ((crown_radius as f32) * (1.0 - progress * 0.72)).ceil() as i32 }
            else if species == 4 || species == 5 { ((crown_radius as f32) * (0.55 + (1.0 - progress) * 0.45)).ceil() as i32 }
            else { ((crown_radius as f32) * (0.72 + (1.0 - progress) * 0.28)).ceil() as i32 };
        for dq in -radius..=radius {
            for dr in -radius..=radius {
                let distance = (dq.abs() + dr.abs() + (dq + dr).abs()) / 2;
                let irregular = vegetation_hash(seed_hash, root.q + dq + cy * 17, root.r + dr - cy * 11) % 100;
                let edge_noise = if species == 2 { 8 } else { 34 };
                let keep = distance <= radius && (distance < radius || irregular > edge_noise);
                if !keep || (cy < crown_layers / 3 && dq == 0 && dr == 0) { continue; }
                // Avoid a solid voxel-like leaf mass: only a deterministic subset of
                // canopy cells receives a textured, crossed leaf cluster. This gives
                // branches and sky gaps room to read through the silhouette.
                if irregular % if species == 2 { 4 } else { 3 } == 0 || distance == radius {
                    let cluster_cell = CellPos::new(root.q + dq, root.r + dr, root.y + crown_start + cy);
                    append_foliage_cluster(vertices, cluster_cell, scale, leaf_color, seed_hash ^ irregular as u32, leaf_layer);
                }
            }
        }
    }
}

fn append_foliage_cluster(vertices: &mut Vec<Vertex>, cell: CellPos, scale: WorldScale, color: [f32; 3], seed: u32, leaf_layer: u32) {
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
        let tint = [color[0] * (0.88 + angle_deg / 600.0), color[1], color[2] * (1.08 - angle_deg / 900.0)];
        push_tree_textured_triangle(vertices, [(a, [0.0, 1.0]), (b, [1.0, 1.0]), (c, [1.0, 0.0])], tint, leaf_layer);
        push_tree_textured_triangle(vertices, [(a, [0.0, 1.0]), (c, [1.0, 0.0]), (d, [0.0, 0.0])], tint, leaf_layer);
        // Reverse winding makes the card visible from its back side without
        // relying on a double-sided material setting.
        push_tree_textured_triangle(vertices, [(b, [1.0, 1.0]), (a, [0.0, 1.0]), (d, [0.0, 0.0])], tint, leaf_layer);
        push_tree_textured_triangle(vertices, [(b, [1.0, 1.0]), (d, [0.0, 0.0]), (c, [1.0, 0.0])], tint, leaf_layer);
    }
}

fn append_hexel_prism(vertices: &mut Vec<Vertex>, cell: CellPos, scale: WorldScale, color: [f32; 3], wood: bool, texture_layer: u32) {
    let center = scale.prism_center(cell);
    let bottom = cell.y as f32 * scale.hexel_height;
    let top = (cell.y + 1) as f32 * scale.hexel_height;
    let radius = scale.hex_radius * if wood { 0.42 } else { 0.50 };
    let ring: [Vec3; 6] = std::array::from_fn(|i| {
        let angle = (30.0_f32 + i as f32 * 60.0).to_radians();
        Vec3::new(center.x + angle.cos() * radius, center.y, center.z + angle.sin() * radius)
    });
    let top_color = [(color[0] * 1.10).min(1.0), (color[1] * 1.10).min(1.0), (color[2] * 1.10).min(1.0)];
    for i in 0..6 {
        let j = (i + 1) % 6;
        if wood {
            push_triangle(vertices, Vec3::new(center.x, top, center.z), Vec3::new(ring[i].x, top, ring[i].z), Vec3::new(ring[j].x, top, ring[j].z), top_color);
        } else {
            push_textured_triangle(vertices, [
                (Vec3::new(center.x, top, center.z), [0.5, 0.5]),
                (Vec3::new(ring[i].x, top, ring[i].z), [0.0, 0.0]),
                (Vec3::new(ring[j].x, top, ring[j].z), [1.0, 0.0]),
            ], top_color);
        }
        let shade = 0.55 + i as f32 * 0.045;
        let side = [color[0] * shade, color[1] * shade, color[2] * shade];
        if wood {
            let a_bottom = Vec3::new(ring[i].x, bottom, ring[i].z);
            let a_top = Vec3::new(ring[i].x, top, ring[i].z);
            let b_bottom = Vec3::new(ring[j].x, bottom, ring[j].z);
            let b_top = Vec3::new(ring[j].x, top, ring[j].z);
            push_tree_textured_triangle(vertices, [(a_bottom, [0.0, cell.y as f32 * 0.35 + 1.0]), (a_top, [0.0, cell.y as f32 * 0.35]), (b_top, [1.0, cell.y as f32 * 0.35])], side, texture_layer);
            push_tree_textured_triangle(vertices, [(a_bottom, [0.0, cell.y as f32 * 0.35 + 1.0]), (b_top, [1.0, cell.y as f32 * 0.35]), (b_bottom, [1.0, cell.y as f32 * 0.35 + 1.0])], side, texture_layer);
        } else {
            let a_bottom = Vec3::new(ring[i].x, bottom, ring[i].z);
            let a_top = Vec3::new(ring[i].x, top, ring[i].z);
            let b_bottom = Vec3::new(ring[j].x, bottom, ring[j].z);
            let b_top = Vec3::new(ring[j].x, top, ring[j].z);
            push_textured_triangle(vertices, [(a_bottom, [0.0, 1.0]), (a_top, [0.0, 0.0]), (b_top, [1.0, 0.0])], side);
            push_textured_triangle(vertices, [(a_bottom, [0.0, 1.0]), (b_top, [1.0, 0.0]), (b_bottom, [1.0, 1.0])], side);
        }
    }
}
fn push_triangle(vertices: &mut Vec<Vertex>, a: Vec3, b: Vec3, c: Vec3, color: [f32; 3]) {
    for point in [a, b, c] { vertices.push(Vertex { position: point.to_array(), color, uv: [0.0, 0.0], textured: 0.0 }); }
}

fn push_textured_triangle(vertices: &mut Vec<Vertex>, points: [(Vec3, [f32; 2]); 3], color: [f32; 3]) {
    for (point, uv) in points {
        vertices.push(Vertex { position: point.to_array(), color, uv, textured: 1.0 });
    }
}

fn push_tree_textured_triangle(vertices: &mut Vec<Vertex>, points: [(Vec3, [f32; 2]); 3], color: [f32; 3], layer: u32) {
    for (point, uv) in points {
        vertices.push(Vertex { position: point.to_array(), color, uv, textured: (layer + 2) as f32 });
    }
}

fn create_tree_texture_array(device: &wgpu::Device, queue: &wgpu::Queue) -> (wgpu::Texture, wgpu::Sampler) {
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
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: PATHS.len() as u32 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, relative_path) in PATHS.iter().enumerate() {
        let full_path = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), relative_path);
        let pixels = match std::fs::read(&full_path).ok().and_then(|bytes| image::load_from_memory(&bytes).ok()) {
            Some(image) => image.resize_exact(SIZE, SIZE, image::imageops::FilterType::Nearest).to_rgba8().into_raw(),
            None => {
                eprintln!("Hexel Engine: fallback texture used for missing/unreadable asset {}", relative_path);
                fallback_tree_texture(layer, SIZE)
            }
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture, mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 },
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(SIZE * 4), rows_per_image: Some(SIZE) },
            wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        );
    }
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Nearest-filtered tree texture sampler"),
        address_mode_u: wgpu::AddressMode::Repeat, address_mode_v: wgpu::AddressMode::Repeat, address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest, min_filter: wgpu::FilterMode::Nearest, mipmap_filter: wgpu::FilterMode::Nearest,
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
                let leaf = dx * dx * 2.0 + dy * dy * 1.4 < 0.20 && (x + y * 3 + layer as u32) % 5 != 0;
                pixels[i..i + 4].copy_from_slice(if leaf { &[43, 115, 39, 255] } else { &[0, 0, 0, 0] });
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
