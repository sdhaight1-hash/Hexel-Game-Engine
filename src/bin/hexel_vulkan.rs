use ash::{vk, Entry};
use std::ffi::CString;
use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

#[derive(Default)]
struct VulkanApp {
    window: Option<Window>,
    _entry: Option<Entry>,
    instance: Option<ash::Instance>,
    physical_device: Option<vk::PhysicalDevice>,
    device: Option<ash::Device>,
    graphics_queue: Option<vk::Queue>,
    surface_loader: Option<ash::khr::surface::Instance>,
    surface: Option<vk::SurfaceKHR>,
    swapchain_loader: Option<ash::khr::swapchain::Device>,
    swapchain: Option<vk::SwapchainKHR>,
    swapchain_images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    render_pass: Option<vk::RenderPass>,
    framebuffers: Vec<vk::Framebuffer>,
    swapchain_extent: vk::Extent2D,
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

        self.swapchain_extent = extent;

        self.surface_loader = Some(surface_loader);
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
            if let Err(error) = unsafe { device.begin_command_buffer(command_buffer, &begin_info) } {
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
            unsafe {
                device.cmd_begin_render_pass(
                    command_buffer,
                    &render_pass_begin,
                    vk::SubpassContents::INLINE,
                );
                device.cmd_end_render_pass(command_buffer);
            }

            if let Err(error) = unsafe { device.end_command_buffer(command_buffer) } {
                eprintln!("Failed to end Vulkan command buffer: {error:?}");
                event_loop.exit();
                return;
            }
        }

        println!("Recorded {} clear-frame command buffers.", self.command_buffers.len());
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
        self.device = Some(device);
        self.instance = Some(instance);
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
    fn draw_frame(&mut self, event_loop: &ActiveEventLoop) {
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
                eprintln!("Swapchain is out of date; resize/recreation is the next lifecycle milestone.");
                event_loop.exit();
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
                if let Err(error) = device.wait_for_fences(
                    &[self.images_in_flight[image_index]],
                    true,
                    u64::MAX,
                ) {
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
                eprintln!("Swapchain needs recreation; resize handling is the next lifecycle milestone.");
                event_loop.exit();
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
