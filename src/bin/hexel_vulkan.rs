use ash::{vk, Entry};
use std::{ffi::CString, fs, io::Cursor};
use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

const HEXEL_HEIGHT: f32 = 0.8;

// Column-major projection: show height on screen and angle the X/Z ground plane
// so the hexagonal prism has visible top and side faces.
const HEXEL_PROJECTION: [f32; 16] = [
    0.8, 0.0, 0.0, 0.0,
    0.0, 0.8, 0.0, 0.0,
    0.5, 0.35, 0.5, 0.0,
    0.0, 0.0, 0.5, 1.0,
];

const HEXEL_VERTICES: [Vertex; 54] = {
    let corners = [
        [0.0, -0.6],
        [0.5196, -0.3],
        [0.5196, 0.3],
        [0.0, 0.6],
        [-0.5196, 0.3],
        [-0.5196, -0.3],
    ];

    let top_y = HEXEL_HEIGHT / 2.0;
    let bottom_y = -HEXEL_HEIGHT / 2.0;

    let mut vertices = [Vertex {
        position: [0.0, 0.0, 0.0],
        color: [1.0, 1.0, 1.0],
    }; 54];

    let mut i = 0;

    while i < 6 {
        let next = (i + 1) % 6;

        // Top face: one triangle per hexagon edge.
        vertices[i * 3] = Vertex {
            position: [0.0, top_y, 0.0],
            color: [0.3, 0.8, 1.0],
        };
        vertices[i * 3 + 1] = Vertex {
            position: [corners[i][0], top_y, corners[i][1]],
            color: [0.3, 0.8, 1.0],
        };
        vertices[i * 3 + 2] = Vertex {
            position: [corners[next][0], top_y, corners[next][1]],
            color: [0.3, 0.8, 1.0],
        };

        // One side face: two triangles per edge.
        let base = 18 + i * 6;

        let top_a = [corners[i][0], top_y, corners[i][1]];
        let top_b = [corners[next][0], top_y, corners[next][1]];
        let bottom_a = [corners[i][0], bottom_y, corners[i][1]];
        let bottom_b = [corners[next][0], bottom_y, corners[next][1]];

        vertices[base] = Vertex {
            position: top_a,
            color: [0.2, 0.6, 0.9],
        };
        vertices[base + 1] = Vertex {
            position: bottom_a,
            color: [0.15, 0.4, 0.7],
        };
        vertices[base + 2] = Vertex {
            position: bottom_b,
            color: [0.15, 0.4, 0.7],
        };

        vertices[base + 3] = Vertex {
            position: top_a,
            color: [0.2, 0.6, 0.9],
        };
        vertices[base + 4] = Vertex {
            position: bottom_b,
            color: [0.15, 0.4, 0.7],
        };
        vertices[base + 5] = Vertex {
            position: top_b,
            color: [0.2, 0.6, 0.9],
        };

        i += 1;
    }

    vertices
};

fn find_memory_type(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    type_filter: u32,
    properties: vk::MemoryPropertyFlags,
) -> Result<u32, String> {
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };

    for index in 0..memory_properties.memory_type_count {
        let type_supported = (type_filter & (1 << index)) != 0;
        let flags = memory_properties.memory_types[index as usize].property_flags;

        if type_supported && flags.contains(properties) {
            return Ok(index);
        }
    }

    Err("Failed to find a suitable Vulkan memory type".to_owned())
}

fn load_shader_code(path: &str) -> Result<Vec<u32>, String> {
    let bytes = fs::read(path).map_err(|error| format!("Failed to read shader {path}: {error}"))?;

    ash::util::read_spv(&mut Cursor::new(bytes))
        .map_err(|error| format!("Failed to parse shader {path}: {error}"))
}

#[derive(Default)]
struct VulkanApp {
    window: Option<Window>,
    vertex_buffer: Option<vk::Buffer>,
    vertex_buffer_memory: Option<vk::DeviceMemory>,
    _entry: Option<Entry>,
    instance: Option<ash::Instance>,
    physical_device: Option<vk::PhysicalDevice>,
    device: Option<ash::Device>,
    graphics_queue: Option<vk::Queue>,
    surface_loader: Option<ash::khr::surface::Instance>,
    surface: Option<vk::SurfaceKHR>,
    swapchain_loader: Option<ash::khr::swapchain::Device>,
    swapchain: Option<vk::SwapchainKHR>,
    surface_format: Option<vk::SurfaceFormatKHR>,
    present_mode: Option<vk::PresentModeKHR>,
    recreate_swapchain: bool,
    swapchain_images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    render_pass: Option<vk::RenderPass>,
    pipeline_layout: Option<vk::PipelineLayout>,
    graphics_pipeline: Option<vk::Pipeline>,
    framebuffers: Vec<vk::Framebuffer>,
    command_pool: Option<vk::CommandPool>,
    command_buffers: Vec<vk::CommandBuffer>,
    images_in_flight: Vec<vk::Fence>,
    current_frame: usize,
    image_available_semaphores: Vec<vk::Semaphore>,
    render_finished_semaphores: Vec<vk::Semaphore>,
    in_flight_fences: Vec<vk::Fence>,
}

impl ApplicationHandler for VulkanApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Hexel Engine - Vulkan")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 1000.0));

        let window = match event_loop.create_window(attributes) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("Failed to create window: {error}");
                event_loop.exit();
                return;
            }
        };

        let entry = match unsafe { Entry::load() } {
            Ok(entry) => entry,
            Err(error) => {
                eprintln!("Failed to load Vulkan: {error}");
                event_loop.exit();
                return;
            }
        };

        let app_name = CString::new("Hexel Engine").unwrap();

        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(vk::make_api_version(0, 1, 0, 0))
            .engine_name(&app_name)
            .engine_version(vk::make_api_version(0, 1, 0, 0))
            .api_version(vk::API_VERSION_1_0);

        let extensions = match ash_window_extensions(&entry, &window) {
            Ok(extensions) => extensions,
            Err(error) => {
                eprintln!("Failed to determine Vulkan window extensions: {error}");
                event_loop.exit();
                return;
            }
        };

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&extensions);

        let instance = match unsafe { entry.create_instance(&create_info, None) } {
            Ok(instance) => instance,
            Err(error) => {
                eprintln!("Failed to create Vulkan instance: {error}");
                event_loop.exit();
                return;
            }
        };

        println!("Vulkan window created.");
        println!("Vulkan instance created for the window.");

        let physical_devices = match unsafe { instance.enumerate_physical_devices() } {
            Ok(devices) => devices,
            Err(error) => {
                eprintln!("Failed to enumerate GPUs: {error:?}");
                return;
            }
        };

        let mut selected_device = None;

        'devices: for physical_device in &physical_devices {
            let properties = unsafe { instance.get_physical_device_properties(*physical_device) };

            let name = unsafe { std::ffi::CStr::from_ptr(properties.device_name.as_ptr()) };

            println!("Detected Vulkan GPU: {}", name.to_string_lossy());

            let queue_families =
                unsafe { instance.get_physical_device_queue_family_properties(*physical_device) };

            for (index, queue) in queue_families.iter().enumerate() {
                if queue.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                    println!("Graphics queue family: {}", index);
                    selected_device = Some((*physical_device, index as u32));
                    break 'devices;
                }
            }
        }

        let (physical_device, graphics_queue_family) = match selected_device {
            Some(selected) => selected,
            None => {
                eprintln!("No GPU with a graphics queue was found.");
                event_loop.exit();
                return;
            }
        };

        let queue_priority = [1.0_f32];

        let queue_info = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(graphics_queue_family)
            .queue_priorities(&queue_priority)];
        let device_extensions = [ash::khr::swapchain::NAME.as_ptr()];

        let device_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_info)
            .enabled_extension_names(&device_extensions);

        let device = match unsafe { instance.create_device(physical_device, &device_info, None) } {
            Ok(device) => device,
            Err(error) => {
                eprintln!("Failed to create Vulkan logical device: {error:?}");
                event_loop.exit();
                return;
            }
        };

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_family, 0) };

        println!("Vulkan logical device created successfully.");
        println!("Graphics queue retrieved successfully.");
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);

        let surface = {
            use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

            let display_handle = window.display_handle().unwrap().as_raw();
            let window_handle = window.window_handle().unwrap().as_raw();

            match unsafe {
                ash_window::create_surface(&entry, &instance, display_handle, window_handle, None)
            } {
                Ok(surface) => surface,
                Err(error) => {
                    eprintln!("Failed to create Vulkan surface: {error:?}");
                    event_loop.exit();
                    return;
                }
            }
        };

        println!("Vulkan window surface created successfully.");
        let present_supported = unsafe {
            surface_loader.get_physical_device_surface_support(
                physical_device,
                graphics_queue_family,
                surface,
            )
        };

        match present_supported {
            Ok(true) => println!("Surface presentation is supported."),
            Ok(false) => {
                eprintln!("Graphics queue cannot present to this surface.");
                event_loop.exit();
                return;
            }
            Err(error) => {
                eprintln!("Failed to check presentation support: {error:?}");
                event_loop.exit();
                return;
            }
        }

        let capabilities = match unsafe {
            surface_loader.get_physical_device_surface_capabilities(physical_device, surface)
        } {
            Ok(capabilities) => capabilities,
            Err(error) => {
                eprintln!("Failed to get surface capabilities: {error:?}");
                event_loop.exit();
                return;
            }
        };

        let formats = match unsafe {
            surface_loader.get_physical_device_surface_formats(physical_device, surface)
        } {
            Ok(formats) => formats,
            Err(error) => {
                eprintln!("Failed to get surface formats: {error:?}");
                event_loop.exit();
                return;
            }
        };

        let present_modes = match unsafe {
            surface_loader.get_physical_device_surface_present_modes(physical_device, surface)
        } {
            Ok(modes) => modes,
            Err(error) => {
                eprintln!("Failed to get presentation modes: {error:?}");
                event_loop.exit();
                return;
            }
        };

        println!("Surface capabilities retrieved successfully.");
        println!("Supported surface formats: {}", formats.len());
        println!("Supported presentation modes: {}", present_modes.len());

        if formats.is_empty() || present_modes.is_empty() {
            eprintln!("The surface has no usable formats or presentation modes.");
            event_loop.exit();
            return;
        }

        let surface_format = formats
            .iter()
            .copied()
            .find(|format| {
                format.format == vk::Format::B8G8R8A8_SRGB
                    && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .unwrap_or(formats[0]);

        let present_mode = present_modes
            .iter()
            .copied()
            .find(|mode| *mode == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO);

        let window_size = window.inner_size();

        println!("Selected surface format: {:?}", surface_format.format);
        println!("Selected presentation mode: {:?}", present_mode);
        println!(
            "Surface minimum image count: {}",
            capabilities.min_image_count
        );
        println!(
            "Surface maximum image count: {}",
            capabilities.max_image_count
        );

        self.surface_loader = Some(surface_loader);
        self.surface = Some(surface);
        self.physical_device = Some(physical_device);
        self.graphics_queue = Some(graphics_queue);
        self.window = Some(window);
        self._entry = Some(entry);

        let extent = if capabilities.current_extent.width != u32::MAX {
            capabilities.current_extent
        } else {
            vk::Extent2D {
                width: window_size.width.clamp(
                    capabilities.min_image_extent.width,
                    capabilities.max_image_extent.width,
                ),
                height: window_size.height.clamp(
                    capabilities.min_image_extent.height,
                    capabilities.max_image_extent.height,
                ),
            }
        };

        let mut image_count = capabilities.min_image_count + 1;
        if capabilities.max_image_count > 0 {
            image_count = image_count.min(capabilities.max_image_count);
        }

        let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);

        let create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true);

        let swapchain = match unsafe { swapchain_loader.create_swapchain(&create_info, None) } {
            Ok(swapchain) => swapchain,
            Err(error) => {
                eprintln!("Failed to create Vulkan swapchain: {error:?}");
                event_loop.exit();
                return;
            }
        };

        println!("Vulkan swapchain created successfully.");
        println!("Swapchain extent: {} x {}", extent.width, extent.height);
        println!("Swapchain image count: {}", image_count);

        self.swapchain = Some(swapchain);
        self.surface_format = Some(surface_format);
        self.present_mode = Some(present_mode);

        let swapchain_images = match unsafe { swapchain_loader.get_swapchain_images(swapchain) } {
            Ok(images) => images,
            Err(error) => {
                eprintln!("Failed to retrieve swapchain images: {error:?}");
                event_loop.exit();
                return;
            }
        };

        println!("Retrieved {} swapchain images.", swapchain_images.len());
        self.swapchain_images = swapchain_images.clone();
        let mut image_views = Vec::with_capacity(swapchain_images.len());

        for image in &swapchain_images {
            let view_info = vk::ImageViewCreateInfo::default()
                .image(*image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(surface_format.format)
                .components(vk::ComponentMapping::default())
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );

            match unsafe { device.create_image_view(&view_info, None) } {
                Ok(view) => image_views.push(view),
                Err(error) => {
                    eprintln!("Failed to create swapchain image view: {error:?}");
                    event_loop.exit();
                    return;
                }
            }
        }

        println!("Created {} swapchain image views.", image_views.len());
        self.image_views = image_views;

        let color_attachment = [vk::AttachmentDescription::default()
            .format(surface_format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
        let color_attachment_ref = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let subpass = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attachment_ref)];
        let dependency = [vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)];
        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(&color_attachment)
            .subpasses(&subpass)
            .dependencies(&dependency);
        let render_pass = match unsafe { device.create_render_pass(&render_pass_info, None) } {
            Ok(render_pass) => render_pass,
            Err(error) => {
                eprintln!("Failed to create Vulkan render pass: {error:?}");
                event_loop.exit();
                return;
            }
        };
        self.render_pass = Some(render_pass);

        let (pipeline_layout, graphics_pipeline) =
            match Self::create_graphics_pipeline(&device, render_pass) {
                Ok(pipeline) => pipeline,
                Err(error) => {
                    eprintln!("Failed to create Vulkan graphics pipeline: {error}");
                    event_loop.exit();
                    return;
                }
            };

        self.pipeline_layout = Some(pipeline_layout);
        self.graphics_pipeline = Some(graphics_pipeline);

        println!("Vulkan graphics pipeline created successfully.");

        for &image_view in &self.image_views {
            let attachments = [image_view];
            let framebuffer_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            match unsafe { device.create_framebuffer(&framebuffer_info, None) } {
                Ok(framebuffer) => self.framebuffers.push(framebuffer),
                Err(error) => {
                    eprintln!("Failed to create Vulkan framebuffer: {error:?}");
                    event_loop.exit();
                    return;
                }
            }
        }
        println!("Created {} Vulkan framebuffers.", self.framebuffers.len());

        let (vertex_buffer, vertex_buffer_memory) =
            match Self::create_vertex_buffer(&instance, physical_device, &device) {
                Ok(buffer) => buffer,
                Err(error) => {
                    eprintln!("Failed to create Vulkan vertex buffer: {error}");
                    event_loop.exit();
                    return;
                }
            };

        self.vertex_buffer = Some(vertex_buffer);
        self.vertex_buffer_memory = Some(vertex_buffer_memory);

        println!("Vulkan vertex buffer created successfully.");

        let command_pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(graphics_queue_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

        let command_pool = match unsafe { device.create_command_pool(&command_pool_info, None) } {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("Failed to create Vulkan command pool: {error:?}");
                event_loop.exit();
                return;
            }
        };

        println!("Vulkan command pool created successfully.");
        self.command_pool = Some(command_pool);
        let command_buffer_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(swapchain_images.len() as u32);

        let command_buffers = match unsafe { device.allocate_command_buffers(&command_buffer_info) }
        {
            Ok(buffers) => buffers,
            Err(error) => {
                eprintln!("Failed to allocate Vulkan command buffers: {error:?}");
                event_loop.exit();
                return;
            }
        };

        println!(
            "Allocated {} Vulkan command buffers.",
            command_buffers.len()
        );
        self.command_buffers = command_buffers;
        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.055, 0.075, 0.12, 1.0],
            },
        }];
        for (index, &command_buffer) in self.command_buffers.iter().enumerate() {
            let begin_info = vk::CommandBufferBeginInfo::default();
            if let Err(error) = unsafe { device.begin_command_buffer(command_buffer, &begin_info) }
            {
                eprintln!("Failed to begin Vulkan command buffer: {error:?}");
                event_loop.exit();
                return;
            }

            let render_pass_begin = vk::RenderPassBeginInfo::default()
                .render_pass(render_pass)
                .framebuffer(self.framebuffers[index])
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent,
                })
                .clear_values(&clear_values);
            let viewport = [vk::Viewport::default()
                .x(0.0)
                .y(0.0)
                .width(extent.width as f32)
                .height(extent.height as f32)
                .min_depth(0.0)
                .max_depth(1.0)];

            let scissor = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            }];

            let graphics_pipeline = self
                .graphics_pipeline
                .expect("Graphics pipeline should exist before recording commands");

            unsafe {
                device.cmd_begin_render_pass(
                    command_buffer,
                    &render_pass_begin,
                    vk::SubpassContents::INLINE,
                );

                device.cmd_set_viewport(command_buffer, 0, &viewport);
                device.cmd_set_scissor(command_buffer, 0, &scissor);

                device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    graphics_pipeline,
                );

                let pipeline_layout = self
                    .pipeline_layout
                    .expect("Pipeline layout should exist before recording commands");

                let transform = HEXEL_PROJECTION;

                device.cmd_push_constants(
                    command_buffer,
                    pipeline_layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    std::slice::from_raw_parts(
                        transform.as_ptr().cast::<u8>(),
                        std::mem::size_of_val(&transform),
                    ),
                );

                let vertex_buffer = self
                    .vertex_buffer
                    .expect("Vertex buffer should exist before recording commands");

                let vertex_buffers = [vertex_buffer];
                let offsets = [0_u64];

                device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &offsets);
                device.cmd_draw(command_buffer, HEXEL_VERTICES.len() as u32, 1, 0, 0);

                device.cmd_end_render_pass(command_buffer);
            }

            if let Err(error) = unsafe { device.end_command_buffer(command_buffer) } {
                eprintln!("Failed to end Vulkan command buffer: {error:?}");
                event_loop.exit();
                return;
            }
        }

        println!(
            "Recorded {} clear-frame command buffers.",
            self.command_buffers.len()
        );
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

        let mut image_available_semaphores = Vec::new();
        let mut render_finished_semaphores = Vec::new();
        let mut in_flight_fences = Vec::new();

        for _ in 0..swapchain_images.len() {
            let image_available = match unsafe { device.create_semaphore(&semaphore_info, None) } {
                Ok(semaphore) => semaphore,
                Err(error) => {
                    eprintln!("Failed to create image-available semaphore: {error:?}");
                    event_loop.exit();
                    return;
                }
            };

            let render_finished = match unsafe { device.create_semaphore(&semaphore_info, None) } {
                Ok(semaphore) => semaphore,
                Err(error) => {
                    eprintln!("Failed to create render-finished semaphore: {error:?}");
                    event_loop.exit();
                    return;
                }
            };

            let fence = match unsafe { device.create_fence(&fence_info, None) } {
                Ok(fence) => fence,
                Err(error) => {
                    eprintln!("Failed to create in-flight fence: {error:?}");
                    event_loop.exit();
                    return;
                }
            };

            image_available_semaphores.push(image_available);
            render_finished_semaphores.push(render_finished);
            in_flight_fences.push(fence);
        }

        println!(
            "Created {} sets of Vulkan synchronization objects.",
            in_flight_fences.len()
        );

        self.image_available_semaphores = image_available_semaphores;
        self.render_finished_semaphores = render_finished_semaphores;
        self.in_flight_fences = in_flight_fences;
        self.images_in_flight = vec![vk::Fence::null(); swapchain_images.len()];
        self.current_frame = 0;
        self.swapchain_loader = Some(swapchain_loader);
        self.instance = Some(instance);
        self.device = Some(device);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: winit::event::WindowEvent,
    ) {
        match event {
            winit::event::WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            winit::event::WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    self.recreate_swapchain = true;
                }
            }
            winit::event::WindowEvent::RedrawRequested => {
                self.draw_frame(event_loop);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl VulkanApp {
    fn create_vertex_buffer(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        device: &ash::Device,
    ) -> Result<(vk::Buffer, vk::DeviceMemory), String> {
        let buffer_size = std::mem::size_of_val(&HEXEL_VERTICES) as vk::DeviceSize;

        let buffer_info = vk::BufferCreateInfo::default()
            .size(buffer_size)
            .usage(vk::BufferUsageFlags::VERTEX_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { device.create_buffer(&buffer_info, None) }
            .map_err(|error| format!("Failed to create vertex buffer: {error:?}"))?;

        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

        let memory_type = match find_memory_type(
            instance,
            physical_device,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        ) {
            Ok(index) => index,
            Err(error) => {
                unsafe {
                    device.destroy_buffer(buffer, None);
                }
                return Err(error);
            }
        };

        let allocation_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type);

        let memory = match unsafe { device.allocate_memory(&allocation_info, None) } {
            Ok(memory) => memory,
            Err(error) => {
                unsafe {
                    device.destroy_buffer(buffer, None);
                }
                return Err(format!(
                    "Failed to allocate vertex buffer memory: {error:?}"
                ));
            }
        };

        if let Err(error) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_buffer(buffer, None);
            }
            return Err(format!("Failed to bind vertex buffer memory: {error:?}"));
        }

        let mapped =
            match unsafe { device.map_memory(memory, 0, buffer_size, vk::MemoryMapFlags::empty()) }
            {
                Ok(pointer) => pointer,
                Err(error) => {
                    unsafe {
                        device.free_memory(memory, None);
                        device.destroy_buffer(buffer, None);
                    }
                    return Err(format!("Failed to map vertex buffer memory: {error:?}"));
                }
            };

        unsafe {
            std::ptr::copy_nonoverlapping(
                HEXEL_VERTICES.as_ptr(),
                mapped.cast::<Vertex>(),
                HEXEL_VERTICES.len(),
            );
            device.unmap_memory(memory);
        }

        Ok((buffer, memory))
    }
}

impl VulkanApp {
    fn draw_frame(&mut self, event_loop: &ActiveEventLoop) {
        if self.recreate_swapchain {
            if !self.recreate_swapchain_resources(event_loop) {
                return;
            }
            self.recreate_swapchain = false;
        }
        let (Some(device), Some(swapchain_loader), Some(swapchain), Some(queue)) = (
            self.device.as_ref(),
            self.swapchain_loader.as_ref(),
            self.swapchain,
            self.graphics_queue,
        ) else {
            return;
        };

        let frame = self.current_frame % self.in_flight_fences.len();
        let fence = self.in_flight_fences[frame];
        unsafe {
            if let Err(error) = device.wait_for_fences(&[fence], true, u64::MAX) {
                eprintln!("Failed waiting for frame fence: {error:?}");
                event_loop.exit();
                return;
            }
        }

        let (image_index, _suboptimal) = match unsafe {
            swapchain_loader.acquire_next_image(
                swapchain,
                u64::MAX,
                self.image_available_semaphores[frame],
                vk::Fence::null(),
            )
        } {
            Ok(result) => result,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.recreate_swapchain = true;
                return;
            }
            Err(error) => {
                eprintln!("Failed to acquire swapchain image: {error:?}");
                event_loop.exit();
                return;
            }
        };
        let image_index = image_index as usize;

        if self.images_in_flight[image_index] != vk::Fence::null() {
            unsafe {
                if let Err(error) =
                    device.wait_for_fences(&[self.images_in_flight[image_index]], true, u64::MAX)
                {
                    eprintln!("Failed waiting for swapchain image fence: {error:?}");
                    event_loop.exit();
                    return;
                }
            }
        }
        self.images_in_flight[image_index] = fence;

        if let Err(error) = unsafe { device.reset_fences(&[fence]) } {
            eprintln!("Failed to reset frame fence: {error:?}");
            event_loop.exit();
            return;
        }

        let wait_semaphores = [self.image_available_semaphores[frame]];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = [self.command_buffers[image_index]];
        let signal_semaphores = [self.render_finished_semaphores[image_index]];
        let submit_info = [vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores)];

        if let Err(error) = unsafe { device.queue_submit(queue, &submit_info, fence) } {
            eprintln!("Failed to submit Vulkan frame: {error:?}");
            event_loop.exit();
            return;
        }

        let swapchains = [swapchain];
        let image_indices = [image_index as u32];
        let present_wait_semaphores = [self.render_finished_semaphores[image_index]];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&present_wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        match unsafe { swapchain_loader.queue_present(queue, &present_info) } {
            Ok(_) => {}
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::SUBOPTIMAL_KHR) => {
                self.recreate_swapchain = true;
                return;
            }
            Err(error) => {
                eprintln!("Failed to present Vulkan frame: {error:?}");
                event_loop.exit();
                return;
            }
        }

        self.current_frame = (frame + 1) % self.in_flight_fences.len();
    }
}

impl VulkanApp {
    fn recreate_swapchain_resources(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let (
            Some(window),
            Some(physical_device),
            Some(device),
            Some(surface_loader),
            Some(surface),
            Some(swapchain_loader),
            Some(old_swapchain),
            Some(render_pass),
            Some(command_pool),
            Some(surface_format),
            Some(present_mode),
        ) = (
            self.window.as_ref(),
            self.physical_device,
            self.device.as_ref(),
            self.surface_loader.as_ref(),
            self.surface,
            self.swapchain_loader.as_ref(),
            self.swapchain,
            self.render_pass,
            self.command_pool,
            self.surface_format,
            self.present_mode,
        )
        else {
            return false;
        };

        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return false;
        }
        unsafe {
            if let Err(error) = device.device_wait_idle() {
                eprintln!("Failed waiting for device before resize: {error:?}");
                event_loop.exit();
                return false;
            }
        }
        let capabilities = match unsafe {
            surface_loader.get_physical_device_surface_capabilities(physical_device, surface)
        } {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Failed to query surface capabilities: {error:?}");
                event_loop.exit();
                return false;
            }
        };
        let extent = if capabilities.current_extent.width != u32::MAX {
            capabilities.current_extent
        } else {
            vk::Extent2D {
                width: size.width.clamp(
                    capabilities.min_image_extent.width,
                    capabilities.max_image_extent.width,
                ),
                height: size.height.clamp(
                    capabilities.min_image_extent.height,
                    capabilities.max_image_extent.height,
                ),
            }
        };
        let image_count = if capabilities.max_image_count > 0 {
            (capabilities.min_image_count + 1).min(capabilities.max_image_count)
        } else {
            capabilities.min_image_count + 1
        };
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(old_swapchain);
        let new_swapchain = match unsafe { swapchain_loader.create_swapchain(&info, None) } {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Failed to recreate swapchain: {error:?}");
                event_loop.exit();
                return false;
            }
        };
        let images = match unsafe { swapchain_loader.get_swapchain_images(new_swapchain) } {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Failed to get recreated swapchain images: {error:?}");
                unsafe {
                    swapchain_loader.destroy_swapchain(new_swapchain, None);
                }
                event_loop.exit();
                return false;
            }
        };
        let mut views = Vec::with_capacity(images.len());
        for &image in &images {
            let info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(surface_format.format)
                .components(vk::ComponentMapping::default())
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );
            match unsafe { device.create_image_view(&info, None) } {
                Ok(view) => views.push(view),
                Err(error) => {
                    eprintln!("Failed to create resized image view: {error:?}");
                    unsafe {
                        for view in views {
                            device.destroy_image_view(view, None);
                        }
                        swapchain_loader.destroy_swapchain(new_swapchain, None);
                    }
                    event_loop.exit();
                    return false;
                }
            }
        }
        let mut framebuffers = Vec::with_capacity(views.len());
        for &view in &views {
            let attachments = [view];
            let info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            match unsafe { device.create_framebuffer(&info, None) } {
                Ok(fb) => framebuffers.push(fb),
                Err(error) => {
                    eprintln!("Failed to create resized framebuffer: {error:?}");
                    unsafe {
                        for fb in framebuffers {
                            device.destroy_framebuffer(fb, None);
                        }
                        for view in views {
                            device.destroy_image_view(view, None);
                        }
                        swapchain_loader.destroy_swapchain(new_swapchain, None);
                    }
                    event_loop.exit();
                    return false;
                }
            }
        }
        let allocation = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(images.len() as u32);
        let buffers = match unsafe { device.allocate_command_buffers(&allocation) } {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Failed to allocate resized command buffers: {error:?}");
                unsafe {
                    for fb in framebuffers {
                        device.destroy_framebuffer(fb, None);
                    }
                    for view in views {
                        device.destroy_image_view(view, None);
                    }
                    swapchain_loader.destroy_swapchain(new_swapchain, None);
                }
                event_loop.exit();
                return false;
            }
        };
        let clear = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.055, 0.075, 0.12, 1.0],
            },
        }];
        for (i, &buffer) in buffers.iter().enumerate() {
            let begin = vk::CommandBufferBeginInfo::default();
            let render = vk::RenderPassBeginInfo::default()
                .render_pass(render_pass)
                .framebuffer(framebuffers[i])
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent,
                })
                .clear_values(&clear);
            let viewport = [vk::Viewport::default()
                .x(0.0)
                .y(0.0)
                .width(extent.width as f32)
                .height(extent.height as f32)
                .min_depth(0.0)
                .max_depth(1.0)];

            let scissor = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            }];

            let Some(graphics_pipeline) = self.graphics_pipeline else {
                return false;
            };

            let result = unsafe {
                device.begin_command_buffer(buffer, &begin).and_then(|_| {
                    device.cmd_begin_render_pass(buffer, &render, vk::SubpassContents::INLINE);

                    device.cmd_set_viewport(buffer, 0, &viewport);
                    device.cmd_set_scissor(buffer, 0, &scissor);

                    device.cmd_bind_pipeline(
                        buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        graphics_pipeline,
                    );

                    // Push the same column-major projection used by the initial frame.
                    let pipeline_layout = self
                        .pipeline_layout
                        .expect("Pipeline layout should exist before recording resized commands");
                    let transform = HEXEL_PROJECTION;
                    device.cmd_push_constants(
                        buffer,
                        pipeline_layout,
                        vk::ShaderStageFlags::VERTEX,
                        0,
                        std::slice::from_raw_parts(
                            transform.as_ptr().cast::<u8>(),
                            std::mem::size_of_val(&transform),
                        ),
                    );

                    let vertex_buffer = self
                        .vertex_buffer
                        .expect("Vertex buffer should exist before recording commands");

                    let vertex_buffers = [vertex_buffer];
                    let offsets = [0_u64];

                    device.cmd_bind_vertex_buffers(buffer, 0, &vertex_buffers, &offsets);
                    device.cmd_draw(buffer, HEXEL_VERTICES.len() as u32, 1, 0, 0);

                    device.cmd_end_render_pass(buffer);
                    device.end_command_buffer(buffer)
                })
            };
            if let Err(error) = result {
                eprintln!("Failed recording resized command buffer: {error:?}");
                unsafe {
                    device.free_command_buffers(command_pool, &buffers);
                    for fb in framebuffers {
                        device.destroy_framebuffer(fb, None);
                    }
                    for view in views {
                        device.destroy_image_view(view, None);
                    }
                    swapchain_loader.destroy_swapchain(new_swapchain, None);
                }
                event_loop.exit();
                return false;
            }
        }

        // Image count can change on resize, so rebuild all per-image synchronization objects.
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let mut available = Vec::with_capacity(images.len());
        let mut finished = Vec::with_capacity(images.len());
        let mut fences = Vec::with_capacity(images.len());
        for _ in 0..images.len() {
            let a = match unsafe { device.create_semaphore(&semaphore_info, None) } {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Failed creating resize semaphore: {e:?}");
                    event_loop.exit();
                    return false;
                }
            };
            let b = match unsafe { device.create_semaphore(&semaphore_info, None) } {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Failed creating resize semaphore: {e:?}");
                    unsafe {
                        device.destroy_semaphore(a, None);
                    }
                    event_loop.exit();
                    return false;
                }
            };
            let f = match unsafe { device.create_fence(&fence_info, None) } {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Failed creating resize fence: {e:?}");
                    unsafe {
                        device.destroy_semaphore(a, None);
                        device.destroy_semaphore(b, None);
                    }
                    event_loop.exit();
                    return false;
                }
            };
            available.push(a);
            finished.push(b);
            fences.push(f);
        }
        unsafe {
            device.free_command_buffers(command_pool, &self.command_buffers);
            for fb in self.framebuffers.drain(..) {
                device.destroy_framebuffer(fb, None);
            }
            for view in self.image_views.drain(..) {
                device.destroy_image_view(view, None);
            }
            for s in self.image_available_semaphores.drain(..) {
                device.destroy_semaphore(s, None);
            }
            for s in self.render_finished_semaphores.drain(..) {
                device.destroy_semaphore(s, None);
            }
            for f in self.in_flight_fences.drain(..) {
                device.destroy_fence(f, None);
            }
            swapchain_loader.destroy_swapchain(old_swapchain, None);
        }
        self.swapchain = Some(new_swapchain);
        self.swapchain_images = images;
        self.image_views = views;
        self.framebuffers = framebuffers;
        self.command_buffers = buffers;
        self.image_available_semaphores = available;
        self.render_finished_semaphores = finished;
        self.in_flight_fences = fences;
        self.images_in_flight = vec![vk::Fence::null(); self.swapchain_images.len()];
        self.current_frame = 0;
        println!(
            "Recreated Vulkan swapchain: {} x {}.",
            extent.width, extent.height
        );
        true
    }
}

impl VulkanApp {
    fn create_graphics_pipeline(
        device: &ash::Device,
        render_pass: vk::RenderPass,
    ) -> Result<(vk::PipelineLayout, vk::Pipeline), String> {
        let vert_code = load_shader_code("shaders/triangle.vert.spv")?;
        let frag_code = load_shader_code("shaders/triangle.frag.spv")?;

        let vert_info = vk::ShaderModuleCreateInfo::default().code(&vert_code);
        let frag_info = vk::ShaderModuleCreateInfo::default().code(&frag_code);

        let vert_module = unsafe { device.create_shader_module(&vert_info, None) }
            .map_err(|error| format!("Failed to create vertex shader module: {error:?}"))?;

        let frag_module = match unsafe { device.create_shader_module(&frag_info, None) } {
            Ok(module) => module,
            Err(error) => {
                unsafe {
                    device.destroy_shader_module(vert_module, None);
                }
                return Err(format!(
                    "Failed to create fragment shader module: {error:?}"
                ));
            }
        };

        let result = Self::build_graphics_pipeline(device, render_pass, vert_module, frag_module);

        unsafe {
            device.destroy_shader_module(vert_module, None);
            device.destroy_shader_module(frag_module, None);
        }

        result
    }
}

impl VulkanApp {
    fn build_graphics_pipeline(
        device: &ash::Device,
        render_pass: vk::RenderPass,
        vert_module: vk::ShaderModule,
        frag_module: vk::ShaderModule,
    ) -> Result<(vk::PipelineLayout, vk::Pipeline), String> {
        let entry_point = CString::new("main").unwrap();

        let shader_stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vert_module)
                .name(&entry_point),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(frag_module)
                .name(&entry_point),
        ];

        let vertex_binding = [vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)];

        let vertex_attribute_descriptions = [
            vk::VertexInputAttributeDescription::default()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(0),
            vk::VertexInputAttributeDescription::default()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(std::mem::size_of::<[f32; 3]>() as u32),
        ];

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&vertex_binding)
            .vertex_attribute_descriptions(&vertex_attribute_descriptions);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];

        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(false)
            .line_width(1.0);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1)
            .sample_shading_enable(false);

        let color_attachment = vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(false)
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
            );

        let color_attachments = [color_attachment];

        let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(&color_attachments);

        let push_constant_ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(64)];

        let layout_info =
            vk::PipelineLayoutCreateInfo::default().push_constant_ranges(&push_constant_ranges);

        let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
            .map_err(|error| format!("Failed to create pipeline layout: {error:?}"))?;

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_state)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0);

        let pipeline = match unsafe {
            device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
        } {
            Ok(pipelines) => pipelines[0],
            Err((_, error)) => {
                unsafe {
                    device.destroy_pipeline_layout(pipeline_layout, None);
                }
                return Err(format!("Failed to create graphics pipeline: {error:?}"));
            }
        };

        Ok((pipeline_layout, pipeline))
    }
}

impl Drop for VulkanApp {
    fn drop(&mut self) {
        let Some(device) = self.device.as_ref() else {
            return;
        };
        unsafe {
            let _ = device.device_wait_idle();
            for &framebuffer in &self.framebuffers {
                device.destroy_framebuffer(framebuffer, None);
            }

            if let Some(graphics_pipeline) = self.graphics_pipeline {
                device.destroy_pipeline(graphics_pipeline, None);
            }

            if let Some(pipeline_layout) = self.pipeline_layout {
                device.destroy_pipeline_layout(pipeline_layout, None);
            }

            if let Some(render_pass) = self.render_pass {
                device.destroy_render_pass(render_pass, None);
            }
            for &semaphore in &self.image_available_semaphores {
                device.destroy_semaphore(semaphore, None);
            }
            for &semaphore in &self.render_finished_semaphores {
                device.destroy_semaphore(semaphore, None);
            }
            for &fence in &self.in_flight_fences {
                device.destroy_fence(fence, None);
            }
            if let Some(command_pool) = self.command_pool {
                device.destroy_command_pool(command_pool, None);
            }
            for &view in &self.image_views {
                device.destroy_image_view(view, None);
            }
            if let (Some(loader), Some(swapchain)) = (&self.swapchain_loader, self.swapchain) {
                loader.destroy_swapchain(swapchain, None);
            }
            if let (Some(loader), Some(surface)) = (&self.surface_loader, self.surface) {
                loader.destroy_surface(surface, None);
            }

            if let Some(vertex_buffer) = self.vertex_buffer {
                device.destroy_buffer(vertex_buffer, None);
            }

            if let Some(vertex_buffer_memory) = self.vertex_buffer_memory {
                device.free_memory(vertex_buffer_memory, None);
            }

            device.destroy_device(None);
            if let Some(instance) = &self.instance {
                instance.destroy_instance(None);
            }
        }
    }
}

fn ash_window_extensions(
    _entry: &Entry,
    window: &Window,
) -> Result<Vec<*const i8>, Box<dyn std::error::Error>> {
    use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

    let display_handle = window.display_handle()?.as_raw();
    let window_handle = window.window_handle()?.as_raw();

    let extensions = ash_window::enumerate_required_extensions(display_handle)?;
    let _ = window_handle;

    Ok(extensions.to_vec())
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = VulkanApp::default();

    event_loop
        .run_app(&mut app)
        .expect("Failed to run Vulkan window");
}
