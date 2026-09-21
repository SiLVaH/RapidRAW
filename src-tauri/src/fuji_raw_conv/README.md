# Fujifilm camera-backed RAW conversion

Camera-backed Fujifilm RAW conversion (USB RAW CONV. / PTP), feature-gated so
default builds stay free of USB deps. Matches X RAW Studio flow:
USB RAF + recipe → camera processor → rendered JPEG.

## Decisions locked in

- Feature flag: `fuji-raw-conv` (not bundled with `tethering`)
- USB crate: `nusb = "=0.2.7"` (pure Rust; still 0.x — pin exact)
- Transport isolated behind `PtpTransport` / `TransportFactory` traits
- Fail-closed session cleanup via `Drop` + explicit `fuji_recover_session`
- User-facing errors for PTP `0x2002` (body mismatch) and wrong USB mode
- WinUSB / udev guidance via `fuji_raw_conv_platform_guidance`
- Capability table: X100VI verified; other Fuji bodies warned as untested
- Conversion uses **D185 native profile patching** (not only D18E–D1A5 presets)
- D183 trigger: `0` = preview / half-res, `1` = full-resolution (default)
- A5: separate camera-render virtual copy; cache under app cache dir keyed
  `hash(RAF)+hash(recipe)` with size limit + purge
- Scene-referred edits grayed out on camera-render versions
- Offline-first queue; export first-class via camera-render VCs
- Study-only reimplementation (no fujihack / no copied filmkit source)

## Conversion pipeline

1. `OpenSession` + capability probe (`GetDevicePropDesc` on D183/D185)
2. `SendObjectInfo` (0x900C) + `SendObject` (0x900D) — upload RAF
3. `GetDevicePropValue` D185 — read base profile (~625 bytes)
4. Patch profile fields (film sim, DR, tones ×10, grain, chrome, WB, NR…)
5. `SetDevicePropValue` D185
6. `SetDevicePropValue` D183 — start conversion
7. Poll `GetObjectHandles` → `GetObject` → `DeleteObject`

Preset properties D18E–D1A5 remain available for encode/decode / future slot sync.

## How to build

```bash
cargo check
cargo check --features fuji-raw-conv
npm run start:fuji-raw-conv
```

## Tests

```bash
cargo test --lib fuji_raw_conv
cargo test --lib fuji_raw_conv --features fuji-raw-conv
```

Includes mock end-to-end convert flow and optional real X100VI RAF samples under
`/tmp/fuji-samples/` (from raw.pixls.us).

## Hardware required for a full render

A Fujifilm body in **USB RAW CONV. / BACKUP RESTORE** mode. Without a camera,
enqueue still works; process queue stays offline-first until connect.
