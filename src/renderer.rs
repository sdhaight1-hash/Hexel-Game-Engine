use std::{collections::{HashMap, HashSet}, sync::{Arc, mpsc::{self, Receiver, Sender}}, thread};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use hexel_engine::{BlockId, CellPos, WorldScale, raycast_prism, terrain_height};
use noise::{NoiseFn, Perlin};
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
const RENDER_DISTANCE: f32 = 260.0;

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
    fn view_projection(&self, size: PhysicalSize<u32>) -> Mat4 {
        let aspect = size.width.max(1) as f32 / size.height.max(1) as f32;
        Mat4::perspective_rh(70.0_f32.to_radians(), aspect, 0.1, 900.0) * Mat4::look_to_rh(self.position, self.forward(), Vec3::Y)
    }
}

struct MeshJob { revision: u64, seed: u32, edits: HashMap<CellPos, BlockId>, chunks: Vec<(usize, i32, i32)> }
struct MeshResult { revision: u64, chunks: Vec<(usize, Vec<Vertex>)> }
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
    chunks: Vec<ChunkMesh>, camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup, size: PhysicalSize<u32>,
    _depth_texture: wgpu::Texture, depth_view: wgpu::TextureView,
    seed: u32, edits: HashMap<CellPos, BlockId>, terrain_heights: HashMap<(i32, i32), i32>,
    mesh_job_tx: Sender<MeshJob>, mesh_result_rx: Receiver<MeshResult>, dirty_chunks: HashSet<usize>, edit_revision: u64,
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("Hexel pipeline layout"), bind_group_layouts: &[&camera_layout], push_constant_ranges: &[] });
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
            Vertex { position: [-0.018, 0.0, 0.0], color: [0.02, 0.02, 0.02] },
            Vertex { position: [ 0.018, 0.0, 0.0], color: [0.02, 0.02, 0.02] },
            Vertex { position: [0.0, -0.025, 0.0], color: [0.02, 0.02, 0.02] },
            Vertex { position: [0.0,  0.025, 0.0], color: [0.02, 0.02, 0.02] },
            Vertex { position: [-0.010, 0.0, 0.0], color: [1.0, 1.0, 1.0] },
            Vertex { position: [ 0.010, 0.0, 0.0], color: [1.0, 1.0, 1.0] },
            Vertex { position: [0.0, -0.015, 0.0], color: [1.0, 1.0, 1.0] },
            Vertex { position: [0.0,  0.015, 0.0], color: [1.0, 1.0, 1.0] },
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
                let chunks = job.chunks.iter().map(|(index, cq, cr)| {
                    let qs = Q_START + cq * CHUNK_SIZE;
                    let rs = R_START + cr * CHUNK_SIZE;
                    (*index, build_chunk_mesh(job.seed, &job.edits, qs, qs + CHUNK_SIZE, rs, rs + CHUNK_SIZE))
                }).collect();
                if mesh_result_tx.send(MeshResult { revision: job.revision, chunks }).is_err() { break; }
            }
        }).map_err(|e| format!("Could not start mesh worker: {e}"))?;
        let mut renderer = Self {
            surface, device, queue, config, pipeline, outline_pipeline, overlay_pipeline, crosshair_buffer, target_buffer, chunks: Vec::new(),
            camera_buffer, camera_bind_group, size, _depth_texture: depth_texture, depth_view,
            seed, edits: HashMap::new(), terrain_heights: (Q_START..Q_END).flat_map(|q| (R_START..R_END).map(move |r| ((q, r), terrain_height(seed, q, r)))).collect(), mesh_job_tx, mesh_result_rx, dirty_chunks: HashSet::new(), edit_revision: 0,
        };
        for cr in 0..CHUNKS_R {
            for cq in 0..CHUNKS_Q {
                let chunk = renderer.create_chunk(cq, cr);
                renderer.chunks.push(chunk);
            }
        }
        Ok(renderer)
    }

    pub fn ground_height(&self, q: i32, r: i32) -> f32 {
        let scale = WorldScale::default();
        let terrain_top = *self.terrain_heights.get(&(q, r)).unwrap_or(&0);
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
        self.edits.get(&cell).copied().unwrap_or_else(|| generated_block_from_height(cell, *self.terrain_heights.get(&(cell.q, cell.r)).unwrap_or(&0)))
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
                if let Some((hit_distance, _)) = raycast_prism(origin, dir, center, scale.hex_radius, scale.hexel_height, max_distance)
                    && hit_distance <= distance + 0.15
                {
                    return Some((cell, last_air.unwrap_or(cell)));
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
        let cq = (cell.q - Q_START).div_euclid(CHUNK_SIZE);
        let cr = (cell.r - R_START).div_euclid(CHUNK_SIZE);
        let local_q = (cell.q - Q_START).rem_euclid(CHUNK_SIZE);
        let local_r = (cell.r - R_START).rem_euclid(CHUNK_SIZE);
        let mut q_indices = vec![cq];
        let mut r_indices = vec![cr];
        if local_q == 0 { q_indices.push(cq - 1); }
        if local_q == CHUNK_SIZE - 1 { q_indices.push(cq + 1); }
        if local_r == 0 { r_indices.push(cr - 1); }
        if local_r == CHUNK_SIZE - 1 { r_indices.push(cr + 1); }
        let mut affected = Vec::with_capacity(9);
        for rr in &r_indices {
            for qq in &q_indices {
                if (0..CHUNKS_Q).contains(qq) && (0..CHUNKS_R).contains(rr) {
                    affected.push((*rr * CHUNKS_Q + *qq) as usize);
                }
            }
        }
        affected.sort_unstable();
        affected.dedup();
        self.dirty_chunks.extend(affected);
        self.edit_revision = self.edit_revision.wrapping_add(1);
        let chunks = self.dirty_chunks.iter().map(|index| (*index, *index as i32 % CHUNKS_Q, *index as i32 / CHUNKS_Q)).collect();
        let _ = self.mesh_job_tx.send(MeshJob { revision: self.edit_revision, seed: self.seed, edits: self.edits.clone(), chunks });
    }
    fn poll_mesh_results(&mut self) {
        while let Ok(result) = self.mesh_result_rx.try_recv() {
            if result.revision != self.edit_revision { continue; }
            for (index, vertices) in result.chunks {
                let vertex_count = vertices.len() as u32;
                let buffer = if vertices.is_empty() {
                    self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("Empty hexel chunk"), size: 4, usage: wgpu::BufferUsages::VERTEX, mapped_at_creation: false })
                } else {
                    self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("Updated hexel chunk mesh"), contents: bytemuck::cast_slice(&vertices), usage: wgpu::BufferUsages::VERTEX })
                };
                self.chunks[index].buffer = buffer;
                self.chunks[index].vertex_count = vertex_count;
            }
            self.dirty_chunks.clear();
        }
    }
    fn create_chunk(&self, cq: i32, cr: i32) -> ChunkMesh {
        let q_start = Q_START + cq * CHUNK_SIZE;
        let r_start = R_START + cr * CHUNK_SIZE;
        let q_end = q_start + CHUNK_SIZE;
        let r_end = r_start + CHUNK_SIZE;
        let vertices = build_chunk_mesh(self.seed, &self.edits, q_start, q_end, r_start, r_end);
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
        let mut max = Vec3::new(f32::NEG_INFINITY, 70.0 * scale.hexel_height, f32::NEG_INFINITY);
        for q in [q_start, q_end - 1] {
            for r in [r_start, r_end - 1] {
                let center = scale.prism_center(CellPos::new(q, r, 0));
                min.x = min.x.min(center.x); min.z = min.z.min(center.z);
                max.x = max.x.max(center.x); max.z = max.z.max(center.z);
            }
        }
        min.x -= scale.hex_radius * 2.0; min.z -= scale.hex_radius * 2.0;
        max.x += scale.hex_radius * 2.0; max.z += scale.hex_radius * 2.0;
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
    pub fn render(&mut self, camera: &Camera) -> Result<(), wgpu::SurfaceError> {
        self.poll_mesh_results();
        let uniform = CameraUniform { view_proj: camera.view_projection(self.size).to_cols_array_2d() };
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
        let target_vertices = self.raycast(camera.position, camera.forward(), 8.0).map(|(cell, _)| build_target_outline(cell)).unwrap_or_default();
        if !target_vertices.is_empty() { self.queue.write_buffer(&self.target_buffer, 0, bytemuck::cast_slice(&target_vertices)); }
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
            let view_proj = camera.view_projection(self.size);
            let forward = camera.forward();
            for chunk in &self.chunks {
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
        vertices.push(Vertex { position: a.to_array(), color });
        vertices.push(Vertex { position: b.to_array(), color });
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
fn build_chunk_mesh(seed: u32, edits: &HashMap<CellPos, BlockId>, q_start: i32, q_end: i32, r_start: i32, r_end: i32) -> Vec<Vertex> {
    let scale = WorldScale::default();
    let mut vertices = Vec::with_capacity(70_000);
    let noise = Perlin::new(seed);
    let mut heights = HashMap::with_capacity(((q_end - q_start + 2) * (r_end - r_start + 2)) as usize);
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
            let max_y = (terrain_top + MAX_EDIT_HEIGHT).max(edited_top).max(terrain_top).min(64);
            for y in 0..=max_y {
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
                    let nq = q + dq;
                    let nr = r + dr;
                    let inside_world = (Q_START..Q_END).contains(&nq) && (R_START..R_END).contains(&nr);
                    if inside_world && block_at(CellPos::new(nq, nr, y)).is_solid() { continue; }
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
