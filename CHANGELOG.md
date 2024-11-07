# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added a complete example in `example` to show how to use BHP end-to-end

### Changed

- Bump `bevy` version to `0.15`
- Bump `thiserror` version to `2.0`
- `Deflate` compression now uses [google's zopfli](https://crates.io/crates/zopfli) for better compression ratio
- `Writer` no longer uses tempfile for writing the archive
- The archive format has been updated to version 6
- `HpakReader` now uses `memmap2` to read the archive
- `HpakWriter` can now take `with_padding` option to align data entries to 4096 bytes
- `pack_assets_folder` now takes more options to control the compression method and padding

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

[Unreleased]: https://github.com/ldubos/bevy-histrion-packer/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/ldubos/bevy-histrion-packer/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/ldubos/bevy-histrion-packer/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ldubos/bevy-histrion-packer/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/ldubos/bevy-histrion-packer/releases/tag/v0.1.3
