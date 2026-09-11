# ADR 0003 — M0 clears the swapchain rather than drawing a triangle

**Status:** accepted (M0)

## Context

The conventional first milestone for a graphics project is a triangle. It
requires a graphics pipeline, a shader toolchain, SPIR-V compilation, vertex
buffers and a render pass.

M0's actual purpose is narrower: find out whether this machine can open an
OpenXR session and get stereo frames onto the headset, before any effort is
spent on a compositor that would be worthless if it cannot.

None of the pipeline machinery tells us anything about that question. All of it
can fail in ways that look like XR failures, which is the worst possible
property in a diagnostic.

## Decision

**M0 clears each eye's array layer to an animated colour** — teal left, violet
right — using `vkCmdClearColorImage` and layout transitions. No pipeline, no
shaders, no vertex data.

This still exercises everything that is actually at risk: instance and device
creation through the runtime, session lifecycle, stereo swapchain with array
layers, image acquisition and release, layout transitions, queue submission,
projection layer composition, and frame timing.

## Consequences

The asymmetric tint is a deliberate diagnostic. If both eyes look the same in
the headset, the per-eye array layers are not being submitted correctly — a bug
that is easy to introduce, invisible in a screenshot, and much cheaper to catch
at M0 than at M2 with a compositor on top of it.

A unit test asserts the two eye colours never converge, so the diagnostic cannot
silently stop working.

The graphics pipeline arrives at M2, when there is something real to draw.
