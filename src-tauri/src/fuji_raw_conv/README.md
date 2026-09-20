# Fujifilm camera-backed RAW conversion (A1)

Phase A1 of camera-backed Fujifilm RAW conversion: PTP transport over `nusb`,
feature-gated so default builds stay free of USB deps.

## Decisions locked in

- Feature flag: `fuji-raw-conv` (not bundled with `tethering`)
- USB crate: `nusb = "=0.2.7"` (pure Rust; still 0.x — pin exact)
- Transport isolated behind `PtpTransport` / `TransportFactory` traits
- Fail-closed session cleanup via `Drop` + explicit `fuji_recover_session`
- User-facing errors for PTP `0x2002` (body mismatch) and wrong USB mode
- WinUSB / udev guidance via `fuji_raw_conv_platform_guidance`
- Capability table: X100VI verified; other Fuji bodies warned as untested

## How to build

```bash
# default — no nusb, stubs return clear errors
cargo check

# with camera RAW CONV support
cargo check --features fuji-raw-conv
```

## Tests

```bash
cargo test --lib fuji_raw_conv
cargo test --lib fuji_raw_conv --features fuji-raw-conv
```

Mock transport covers session open/close/recover without hardware.

## Out of scope for A1

Preset encode/decode (A2), RAF round-trip (A3), UI (A4), pipeline/cache (A5).
