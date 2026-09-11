# Spatial Shell

> A Wayland compositor that renders real Linux applications as floating panels
> in 3D space, driven by hand tracking on a Meta Quest.

Not a 3D mockup of windows. A real compositor: Firefox, a terminal and an
editor actually running, actually interactive, floating in space, where you
reach out and pinch to click them.

**Status: M0 — risk spike.** The XR display path is implemented and compiles;
it has not yet been run against hardware. Nothing else exists yet. See
[Milestones](#milestones).

---

## Why

visionOS's interesting claim is not that it renders windows in 3D — it is that
*your existing applications live in space*. This project makes the same claim
on a different substrate: Wayland clients, a Quest, and a latency budget.

The engineering content is the latency budget. At 90Hz there are 11.1ms per
frame to capture hand poses, hit-test them against surfaces, deliver input to
clients, composite, and submit. Most of the work here is measuring where that
time goes and taking it back.

## Architecture

The Quest cannot run Wayland desktop applications, so the compositor runs on a
Linux host and the headset is a stereo display plus a sensor package.

```
Linux host                                        Quest
─────────────────────────────────────────         ──────────────────
Firefox, terminal, editor
        │ (Wayland)
        ▼
smithay compositor  ──► scene graph ──► stereo ──► ALVR / Link ──► display
        ▲                (quad per       render      (encode→net    (runtime
        │                 surface)         │          →decode)       reprojects)
        │                                  │
   synthesized                        OpenXR ◄──── head pose
   wl_pointer events ◄── hit-test ◄── hand joints ◄── hand tracking
```

The mechanism at the centre of the whole design: **a ray from the hand or gaze
hits a quad, the hit point converts to surface-local pixel coordinates, and
that is injected as an ordinary `wl_pointer` event.** The client never learns it
is in 3D. That is why unmodified applications work.

Longer form in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Decisions and their
trade-offs are recorded in [docs/adr/](docs/adr/).

## Milestones

| | Goal | Status |
|---|---|---|
| **M0** | Stereo frames reach the headset | Implemented, **unverified on hardware** |
| **M1** | Flat smithay compositor — `weston-terminal` runs | Not started |
| **M2** | Wayland surfaces as textured quads in XR | Not started |
| **M3** | Hand tracking → synthesized `wl_pointer` events | Not started |
| **M4** | Spatial window management — grab, move, scale, anchor | Not started |
| **M5** | Latency instrumentation and optimisation | Not started |
| **M6** | Documentation, demo, polish | Not started |

M3 is when the project becomes real. Everything before it is scaffolding.

## Running M0

```bash
cargo run -p xr-spike
```

Success is a **teal left eye and a violet right eye**, both pulsing, with a
frame-time summary printing every 300 frames at roughly the headset's refresh
cadence.

If both eyes look identical, the stereo array layers are not being submitted
correctly — that is the diagnostic this spike exists to provide.

Runtime setup, ALVR/Monado configuration and troubleshooting are in
[docs/SETUP.md](docs/SETUP.md).

## Development

```bash
cargo build              # build
cargo test               # unit tests
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

CI runs all four on every push and pull request. It cannot run the XR path —
that needs a headset — so anything hardware-dependent stays behind explicit
manual verification, and this README says plainly which parts are unverified.

## Scope, honestly

This is not visionOS and will not become it. visionOS spans a kernel, a
perception stack on dedicated silicon, and a spatial compositor. Apple hits
~12ms photon-to-photon because the R1 chip exists for that purpose.

This project implements the third layer only, and accepts a streaming transport
it does not control. The interesting question is not "can it match Apple" — it
cannot — but "how much of the budget belongs to the compositor, and how small
can that part get".

## Licence

MIT — see [LICENSE](LICENSE).
