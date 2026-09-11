//! M0 risk spike for Spatial Shell.
//!
//! Goal: prove that this machine can open an OpenXR session, submit stereo
//! frames, and get them onto the headset. Nothing else. If this does not run,
//! no amount of compositor work matters, so it is deliberately the first thing
//! built and the smallest thing that can fail meaningfully.
//!
//! Success looks like: a teal left eye and a violet right eye, both pulsing,
//! with the frame-time summary printing a cadence close to the headset's
//! refresh rate.

mod renderer;
mod vulkan;

use anyhow::{Context, Result, bail};
use ash::vk::{self, Handle};
use std::time::Instant;

use renderer::Renderer;
use vulkan::VulkanContext;

const VIEW_TYPE: openxr::ViewConfigurationType = openxr::ViewConfigurationType::PRIMARY_STEREO;
const EYES: usize = 2;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "xr_spike=info".into()),
        )
        .init();

    if let Err(error) = run() {
        tracing::error!("{error:#}");
        eprintln!("\nSee docs/SETUP.md for runtime setup and troubleshooting.");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // SAFETY: loads the OpenXR loader shared library. Safe as long as no other
    // thread is concurrently loading or unloading it; we are single-threaded here.
    let entry = unsafe { openxr::Entry::load() }
        .context("no OpenXR loader found — install Monado or SteamVR, see docs/SETUP.md")?;

    let available = entry
        .enumerate_extensions()
        .context("could not enumerate OpenXR extensions")?;
    if !available.khr_vulkan_enable2 {
        bail!(
            "the active OpenXR runtime does not advertise XR_KHR_vulkan_enable2; \
             check XR_RUNTIME_JSON points at a runtime that supports Vulkan"
        );
    }

    let mut extensions = openxr::ExtensionSet::default();
    extensions.khr_vulkan_enable2 = true;

    let xr_instance = entry
        .create_instance(
            &openxr::ApplicationInfo {
                application_name: "spatial-shell-xr-spike",
                application_version: 1,
                engine_name: "spatial-shell",
                engine_version: 1,
                api_version: openxr::Version::new(1, 0, 0),
            },
            &extensions,
            &[],
        )
        .context("could not create an OpenXR instance")?;

    let properties = xr_instance.properties()?;
    tracing::info!(
        runtime = %properties.runtime_name,
        version = %properties.runtime_version,
        "OpenXR runtime"
    );

    // Fails here when no headset is connected, which is the most common cause
    // of a first-run failure.
    let system = xr_instance
        .system(openxr::FormFactor::HEAD_MOUNTED_DISPLAY)
        .context(
            "no head-mounted display found — is the headset connected and ALVR/Link running?",
        )?;

    let requirements = xr_instance.graphics_requirements::<openxr::Vulkan>(system)?;

    let vk_ctx = unsafe { VulkanContext::new(&xr_instance, system) }?;
    vk_ctx.check_api_version(requirements.min_api_version_supported)?;
    tracing::info!(gpu = %vk_ctx.device_name(), "Vulkan device selected by the runtime");

    let (session, mut frame_waiter, mut frame_stream) = unsafe {
        xr_instance.create_session::<openxr::Vulkan>(
            system,
            &openxr::vulkan::SessionCreateInfo {
                instance: vk_ctx.instance.handle().as_raw() as _,
                physical_device: vk_ctx.physical_device.as_raw() as _,
                device: vk_ctx.device.handle().as_raw() as _,
                queue_family_index: vk_ctx.queue_family_index,
                queue_index: 0,
            },
        )
    }
    .context("could not create the OpenXR session")?;

    // STAGE is floor-relative and fixed to the room, which is what a spatial
    // window manager wants. LOCAL would follow recentering and make windows
    // appear to drift.
    let stage = session
        .create_reference_space(openxr::ReferenceSpaceType::STAGE, openxr::Posef::IDENTITY)
        .context("could not create a STAGE reference space")?;

    let views = xr_instance.enumerate_view_configuration_views(system, VIEW_TYPE)?;
    if views.len() != EYES {
        bail!(
            "expected {EYES} views for stereo, runtime reported {}",
            views.len()
        );
    }
    let width = views[0].recommended_image_rect_width;
    let height = views[0].recommended_image_rect_height;
    tracing::info!(width, height, "per-eye recommended resolution");

    let format = choose_format(&session)?;

    // One image with two array layers, rather than one swapchain per eye: it
    // halves the submission bookkeeping and matches how the compositor will
    // want to render both eyes in a single pass later.
    let swapchain = session
        .create_swapchain(&openxr::SwapchainCreateInfo {
            create_flags: openxr::SwapchainCreateFlags::EMPTY,
            usage_flags: openxr::SwapchainUsageFlags::COLOR_ATTACHMENT
                | openxr::SwapchainUsageFlags::TRANSFER_DST,
            format,
            sample_count: 1,
            width,
            height,
            face_count: 1,
            array_size: EYES as u32,
            mip_count: 1,
        })
        .context("could not create the swapchain")?;

    let images: Vec<vk::Image> = swapchain
        .enumerate_images()?
        .into_iter()
        .map(vk::Image::from_raw)
        .collect();
    tracing::info!(count = images.len(), "swapchain images");

    let mut swapchain = swapchain;
    let mut renderer = Renderer::new(&vk_ctx, images.len())?;
    let result = frame_loop(
        &xr_instance,
        &session,
        &mut frame_waiter,
        &mut frame_stream,
        &stage,
        &mut swapchain,
        &images,
        &renderer,
        &vk_ctx,
        width,
        height,
    );
    renderer.destroy(&vk_ctx);
    result
}

/// Picks a colour format the runtime offers, preferring sRGB so the compositor
/// does not have to gamma-correct by hand later.
///
/// OpenXR reports Vulkan formats as raw `u32`, so the comparison happens on
/// raw values rather than `vk::Format`.
fn choose_format(session: &openxr::Session<openxr::Vulkan>) -> Result<u32> {
    let offered = session.enumerate_swapchain_formats()?;
    const PREFERRED: [vk::Format; 4] = [
        vk::Format::R8G8B8A8_SRGB,
        vk::Format::B8G8R8A8_SRGB,
        vk::Format::R8G8B8A8_UNORM,
        vk::Format::B8G8R8A8_UNORM,
    ];
    for candidate in PREFERRED {
        let raw = candidate.as_raw() as u32;
        if offered.contains(&raw) {
            tracing::info!(format = ?candidate, "swapchain format");
            return Ok(raw);
        }
    }
    bail!("runtime offered no supported colour format; it advertised {offered:?}")
}

#[allow(clippy::too_many_arguments)]
fn frame_loop(
    xr_instance: &openxr::Instance,
    session: &openxr::Session<openxr::Vulkan>,
    frame_waiter: &mut openxr::FrameWaiter,
    frame_stream: &mut openxr::FrameStream<openxr::Vulkan>,
    stage: &openxr::Space,
    swapchain: &mut openxr::Swapchain<openxr::Vulkan>,
    images: &[vk::Image],
    renderer: &Renderer,
    vk_ctx: &VulkanContext,
    width: u32,
    height: u32,
) -> Result<()> {
    let mut event_storage = openxr::EventDataBuffer::new();
    let mut session_running = false;
    let started = Instant::now();
    let mut stats = FrameStats::default();

    loop {
        while let Some(event) = xr_instance.poll_event(&mut event_storage)? {
            use openxr::Event::*;
            use openxr::SessionState;
            match event {
                SessionStateChanged(e) => {
                    tracing::info!(state = ?e.state(), "session state");
                    match e.state() {
                        SessionState::READY => {
                            session.begin(VIEW_TYPE)?;
                            session_running = true;
                        }
                        SessionState::STOPPING => {
                            session.end()?;
                            session_running = false;
                        }
                        SessionState::EXITING | SessionState::LOSS_PENDING => {
                            tracing::info!("runtime asked us to exit");
                            stats.report_final();
                            return Ok(());
                        }
                        _ => {}
                    }
                }
                InstanceLossPending(_) => {
                    tracing::warn!("OpenXR instance loss pending");
                    stats.report_final();
                    return Ok(());
                }
                _ => {}
            }
        }

        if !session_running {
            // Not an error: the runtime parks us here while the headset is idle
            // or the proximity sensor is uncovered.
            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        }

        let frame_state = frame_waiter.wait()?;
        frame_stream.begin()?;

        if !frame_state.should_render {
            frame_stream.end(
                frame_state.predicted_display_time,
                openxr::EnvironmentBlendMode::OPAQUE,
                &[],
            )?;
            continue;
        }

        let frame_started = Instant::now();

        let image_index = swapchain.acquire_image()?;
        swapchain.wait_image(openxr::Duration::INFINITE)?;

        renderer.render(
            vk_ctx,
            image_index as usize,
            images[image_index as usize],
            started.elapsed().as_secs_f32(),
        )?;

        swapchain.release_image()?;

        let (_flags, views) =
            session.locate_views(VIEW_TYPE, frame_state.predicted_display_time, stage)?;

        let rect = openxr::Rect2Di {
            offset: openxr::Offset2Di { x: 0, y: 0 },
            extent: openxr::Extent2Di {
                width: width as i32,
                height: height as i32,
            },
        };
        let projection_views: Vec<_> = (0..EYES)
            .map(|eye| {
                openxr::CompositionLayerProjectionView::new()
                    .pose(views[eye].pose)
                    .fov(views[eye].fov)
                    .sub_image(
                        openxr::SwapchainSubImage::new()
                            .swapchain(swapchain)
                            .image_array_index(eye as u32)
                            .image_rect(rect),
                    )
            })
            .collect();

        frame_stream.end(
            frame_state.predicted_display_time,
            openxr::EnvironmentBlendMode::OPAQUE,
            &[&openxr::CompositionLayerProjection::new()
                .space(stage)
                .views(&projection_views)],
        )?;

        stats.record(frame_started.elapsed().as_secs_f64() * 1000.0);
    }
}

/// Crude frame-time accounting. This is the seed of the latency instrumentation
/// the project actually cares about; for now it only answers "are we hitting
/// the display cadence at all?".
#[derive(Default)]
struct FrameStats {
    samples: Vec<f64>,
    total: u64,
}

impl FrameStats {
    const REPORT_EVERY: usize = 300;

    fn record(&mut self, millis: f64) {
        self.samples.push(millis);
        self.total += 1;
        if self.samples.len() >= Self::REPORT_EVERY {
            self.report("frame times");
            self.samples.clear();
        }
    }

    fn report(&mut self, label: &str) {
        if self.samples.is_empty() {
            return;
        }
        let mut sorted = self.samples.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let pick = |q: f64| sorted[((sorted.len() - 1) as f64 * q).round() as usize];
        tracing::info!(
            frames = self.total,
            p50 = format!("{:.2}ms", pick(0.50)),
            p95 = format!("{:.2}ms", pick(0.95)),
            p99 = format!("{:.2}ms", pick(0.99)),
            max = format!("{:.2}ms", sorted[sorted.len() - 1]),
            "{label}"
        );
    }

    fn report_final(&mut self) {
        self.report("final frame times");
    }
}
