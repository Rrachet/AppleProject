//! Minimal renderer: clears each eye's swapchain layer to a colour.
//!
//! M0 deliberately has no graphics pipeline. A triangle would exercise shader
//! compilation and vertex buffers, neither of which can tell us anything about
//! the thing actually at risk here — whether a session on this machine reaches
//! the headset at all. A clear does exercise every part that can: swapchain
//! acquisition, per-eye array layers, layout transitions, queue submission and
//! frame timing.
//!
//! The two eyes are tinted differently on purpose. If both eyes look identical
//! in the headset, the stereo array layers are not being submitted correctly,
//! and that is worth catching now rather than at M2.

use anyhow::{Context, Result};
use ash::vk;

use crate::vulkan::VulkanContext;

/// Left eye leans teal, right eye leans violet, both pulse over time.
pub fn eye_colour(eye: usize, seconds: f32) -> [f32; 4] {
    let pulse = 0.35 + 0.25 * (seconds * 1.5).sin().abs();
    match eye {
        0 => [0.05, pulse, pulse * 0.9, 1.0],
        _ => [pulse * 0.7, 0.05, pulse, 1.0],
    }
}

pub struct Renderer {
    command_buffers: Vec<vk::CommandBuffer>,
    fences: Vec<vk::Fence>,
}

impl Renderer {
    pub fn new(vk_ctx: &VulkanContext, image_count: usize) -> Result<Self> {
        let command_buffers = unsafe {
            vk_ctx.device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(vk_ctx.command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(image_count as u32),
            )
        }
        .context("could not allocate command buffers")?;

        // Created signalled so the first frame does not wait on a fence that
        // will never be signalled.
        let fences = (0..image_count)
            .map(|_| unsafe {
                vk_ctx.device.create_fence(
                    &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                    None,
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .context("could not create fences")?;

        Ok(Self {
            command_buffers,
            fences,
        })
    }

    /// Clears both eye layers of `image` and submits. Blocks until the previous
    /// submission for this swapchain slot has retired.
    pub fn render(
        &self,
        vk_ctx: &VulkanContext,
        image_index: usize,
        image: vk::Image,
        seconds: f32,
    ) -> Result<()> {
        let device = &vk_ctx.device;
        let cmd = self.command_buffers[image_index];
        let fence = self.fences[image_index];

        unsafe {
            device
                .wait_for_fences(&[fence], true, u64::MAX)
                .context("waiting on the previous frame's fence")?;
            device.reset_fences(&[fence])?;
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;

            device.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;

            let full_range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(2);

            // The runtime hands the image back in an undefined layout; we do not
            // need its previous contents because we overwrite every pixel.
            transition(
                device,
                cmd,
                image,
                full_range,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
            );

            for eye in 0..2usize {
                let clear = vk::ClearColorValue {
                    float32: eye_colour(eye, seconds),
                };
                let range = vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(eye as u32)
                    .layer_count(1);
                device.cmd_clear_color_image(
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &clear,
                    &[range],
                );
            }

            // OpenXR requires colour images to be released in
            // COLOR_ATTACHMENT_OPTIMAL.
            transition(
                device,
                cmd,
                image,
                full_range,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            );

            device.end_command_buffer(cmd)?;

            let buffers = [cmd];
            let submit = vk::SubmitInfo::default().command_buffers(&buffers);
            device
                .queue_submit(vk_ctx.queue, &[submit], fence)
                .context("submitting the frame")?;
        }

        Ok(())
    }

    pub fn destroy(&mut self, vk_ctx: &VulkanContext) {
        unsafe {
            let _ = vk_ctx.device.device_wait_idle();
            for fence in self.fences.drain(..) {
                vk_ctx.device.destroy_fence(fence, None);
            }
            vk_ctx
                .device
                .free_command_buffers(vk_ctx.command_pool, &self.command_buffers);
        }
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn transition(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    range: vk::ImageSubresourceRange,
    from: vk::ImageLayout,
    to: vk::ImageLayout,
    src_access: vk::AccessFlags,
    dst_access: vk::AccessFlags,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
) {
    let barrier = vk::ImageMemoryBarrier::default()
        .old_layout(from)
        .new_layout(to)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(range)
        .src_access_mask(src_access)
        .dst_access_mask(dst_access);

    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::eye_colour;

    #[test]
    fn eyes_are_tinted_differently() {
        // If these ever converge the stereo diagnostic silently stops working.
        let left = eye_colour(0, 0.0);
        let right = eye_colour(1, 0.0);
        assert_ne!(left, right);
    }

    #[test]
    fn colour_channels_stay_in_range() {
        for step in 0..240 {
            let seconds = step as f32 / 24.0;
            for eye in 0..2 {
                for channel in eye_colour(eye, seconds) {
                    assert!(
                        (0.0..=1.0).contains(&channel),
                        "channel {channel} out of range at t={seconds}"
                    );
                }
            }
        }
    }
}
