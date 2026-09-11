# Architecture

## Where things run

The Quest runs Android and cannot host Wayland desktop applications. Running the
compositor on the headset would mean a Linux container on Android, which is slow,
fragile, and not the interesting problem. So:

- **Linux host** — Wayland clients, the compositor, the scene graph, rendering
- **Quest** — stereo display, head pose, hand tracking
- **Between them** — ALVR or Link, carrying encoded frames one way and poses the
  other

See [ADR 0002](adr/0002-host-side-compositor.md).

## Layers

**Compositor core.** A Wayland server built on `smithay`. Owns the protocol
implementation, surface lifecycle, client buffers, and damage tracking. Writing
Wayland from scratch would be months of work that demonstrates nothing; the
interesting code is everything above this layer.

**Scene graph.** Each Wayland surface becomes a textured quad with a 3D
transform. Window management is transform manipulation.

**Renderer.** Vulkan via `ash`, submitting through OpenXR. Both eyes render into
one swapchain image with two array layers. See
[ADR 0001](adr/0001-graphics-api.md).

**Input.** OpenXR hand tracking gives joint poses. A ray from the hand or gaze
is hit-tested against quads; the hit point converts to surface-local pixel
coordinates and is injected as an ordinary `wl_pointer` event.

This is the load-bearing idea of the whole project. Clients need no
modification, no protocol extension, and no awareness of 3D. An unmodified
Firefox is clickable in space because to Firefox, nothing unusual happened.

**Gesture state.** Raw joint poses are not input. Pinch needs debouncing,
confidence gating and recovery when hands leave the tracking frustum — hand
tracking degrades badly at the edges of the camera view and when hands occlude
each other. Treating joint poses as ground truth produces a demo that works for
the person who built it and nobody else.

## Latency budgets

Transport latency is not ours and cannot be optimised from inside this codebase,
so it is accounted for separately rather than folded into one misleading number:

| Budget | Span | Ownership |
|---|---|---|
| **Compositor-internal** | surface damage → GPU submit | Ours. Optimise. |
| **Interaction** | pinch detected → client event → frame submitted | Mostly ours. The number that matters. |
| **Transport** | submit → photon | ALVR's. Measure, report, disclaim. |

Head-tracking latency is largely handled by the runtime's asynchronous
reprojection, which warps the submitted frame using the latest head pose before
display. Our budget therefore governs *interaction* responsiveness rather than
head motion — a distinction that changes what is worth optimising.

At 90Hz the total frame budget is 11.1ms.

## Known risks

**Text legibility.** The most underestimated problem. Quest per-eye resolution
makes normal desktop font sizes unreadable in the headset. Mitigations —
rendering surfaces above native resolution and supersampling, curving panels,
forcing client scaling — all cost fill rate. Projects like this routinely look
good in screenshots and are useless to actually work in.

**DMA-BUF → Vulkan import.** Getting a client's buffer into a Vulkan image is
the hardest single piece of M2. See [ADR 0001](adr/0001-graphics-api.md) for the
fallback if it proves intractable.

**Hand tracking quality at the edges.** Mitigated by the gesture state layer
above, not by wishing.
