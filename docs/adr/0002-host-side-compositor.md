# ADR 0002 — The compositor runs on the host, not the headset

**Status:** accepted (M0)

## Context

A Quest is a standalone Android device with a capable SoC. Running the
compositor on it would remove the streaming stage entirely and take perhaps
30–40ms out of the end-to-end path.

The problem is what the compositor composites. This project's entire premise is
that *real, unmodified Linux applications* live in space. Firefox, a terminal, an
editor. Those are Wayland clients and they need a Linux userspace.

Running them on the Quest means a container — proot, Termux, or similar — which
is slow, fragile, and turns the project into an Android porting exercise rather
than a compositor.

## Decision

**The compositor runs on a Linux host. The Quest is a stereo display and a
sensor package**, reached over ALVR or Link.

## Consequences

The streaming stage is the dominant term in end-to-end latency and is entirely
outside this codebase. A single end-to-end number would therefore be mostly a
measurement of ALVR, not of this project, so latency is reported as three
separate budgets — see [ARCHITECTURE.md](../ARCHITECTURE.md#latency-budgets).

Development should use a wired Link connection where possible. Debugging a
compositor latency problem through a wireless video encoder is miserable and
produces untrustworthy numbers.

## What would change this

A headset that runs a Linux userspace directly, or a Quest-side container that
turns out to be good enough. Neither is worth betting the project on today.
