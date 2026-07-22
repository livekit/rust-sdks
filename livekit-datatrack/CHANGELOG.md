# Changelog
## 0.1.11 (2026-07-14)

### Fixes

- Make some fields public for data track types
- Refactor data tracks E2EE interface
- Use concrete type for data track manager output events

## 0.1.10 (2026-07-09)

### Fixes

- Handle data track SID reassignment
- introduce LiveKitAPI construct, added smoke tests - #1220 (@davidzhao)

## 0.1.9 (2026-06-23)

### Fixes

- Upgrade protocol to v1.48.0

## 0.1.8 (2026-05-29)

### Fixes

- bump protocol to v1.46.4 - #1121 (@lukasIO)

## 0.1.7 (2026-05-21)

### Features

- Introduce pipeline options for remote data tracks, support multiple in-flight frames.

### Fixes

- Fix compilation error in depacketizer test by using correct variable name.

## 0.1.6 (2026-05-18)

### Fixes

- Add AGENTS.md and minor doc revisions
- Add `cargo-fuzz` target for packet deserialization

## 0.1.5 (2026-05-11)

### Fixes

- Upgrade protocol to v1.45.8

## 0.1.4 (2026-04-23)

### Fixes

- Fix data track packet format issue breaking E2EE

## 0.1.3 (2026-04-02)

### Features

- Rename type to `DataTrackStream`

## 0.1.2 (2026-03-31)

### Fixes

- Upgrade to thiserror 2

## 0.1.1 (2026-03-22)

### Features

- Initial release.
