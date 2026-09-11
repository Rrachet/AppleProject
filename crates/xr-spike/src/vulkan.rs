//! Vulkan objects, created *through* OpenXR.
//!
//! The runtime must create the `VkInstance` and `VkDevice` itself (via
//! `XR_KHR_vulkan_enable2`) so it can inject the extensions and queue
//! configuration it needs for compositing. Creating them directly with ash and
//! handing them over is the classic way to get a session that fails on some
//! runtimes and works on others, so we never do it.

use anyhow::{Context, Result, anyhow, bail};
use ash::vk::{self, Handle};

pub struct VulkanContext {
    /// Held only to keep libvulkan loaded: dropping the entry unloads the
    /// library out from under `instance` and `device`.
    #[allow(dead_code)]
    entry: ash::Entry,
    pub instance: ash::Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: ash::Device,
    pub queue: vk::Queue,
    pub queue_family_index: u32,
    pub command_pool: vk::CommandPool,
}

impl VulkanContext {
    /// # Safety
    /// `xr_instance` and `system` must be live, and the OpenXR graphics
    /// requirements for `system` must already have been queried.
    pub unsafe fn new(xr_instance: &openxr::Instance, system: openxr::SystemId) -> Result<Self> {
        // Loads libvulkan at runtime; no Vulkan headers or link-time loader needed.
        let entry = unsafe { ash::Entry::load() }
            .context("could not load libvulkan — is a Vulkan ICD installed?")?;

        let get_instance_proc_addr = entry.static_fn().get_instance_proc_addr;

        let app_name = c"spatial-shell";
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(0)
            .engine_name(app_name)
            .engine_version(0)
            .api_version(vk::make_api_version(0, 1, 2, 0));

        let instance_info = vk::InstanceCreateInfo::default().application_info(&app_info);

        // The nested Result is deliberate: the outer one is the OpenXR call
        // itself failing, the inner one is Vulkan rejecting the create info.
        let raw_instance = unsafe {
            xr_instance.create_vulkan_instance(
                system,
                std::mem::transmute::<
                    vk::PFN_vkGetInstanceProcAddr,
                    unsafe extern "system" fn(
                        *const std::ffi::c_void,
                        *const std::ffi::c_char,
                    )
                        -> Option<unsafe extern "system" fn()>,
                >(get_instance_proc_addr),
                &instance_info as *const _ as *const _,
            )
        }
        .context("OpenXR refused to create a Vulkan instance")?
        .map_err(vk::Result::from_raw)
        .map_err(|e| anyhow!("Vulkan instance creation failed: {e:?}"))?;

        let instance = unsafe {
            ash::Instance::load(entry.static_fn(), vk::Instance::from_raw(raw_instance as _))
        };

        // The runtime dictates which GPU drives the headset. Picking our own
        // would work on a single-GPU machine and break on a laptop with
        // switchable graphics.
        // SAFETY: `raw_instance` is the VkInstance the runtime just created for
        // this system, which is exactly what this call requires.
        let physical_device = vk::PhysicalDevice::from_raw(unsafe {
            xr_instance.vulkan_graphics_device(system, raw_instance)?
        } as _);

        let queue_family_index = unsafe {
            instance
                .get_physical_device_queue_family_properties(physical_device)
                .into_iter()
                .enumerate()
                .find(|(_, props)| props.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                .map(|(index, _)| index as u32)
                .ok_or_else(|| anyhow!("no graphics queue family on the headset's GPU"))?
        };

        let priorities = [1.0f32];
        let queue_info = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&priorities)];
        let device_info = vk::DeviceCreateInfo::default().queue_create_infos(&queue_info);

        let raw_device = unsafe {
            xr_instance.create_vulkan_device(
                system,
                std::mem::transmute::<
                    vk::PFN_vkGetInstanceProcAddr,
                    unsafe extern "system" fn(
                        *const std::ffi::c_void,
                        *const std::ffi::c_char,
                    )
                        -> Option<unsafe extern "system" fn()>,
                >(get_instance_proc_addr),
                physical_device.as_raw() as _,
                &device_info as *const _ as *const _,
            )
        }
        .context("OpenXR refused to create a Vulkan device")?
        .map_err(vk::Result::from_raw)
        .map_err(|e| anyhow!("Vulkan device creation failed: {e:?}"))?;

        let device =
            unsafe { ash::Device::load(instance.fp_v1_0(), vk::Device::from_raw(raw_device as _)) };
        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        let command_pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(queue_family_index)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
        }
        .context("could not create the command pool")?;

        Ok(Self {
            entry,
            instance,
            physical_device,
            device,
            queue,
            queue_family_index,
            command_pool,
        })
    }

    pub fn device_name(&self) -> String {
        let props = unsafe {
            self.instance
                .get_physical_device_properties(self.physical_device)
        };
        props
            .device_name_as_c_str()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "<unknown GPU>".to_owned())
    }

    pub fn check_api_version(&self, required: openxr::Version) -> Result<()> {
        let props = unsafe {
            self.instance
                .get_physical_device_properties(self.physical_device)
        };
        let have = (
            vk::api_version_major(props.api_version),
            vk::api_version_minor(props.api_version),
        );
        let want = (required.major() as u32, required.minor() as u32);
        if have < want {
            bail!(
                "the OpenXR runtime needs Vulkan {}.{} but the device reports {}.{}",
                want.0,
                want.1,
                have.0,
                have.1
            );
        }
        Ok(())
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}
