# Bevy Histrion Packer

![MIT or Apache 2.0](https://img.shields.io/badge/License-MIT%20or%20Apache%202.0-blue.svg)
[![Docs](https://docs.rs/bevy-histrion-packer/badge.svg)](https://docs.rs/bevy-histrion-packer)
[![Crate](https://img.shields.io/crates/v/bevy-histrion-packer.svg)](https://crates.io/crates/bevy-histrion-packer)

A Bevy plugin to pack assets into a single file :boom:

```rust
// build.rs
use std::path::Path;

use bevy_histrion_packer::pack_assets_folder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = Path::new("imported_assets/Default");
    let destination = Path::new("assets.hpak");

    pack_assets_folder(source, destination, false)?;
    Ok(())
}
```

```rust
// src/main.rs
use bevy::prelude::*;
use bevy_histrion_packer::HistrionPackerPlugin;

fn main() {
    App::new().add_plugins((
        HistrionPackerPlugin {
            source: "assets.hpak".into(),
            mode: bevy_histrion_packer::HistrionPackerMode::ReplaceDefaultProcessed,
        },
        DefaultPlugins,
    ));
}
```

## Bevy Compatibility

|bevy|bevy-histrion-packer|
|---|---|
|`0.13`|`0.2`|
|`0.12`|`0.1`|

## License

Dual-licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](/LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](/LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
