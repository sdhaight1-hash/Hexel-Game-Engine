use ash::vk;
use std::ffi::CString;

/// Holds the Vulkan library and instance.
pub struct VulkanContext {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub device: ash::Device,
    pub graphics_queue: vk::Queue,
    pub graphics_queue_family_index: u32,
}
impl VulkanContext {
    /// Load Vulkan and create a Vulkan instance.
    /// Load Vulkan, create an instance, and create a logical graphics device.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let entry = unsafe { ash::Entry::load()? };

        let app_name = CString::new("Hexel Engine")?;
        let engine_name = CString::new("Hexel Engine")?;

        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(vk::make_api_version(0, 1, 0, 0))
            .engine_name(&engine_name)
            .engine_version(vk::make_api_version(0, 1, 0, 0))
            .api_version(vk::API_VERSION_1_0);

        let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);

        let instance = unsafe { entry.create_instance(&create_info, None)? };

        let devices = unsafe { instance.enumerate_physical_devices()? };

        let mut selected_device = None;

        for physical_device in devices {
            let queue_families =
                unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

            for (index, queue_family) in queue_families.iter().enumerate() {
                if queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                    selected_device = Some((physical_device, index as u32));
                    break;
                }
            }

            if selected_device.is_some() {
                break;
            }
        }

        let (physical_device, graphics_queue_family_index) = match selected_device {
            Some(selected) => selected,
            None => {
                unsafe {
                    instance.destroy_instance(None);
                }
                return Err("No Vulkan graphics-capable queue found".into());
            }
        };

        let queue_priorities = [1.0_f32];

        let queue_info = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(graphics_queue_family_index)
            .queue_priorities(&queue_priorities)];

        let device_info = vk::DeviceCreateInfo::default().queue_create_infos(&queue_info);

        let device = match unsafe { instance.create_device(physical_device, &device_info, None) } {
            Ok(device) => device,
            Err(error) => {
                unsafe {
                    instance.destroy_instance(None);
                }
                return Err(Box::new(error));
            }
        };

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_family_index, 0) };

        println!("Logical Vulkan device created successfully.");

        Ok(Self {
            entry,
            instance,
            device,
            graphics_queue,
            graphics_queue_family_index,
        })
    }
    /// Find a physical GPU with a graphics-capable queue.
    pub fn select_physical_device(
        &self,
    ) -> Result<(vk::PhysicalDevice, u32), Box<dyn std::error::Error>> {
        let devices = unsafe { self.instance.enumerate_physical_devices()? };

        for device in devices {
            let queue_families = unsafe {
                self.instance
                    .get_physical_device_queue_family_properties(device)
            };

            for (index, queue_family) in queue_families.iter().enumerate() {
                if queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                    println!("Selected Vulkan GPU with graphics support.");

                    return Ok((device, index as u32));
                }
            }
        }

        Err("No Vulkan graphics-capable queue found".into())
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

/// Create and immediately clean up a Vulkan instance to test initialization.
pub fn test_vulkan() -> Result<(), Box<dyn std::error::Error>> {
    let context = VulkanContext::new()?;

    let properties = unsafe { context.instance.enumerate_physical_devices()? };

    println!("Vulkan initialized successfully!");
    println!("Physical GPUs found: {}", properties.len());

    for (index, device) in properties.iter().enumerate() {
        let info = unsafe { context.instance.get_physical_device_properties(*device) };

        let name = unsafe { std::ffi::CStr::from_ptr(info.device_name.as_ptr()) };

        println!("GPU {}: {}", index + 1, name.to_string_lossy());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vulkan_instance_initializes_and_detects_gpus() {
        let context = VulkanContext::new().expect("Failed to initialize Vulkan");

        let (device, queue_family_index) = context
            .select_physical_device()
            .expect("Failed to find a graphics-capable Vulkan GPU");

        let properties = unsafe { context.instance.get_physical_device_properties(device) };

        let name = unsafe { std::ffi::CStr::from_ptr(properties.device_name.as_ptr()) };

        println!("Selected Vulkan GPU: {}", name.to_string_lossy());
        println!("Graphics queue family: {}", queue_family_index);

        let queue_families = unsafe {
            context
                .instance
                .get_physical_device_queue_family_properties(device)
        };

        assert!(
            queue_families[queue_family_index as usize]
                .queue_flags
                .contains(vk::QueueFlags::GRAPHICS),
            "Selected queue family does not support graphics"
        );
    }
}
