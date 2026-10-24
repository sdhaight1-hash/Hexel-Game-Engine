use ash::vk;
use std::ffi::CString;

/// Owns the Vulkan objects shared by the renderer.
///
/// The physical device and queue family are stored alongside the logical device
/// so later code and tests always refer to the exact device that was created.
pub struct VulkanContext {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: ash::Device,
    pub graphics_queue: vk::Queue,
    pub graphics_queue_family_index: u32,
}

impl VulkanContext {
    /// Load Vulkan and create an instance and logical graphics device.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let entry = unsafe { ash::Entry::load()? };

        let app_name = CString::new("Hexel Engine")?;
        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(vk::make_api_version(0, 1, 0, 0))
            .engine_name(&app_name)
            .engine_version(vk::make_api_version(0, 1, 0, 0))
            .api_version(vk::API_VERSION_1_0);

        let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);
        let instance = unsafe { entry.create_instance(&create_info, None)? };

        let physical_devices = match unsafe { instance.enumerate_physical_devices() } {
            Ok(devices) => devices,
            Err(error) => {
                unsafe { instance.destroy_instance(None) };
                return Err(Box::new(error));
            }
        };

        let selected = physical_devices.iter().find_map(|&physical_device| {
            let queue_families =
                unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

            queue_families
                .iter()
                .enumerate()
                .find(|(_, family)| family.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                .map(|(index, _)| (physical_device, index as u32))
        });

        let (physical_device, graphics_queue_family_index) = match selected {
            Some(selected) => selected,
            None => {
                unsafe { instance.destroy_instance(None) };
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
                unsafe { instance.destroy_instance(None) };
                return Err(Box::new(error));
            }
        };

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_family_index, 0) };

        println!("Logical Vulkan device created successfully.");

        Ok(Self {
            entry,
            instance,
            physical_device,
            device,
            graphics_queue,
            graphics_queue_family_index,
        })
    }

    /// Return the physical device and queue family used to create this context.
    ///
    /// This intentionally does not enumerate devices again: doing so could return
    /// a different GPU from the one that owns `self.device` and `self.graphics_queue`.
    pub fn select_physical_device(
        &self,
    ) -> Result<(vk::PhysicalDevice, u32), Box<dyn std::error::Error>> {
        Ok((self.physical_device, self.graphics_queue_family_index))
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

/// Initialize Vulkan, report detected GPUs, and clean up on return.
pub fn test_vulkan() -> Result<(), Box<dyn std::error::Error>> {
    let context = VulkanContext::new()?;
    let devices = unsafe { context.instance.enumerate_physical_devices()? };

    println!("Vulkan initialized successfully!");
    println!("Physical GPUs found: {}", devices.len());

    for (index, device) in devices.iter().enumerate() {
        let properties = unsafe { context.instance.get_physical_device_properties(*device) };
        let name = unsafe { std::ffi::CStr::from_ptr(properties.device_name.as_ptr()) };
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

        assert_eq!(device, context.physical_device);
        assert_eq!(queue_family_index, context.graphics_queue_family_index);

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
