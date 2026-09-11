# ADR 0001 — Vulkan rather than OpenGL ES

**Status:** accepted (M0)

## Context

The renderer has to satisfy two consumers that pull in opposite directions.

**OpenXR** needs a graphics binding. Vulkan (`XR_KHR_vulkan_enable2`) is
supported by every runtime that matters — Monado, SteamVR, ALVR. OpenGL
(`XR_KHR_opengl_enable`) is supported by Monado but is patchier elsewhere.

**smithay** renders with GLES via EGL, and already solves importing a client's
DMA-BUF as an `EGLImage` and therefore a GL texture. That is the single hardest
piece of M2, and on the GL path it is free.

Choosing GL means the easy path at M2 and runtime risk at M0. Choosing Vulkan
means the safe path at M0 and real work at M2.

## Decision

**Vulkan, via `ash`.**

M0 exists to retire the risk that frames never reach the headset at all. That
risk is highest on the less-supported binding, and a runtime that will not
present is unfixable from inside this codebase — whereas buffer import is only
difficult, and difficulty is schedulable.

Vulkan is also the better long-term choice: it is where OpenXR support is
strongest, it gives explicit control over the synchronisation that the latency
work will depend on, and mixing a GL XR session with a Vulkan compositor later
would be strictly worse than starting on one API.

Practically, `ash` loads Vulkan dynamically, so no Vulkan SDK or headers are
needed to build — which also keeps CI able to compile the renderer.

## Consequences

At M2 the DMA-BUF import must be written by hand against
`VK_EXT_external_memory_dma_buf` and `VK_EXT_image_drm_format_modifier`, rather
than inherited from smithay. Estimated one to two weeks.

That work is not a tax — it is one of the more genuinely interesting systems
problems in the project, and worth being able to explain.

## Fallback

If the import proves intractable, smithay's `GlesRenderer` can render surfaces
into an offscreen GL texture which is then imported into Vulkan through
`VK_KHR_external_memory_fd`. Slower, an extra copy per frame, and it muddies the
latency story — so it is a fallback, not a plan.

If *that* also fails, the last resort is moving the whole renderer to GL and
accepting Monado as the only supported runtime. This would be a significant
reduction in scope and should be a deliberate decision, not a drift.
