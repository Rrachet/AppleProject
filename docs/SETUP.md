# Setup

Getting M0 to display on a Quest from a Linux host. This is the fiddliest part
of the whole project; budget real time for it.

## What you need

- A Linux host with a Vulkan-capable GPU and current drivers
- A Meta Quest (2, 3, Pro or later)
- An OpenXR runtime on the host — **Monado** or SteamVR
- A transport to the headset — **ALVR** (wireless or USB) or a Link cable

## Recommended: Monado + ALVR

Monado is the more predictable runtime on Linux. SteamVR's Linux build works but
fails in more interesting ways.

1. **Install Monado** from your distribution, or build from
   [the upstream repository](https://gitlab.freedesktop.org/monado/monado).

2. **Install ALVR** on both sides — the host streamer and the Quest client
   (SideQuest or the ALVR release APK). Versions must match exactly; mismatched
   versions fail with unhelpful errors.

3. **Point the OpenXR loader at your runtime.** The loader finds it through
   `XR_RUNTIME_JSON`:

   ```bash
   export XR_RUNTIME_JSON=/usr/share/openxr/1/openxr_monado.json
   ```

   ALVR installs its own manifest; if you are driving the headset through ALVR,
   point at that one instead. Check which is active:

   ```bash
   echo $XR_RUNTIME_JSON
   ```

4. **Start ALVR on the host, then the client on the headset,** and confirm they
   pair before running anything.

5. **Run the spike:**

   ```bash
   cargo run -p xr-spike
   ```

## What success looks like

```
INFO xr_spike: OpenXR runtime runtime=Monado version=...
INFO xr_spike: Vulkan device selected by the runtime gpu=...
INFO xr_spike: per-eye recommended resolution width=... height=...
INFO xr_spike: swapchain format format=R8G8B8A8_SRGB
INFO xr_spike: swapchain images count=3
INFO xr_spike: session state state=READY
INFO xr_spike: frame times frames=300 p50=... p95=... p99=...
```

In the headset: **left eye teal, right eye violet**, both pulsing slowly.

Two eyes that look identical means the array layers are not reaching the
compositor correctly. That is a real bug and worth fixing before M1.

## Troubleshooting

**`no OpenXR loader found`**
The loader library is missing. Install `libopenxr-loader` (or your
distribution's equivalent) and confirm `libopenxr_loader.so` is on the library
path.

**`the active OpenXR runtime does not advertise XR_KHR_vulkan_enable2`**
`XR_RUNTIME_JSON` is pointing at the wrong runtime, or at a stale manifest from
an uninstalled one.

**`no head-mounted display found`**
The runtime is up but sees no headset. ALVR is not running, the client has not
connected, or the Link cable is not in a working USB 3 port. Confirm the headset
shows as connected in ALVR before blaming the code.

**`Vulkan instance creation failed` / `could not load libvulkan`**
No Vulkan ICD, or a GPU driver too old for the runtime's required API version.
Check with `vulkaninfo | head`.

**It runs, but the frame-time p99 is enormous**
Expected on wireless. ALVR's encode → network → decode stage dominates and is
outside this project's control. Switch to a USB cable for development, and see
the three-budget split in [ARCHITECTURE.md](ARCHITECTURE.md#latency-budgets)
for how this project accounts for it.

**Session never reaches `READY`**
The runtime parks the session while the headset is idle or the proximity sensor
is uncovered. Put the headset on.
