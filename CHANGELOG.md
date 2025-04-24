# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0] - 2025-24-04

## Changed

- Upgraded `bevy` to `0.16`
- Upgraded `futures-lite` to `2.6`
- Updated the example

## [0.5.0] - 2024-11-29

### Added

- Added a complete example in `example` to show how to use BHP end-to-end
- Added RON minification for `.meta` files in `HpakWriter`
- Added a `with_alignment` option to align the pairs of meta+data to N bytes in `HpakWriter`

### Changed

- Upgraded `bevy` to `0.15`
- Upgraded `thiserror` to `2.0`
- `Deflate` compression now uses [google's zopfli](https://crates.io/crates/zopfli) for better compression ratios
- `HpakWriter` no longer uses tempfile for writing the archive
- The archive format has been updated to version 6
- Switched to `memmap2` for improved archive reading in `HpakReader`
- `pack_assets_folder` now takes more options to control the compression method and alignment

### Removed

- Removed `bevy_histrion_packer::utils` module
- Removed `brotli` support in compression methods
- Removed `brotli` dependency
- Removed `tempfile` dependency
- Removed `walkdir` dependency
- Removed `serde` dependency

## [0.4.0] - 2024-06-15

## [0.3.0] - 2024-05-20

## [0.2.0] - 2024-02-18

## [0.1.3] - 2023-12-30

Initial release.

[Unreleased]: https://github.com/ldubos/bevy-histrion-packer/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/ldubos/bevy-histrion-packer/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/ldubos/bevy-histrion-packer/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/ldubos/bevy-histrion-packer/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/ldubos/bevy-histrion-packer/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ldubos/bevy-histrion-packer/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/ldubos/bevy-histrion-packer/releases/tag/v0.1.3
