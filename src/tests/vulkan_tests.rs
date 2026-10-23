use crate::vulkan::VulkanContext;

#[test]
fn vulkan_instance_initializes_and_detects_gpus() {
    let context = VulkanContext::new()
        .expect("Failed to initialize Vulkan");

    let devices = unsafe {
        context.instance
            .enumerate_physical_devices()
            .expect("Failed to enumerate Vulkan physical devices")
    };

    assert!(
        !devices.is_empty(),
        "Vulkan initialized, but no physical GPUs were detected"
    );

    for device in devices {
        let properties = unsafe {
            context.instance
                .get_physical_device_properties(device)
        };

        let name = unsafe {
            std::ffi::CStr::from_ptr(properties.device_name.as_ptr())
        };

        println!("Detected Vulkan GPU: {}", name.to_string_lossy());
    }
}