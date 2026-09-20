# Fujifilm camera-backed RAW conversion

Camera-backed Fujifilm RAW conversion (USB RAW CONV. / PTP), feature-gated so
default builds stay free of USB deps. Matches X RAW Studio flow:
USB RAF + recipe params → camera processor → rendered JPEG.

## Decisions locked in

- Feature flag: `fuji-raw-conv` (not bundled with `tethering`)
- USB crate: `nusb = "=0.2.7"` (pure Rust; still 0.x — pin exact)
- Transport isolated behind `PtpTransport` / `TransportFactory` traits
- Fail-closed session cleanup via `Drop` + explicit `fuji_recover_session`
- User-facing errors for PTP `0x2002` (body mismatch) and wrong USB mode
- WinUSB / udev guidance via `fuji_raw_conv_platform_guidance`
- Capability table: X100VI verified; other Fuji bodies warned as untested
- A5: separate camera-render virtual copy; cache under app cache dir keyed
  `hash(RAF)+hash(recipe)` with size limit + purge
- Scene-referred edits grayed out on camera-render versions
- Offline-first queue (enqueue without camera; process when connected)
- Export is first-class via camera-render virtual copies

## Phases

| Phase | Status | Contents |
|-------|--------|----------|
| A1 | done | PTP transport over nusb, session, capabilities, platform guidance |
| A2 | done | Preset encode/decode D18E–D1A5 + HighIsoNR/mono/CT rules |
| A3 | done | RAF conversion round-trip via mockable `RawConverter` trait |
| A4 | done | Fuji Recipe UI panel, status, export camera-render choice |
| A5 | done | Camera-render version, disk cache, offline queue, sidecar flags |

Goal B (X-Trans demosaic / embedded lens tables) remains report-only until A ships.

## How to build

```bash
# default — no nusb, stubs return clear errors
cargo check

# with camera RAW CONV support
cargo check --features fuji-raw-conv

# frontend + feature (from repo root)
npm run start:fuji-raw-conv
```

## Tests

```bash
cargo test --lib fuji_raw_conv
cargo test --lib fuji_raw_conv --features fuji-raw-conv
```

Mock transport covers session open/close/recover and conversion without hardware.

## Study-only note

Protocol constants and flow are reimplemented from public PTP/ISO docs and
study of prior art licenses (filmkit MIT, rawji GPL-3+, libfuji MIT). No
fujihack code is copied.
