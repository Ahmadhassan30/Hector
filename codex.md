# Hector H20-E Device Enumeration Evidence Report

## 1. Baseline

- Branch: `master`, synchronized with `origin/master`.
- HEAD: `5cdabb5de3a12fb3bfd72389b326df304a04f58b`
- Commit: `feat(platform-windows): implement H16 worker supervision`
- MSVC Rust 1.96.1.
- H14 and architecture checks passed before implementation.

## 2. Governing scope

H20 implements device discovery, capability inventory, default annotations, and deterministic format selection only. H21/H22 stream work was not started.

## 3. Changed paths

Exactly the ten approved paths changed:

- [Cargo.lock](C:\Users\ahmad\Desktop\Hector\Cargo.lock)
- [hector-audio/Cargo.toml](C:\Users\ahmad\Desktop\Hector\crates\hector-audio\Cargo.toml)
- [lib.rs](C:\Users\ahmad\Desktop\Hector\crates\hector-audio\src\lib.rs)
- [error.rs](C:\Users\ahmad\Desktop\Hector\crates\hector-audio\src\error.rs)
- [device.rs](C:\Users\ahmad\Desktop\Hector\crates\hector-audio\src\device.rs)
- [format.rs](C:\Users\ahmad\Desktop\Hector\crates\hector-audio\src\format.rs)
- [selection.rs](C:\Users\ahmad\Desktop\Hector\crates\hector-audio\src\selection.rs)
- [discovery.rs](C:\Users\ahmad\Desktop\Hector\crates\hector-audio\src\discovery.rs)
- [check-architecture.ps1](C:\Users\ahmad\Desktop\Hector\scripts\check-architecture.ps1)
- [check-architecture.tests.ps1](C:\Users\ahmad\Desktop\Hector\scripts\check-architecture.tests.ps1)

No paths are staged.

## 4. CPAL and lockfile

- CPAL pinned exactly to `0.18.1`.
- `default-features = false`.
- License: Apache-2.0.
- Existing 23 lockfile package identities remain unchanged.
- The added packages are CPAL’s target-specific dependency graph; Windows builds select its WASAPI dependencies.
- No ASIO or JACK feature is enabled.

## 5. Windows backend

Discovery explicitly calls `HostId::Wasapi`. No generic-host fallback or other backend is used.

## 6. Public API isolation

Public inventory and selection APIs expose only Hector-owned types. Rustdoc contains no `cpal`, COM, Windows, or raw-handle type.

## 7. Identifier guarantees

`AudioDeviceId` validates bounded `wasapi:` identifiers. Documentation explicitly limits them to current backend/platform snapshots and promises no permanent hardware identity.

## 8. Defaults

Current input/output defaults are recorded as metadata. Selection always requires an explicit exact identifier; no name or default fallback occurs.

## 9. Selection policies

Tests prove:

- Default ranking: 48 kHz, then 44.1 kHz.
- Format ranking: I16, F32, U16.
- Direction-specific channel preferences.
- Highest-rate and lowest-channel fallback.
- Fully custom policies replace default ranking.
- Enumeration order cannot decide ties.

## 10. Unsupported formats

All CPAL sample formats remain inventoried. Formats outside I16/F32/U16 are deterministically rejected during selection.

## 11. Bounds and failures

Tests cover endpoint, input-format, output-format, ID, name, and policy bounds. Backend failures for every discovery operation return no partial inventory and retain no localized diagnostic strings.

## 12. Excluded behavior

Source and rustdoc audits found no:

- stream creation or callback registration;
- capture or playback;
- threads or async tasks;
- resampling, mixing, or conversion;
- raw COM or Windows APIs;
- protocol/runtime/platform coupling.

## 13. Windows integration

Automated WASAPI enumeration and eight repeated snapshot/drop cycles passed. No stream was created.

## 14. Current device inventory

The ignored observation test ran successfully and found four active endpoints:

- Default output: Realtek headphones.
- Additional output: Realtek speakers.
- Default input: Realtek microphone.
- Additional input: Intel microphone array.

Identifiers were observed but are redacted here and were not written to the repository.

Default-policy results:

- Headphones: 48 kHz, stereo, I16.
- Microphone: 48 kHz, stereo, I16.
- Buffer capability reported 480 frames.

The current hardware therefore has usable input and output. Ahmad’s subjective endpoint confirmation remains part of owner review.

## 15. Architecture tests

- 105 adversarial cases passed.
- Exact CPAL version, Windows target, empty feature set, and disabled defaults are enforced.
- Development/build CPAL declarations, direct Windows bindings, ASIO/JACK, codecs, resampling, reverse edges, and unrelated dependencies are rejected.
- H12–H16 rules remain intact.

## 16. Workspace regression

Passed:

- `cargo fmt --all --check`
- workspace check and Clippy with `-D warnings`
- complete workspace tests and doctests
- 36 automated audio tests, with the manual inventory test separately passing
- 44 core tests
- 21 protocol tests
- H15 and H16 suites
- rustdoc generation

## 17. Diff audit

- `git diff --check`: PASS
- Changed paths: 10
- Unexpected paths: 0
- Missing allowlisted paths: 0
- Staged paths: 0
- No unrelated existing package version changed in `Cargo.lock`.

## 18. Limitations

- Device identifiers may change after hardware, driver, Windows, or backend changes.
- CPAL exposes only generic defaults; Windows role-specific defaults are not distinguished.
- Inventory is point-in-time only.
- Stream opening, actual callback compatibility, hot-plug behavior, and device-loss recovery remain untested and deferred.

## 19. Successor boundary

No H21 capture, H22 playback, H23 resampling, H24 recovery, H30 runtime, model, worker, or product implementation began.

## 20. Decision

H20’s implementation and automated acceptance criteria pass. Changes remain unstaged and uncommitted for owner review.

`H20-E STATUS: PASS — STOPPED AT NODE BOUNDARY`