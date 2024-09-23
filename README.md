# Bevy Histrion Packer

![MIT or Apache 2.0](https://img.shields.io/badge/License-MIT%20or%20Apache%202.0-blue.svg)
[![Docs](https://docs.rs/bevy-histrion-packer/badge.svg)](https://docs.rs/bevy-histrion-packer)
[![Crate](https://img.shields.io/crates/v/bevy-histrion-packer.svg)](https://crates.io/crates/bevy-histrion-packer)

> [!WARNING]
> This crate is in early development, and its API may change in the future.

A Bevy plugin to allows to efficiently pack all game assets, such as textures, audio files, and other resources, into a single common PAK like file format.

## Usage

### Packing assets

By default, the `pack_assets_folder` function compress metadata with Brotli and uses Deflate for data except for some specific loaders/extensions:

| Compression Method | Extensions/Loaders                                                                                                                                      |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| None               | .exr, .basis, .ktx2, .qoi, .qoa, .ogg, .oga, .spx, .mp3                                                                                                 |
| Brotli             | .ron, .json, .yml, .yaml, .toml, .txt, .ini, .cfg, .gltf, .wgsl, .glsl, .hlsl, .vert, .frag, .vs, .fs, .lua, .svg, .js, .html, .css, .xml, .mtlx, .usda |
| Deflate            | **Default**                                                                                                                                             |

Pack assets folder with `pack_assets_folder` function:

```toml
# Cargo.toml

[dependencies]
bevy = "0.14"
bevy-histrion-packer = "0.4"

[build-dependencies]
bevy = { version = "0.14", features = [
  "asset_processor",
] }
bevy-histrion-packer = { version = "0.4", features = [
  "writer",
] }
```

```rust
// build.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // only run this code for release builds
    #[cfg(not(debug_assertions))]
    {
        use std::{env, path::PathBuf};

        use bevy_histrion_packer as bhp;

        let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
        let out_dir = PathBuf::from(env::var("OUT_DIR")?);

        let assets_dir = crate_dir.join("assets");
        let processed_dir = crate_dir.join("processed_assets/Default");
        let destination = out_dir.join("assets.hpak");

        // Process assets
        bhp::utils::get_processing_app()?.run();
        bhp::pack_assets_folder(&assets_dir, &processed_dir, &destination)?;
    }

    Ok(())
}
```

It's also possible to have more control over the packing process with `Writer`:

```rust
// build.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // only run this code for release builds
    #[cfg(not(debug_assertions))]
    {
        use bevy_histrion_packer::{CompressionAlgorithm, WriterBuilder};
        use std::fs::{File, OpenOptions};
        use std::path::Path;

        let destination = Path::new("assets.hpak");

        let mut writer = WriterBuilder::new(
            OpenOptions::new()
                .write(true)
                .truncate(true)
                .create(true)
                .open(&destination)?,
        )
        .meta_compression(CompressionAlgorithm::Brotli)
        .build()?;

        let mut data = File::open("assets/texture_1.png")?;
        let mut meta = File::open("assets/texture_1.png.meta")?;

        writer.add_entry(
            Path::new("my_texture_path.png"),
            &mut meta,
            &mut data,
            CompressionAlgorithm::Deflate,
        )?;

        // ...

        writer.finish()?;
    }

    Ok(())
}
```

### Loading assets

```rust
// main.rs
use bevy::prelude::*;
use bevy_histrion_packer::HistrionPackerPlugin;

fn main() {
    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .build()
            .add_before::<bevy::asset::AssetPlugin, HistrionPackerPlugin>(
                HistrionPackerPlugin {
                    source: "assets.hpak".into(),
                    mode: bevy_histrion_packer::HistrionPackerMode::ReplaceDefaultProcessed,
                },
            )
            .set(bevy::asset::AssetPlugin {
                mode: AssetMode::Processed,
                ..default()
            }),
    );

    app.run();
}
```

## Features

| Feature | Description                                                                               |
| ------- | ----------------------------------------------------------------------------------------- |
| deflate | Enables the deflate compression algorithm.                                                |
| brotli  | Enables the brotli compression algorithm.                                                 |
| writer  | Enables the writer feature, to generate a HPAK file from a folder manually with `Writer`. |

## Bevy Compatibility

| bevy   | bevy-histrion-packer |
| ------ | -------------------- |
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
