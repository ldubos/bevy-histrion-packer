<h1 align="center"> Bevy Histrion Packer</h1>

<div align="center">

![MIT or Apache 2.0](https://img.shields.io/badge/License-MIT%20or%20Apache%202.0-blue.svg)
[![Crate](https://img.shields.io/crates/v/bevy-histrion-packer.svg)](https://crates.io/crates/bevy-histrion-packer)
[![Docs](https://docs.rs/bevy-histrion-packer/badge.svg)](https://docs.rs/bevy-histrion-packer)
[![CI](https://github.com/ldubos/bevy-histrion-packer/workflows/CI/badge.svg)](https://github.com/ldubos/bevy-histrion-packer/actions)

Pack all your game assets into a single common PAK like file format.

</div>

> [!WARNING]
> This crate is in early development.<br/>
> Use it with caution as the format and API is not yet stabilized.

## Features

| feature | description                                                                              |
| ------- | ---------------------------------------------------------------------------------------- |
| deflate | Enables the deflate compression algorithm.                                               |
| writer  | Enables the ability to generate a HPAK file with [`HpakWriter`](./src/format/writer.rs). |

## Bevy Compatibility

| bevy   | bevy-histrion-packer |
| ------ | -------------------- |
| `0.15` | `0.5`                |
| `0.14` | `0.4`                |
| `0.13` | `0.2-0.3`            |
| `0.12` | `0.1`                |

## License

Dual-licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](/LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](/LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
