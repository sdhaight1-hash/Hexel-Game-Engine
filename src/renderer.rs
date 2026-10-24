use std::{collections::HashMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use hexel_engine::{BlockId, CellPos, WorldScale, generated_block, raycast_prism, terrain_height};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

const CHUNKS_Q: i32 = 8;
const CHUNKS_R: i32 = 4;
const CHUNK_SIZE: i32 = 16;
const Q_START: i32 = -(CHUNKS_Q * CHUNK_SIZE) / 2;
const R_START: i32 = -(CHUNKS_R * CHUNK_SIZE) / 2;
const Q_END: i32 = Q_START + CHUNKS_Q * CHUNK_SIZE;
const R_END: i32 = R_START + CHUNKS_R * CHUNK_SIZE;
const MAX_EDIT_HEIGHT: i32 = 12;

const SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32>, };
@group(0) @binding(0) var<uniform> camera: Camera;
struct VertexInput { @location(0) position: vec3<f32>, @location(1) color: vec3<f32>, };
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) color: vec3<f32>, };
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = camera.view_proj * vec4<f32>(input.position, 1.0);
    output.color = input.color;
    return output;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> { return vec4<f32>(input.color, 1.0); }
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex { position: [f32; 3], color: [f32; 3] }
impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
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
    pub fn right(&self) -> Vec3 { Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin()).normalize_or_zero() }
    fn view_projection(&self, size: PhysicalSize<u32>) -> Mat4 {
        let aspect = size.width.max(1) as f32 / size.height.max(1) as f32;
        Mat4::perspective_rh(70.0_f32.to_radians(), aspect, 0.1, 900.0) * Mat4::look_to_rh(self.position, self.forward(), Vec3::Y)
    }
}

pub struct Renderer {
    surface: wgpu::Surface<'static>, device: wgpu::Device, queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration, pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer, vertex_count: u32, camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup, size: PhysicalSize<u32>,
    seed: u32, edits: HashMap<CellPos, BlockId>,
}
impl Renderer {
    pub async fn new(window: Arc<Window>, seed: u32) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("Hexel pipeline layout"), bind_group_layouts: &[&camera_layout], push_constant_ranges: &[] });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Hexel prism pipeline"), layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: wgpu::PipelineCompilationOptions::default(), buffers: &[Vertex::layout()] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs_main"), compilation_options: wgpu::PipelineCompilationOptions::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, strip_index_format: None, front_face: wgpu::FrontFace::Ccw, cull_mode: None, unclipped_depth: false, polygon_mode: wgpu::PolygonMode::Fill, conservative: false },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth24Plus, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState::default() }),
            multisample: wgpu::MultisampleState::default(), multiview: None, cache: None,
        });
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("Initial terrain buffer"), size: 4, usage: wgpu::BufferUsages::VERTEX, mapped_at_creation: false });
        let mut renderer = Self {
            surface, device, queue, config, pipeline, vertex_buffer, vertex_count: 0,
            camera_buffer, camera_bind_group, size, seed, edits: HashMap::new(),
        };
        renderer.rebuild_mesh();
        Ok(renderer)
    }

    pub fn seed(&self) -> u32 { self.seed }
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
        CellPos::new(q_float.round() as i32, r_float.round() as i32, (position.y / scale.hexel_height).floor() as i32)
    }
    pub fn block_at(&self, cell: CellPos) -> BlockId {
        self.edits.get(&cell).copied().unwrap_or_else(|| generated_block(self.seed, cell))
    }
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<(CellPos, CellPos)> {
        let scale = WorldScale::default();
        let dir = direction.normalize_or_zero();
        if dir == Vec3::ZERO { return None; }
        let mut last_air = None;
        let mut distance = 0.0;
        while distance <= max_distance {
            let cell = Self::cell_from_world(origin + dir * distance);
            if self.block_at(cell).is_solid() {
                let center = scale.prism_center(cell);
                if let Some((hit_distance, _)) = raycast_prism(origin, dir, center, scale.hex_radius, scale.hexel_height, max_distance) {
                    if hit_distance <= distance + 0.15 { return Some((cell, last_air.unwrap_or(cell))); }
                }
            } else { last_air = Some(cell); }
            distance += 0.045;
        }
        None
    }
    pub fn edit_block(&mut self, cell: CellPos, block: BlockId) {
        if cell.q < Q_START || cell.q >= Q_END || cell.r < R_START || cell.r >= R_END || cell.y < 0 || cell.y > 64 { return; }
        if self.block_at(cell).is_indestructible() { return; }
        self.edits.insert(cell, block);
        self.rebuild_mesh();
    }
    fn rebuild_mesh(&mut self) {
        let vertices = build_world_mesh(self.seed, &self.edits);
        self.vertex_count = vertices.len() as u32;
        self.vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("32-chunk hexel terrain"), contents: bytemuck::cast_slice(&vertices), usage: wgpu::BufferUsages::VERTEX,
        });
    }
    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 { return; }
        self.size = size; self.config.width = size.width; self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }
    pub fn render(&mut self, camera: &Camera) -> Result<(), wgpu::SurfaceError> {
        let uniform = CameraUniform { view_proj: camera.view_projection(self.size).to_cols_array_2d() };
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Frame depth buffer"), size: wgpu::Extent3d { width: self.config.width, height: self.config.height, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Hexel frame encoder") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Hexel world pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.52, g: 0.72, b: 0.88, a: 1.0 }), store: wgpu::StoreOp::Store,
                } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &depth_view, depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store,
                }), stencil_ops: None }), occlusion_query_set: None, timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline); pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..)); pass.draw(0..self.vertex_count, 0..1);
        }
        self.queue.submit(Some(encoder.finish())); frame.present(); Ok(())
    }
}

fn build_world_mesh(seed: u32, edits: &HashMap<CellPos, BlockId>) -> Vec<Vertex> {
    let scale = WorldScale::default();
    let mut vertices = Vec::with_capacity(2_000_000);
    let block_at = |cell: CellPos| edits.get(&cell).copied().unwrap_or_else(|| generated_block(seed, cell));
    for q in Q_START..Q_END {
        for r in R_START..R_END {
            let terrain_top = terrain_height(seed, q, r);
            for y in 0..=(terrain_top + MAX_EDIT_HEIGHT).max(terrain_top) {
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
                let base = block_color(block);
                if !block_at(CellPos::new(q, r, y + 1)).is_solid() {
                    let top = [(base[0] * 1.12).min(1.0), (base[1] * 1.12).min(1.0), (base[2] * 1.12).min(1.0)];
                    for i in 0..6 {
                        push_triangle(&mut vertices, Vec3::new(center.x, top_y, center.z), Vec3::new(ring[i].x, top_y, ring[i].z), Vec3::new(ring[(i + 1) % 6].x, top_y, ring[(i + 1) % 6].z), top);
                    }
                }
                let neighbors = [(0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1), (1, 0)];
                for (i, (dq, dr)) in neighbors.into_iter().enumerate() {
                    let nq = q + dq; let nr = r + dr;
                    let outside = !(Q_START..Q_END).contains(&nq) || !(R_START..R_END).contains(&nr);
                    if !outside && block_at(CellPos::new(nq, nr, y)).is_solid() { continue; }
                    let a = ring[i]; let b = ring[(i + 1) % 6];
                    let shade = 0.48 + ((i * 7 % 6) as f32) * 0.055;
                    let side = [base[0] * shade, base[1] * shade, base[2] * shade];
                    push_triangle(&mut vertices, Vec3::new(a.x, bottom_y, a.z), Vec3::new(a.x, top_y, a.z), Vec3::new(b.x, top_y, b.z), side);
                    push_triangle(&mut vertices, Vec3::new(a.x, bottom_y, a.z), Vec3::new(b.x, top_y, b.z), Vec3::new(b.x, bottom_y, b.z), side);
                }
            }
        }
    }
    vertices
}
fn block_color(block: BlockId) -> [f32; 3] {
    match block { BlockId::Grass => [0.25, 0.66, 0.25], BlockId::Dirt => [0.47, 0.29, 0.16], BlockId::Stone => [0.48, 0.51, 0.55], BlockId::Foundation => [0.22, 0.23, 0.27], BlockId::Air => [0.0, 0.0, 0.0] }
}
fn push_triangle(vertices: &mut Vec<Vertex>, a: Vec3, b: Vec3, c: Vec3, color: [f32; 3]) {
    for point in [a, b, c] { vertices.push(Vertex { position: point.to_array(), color }); }
}
