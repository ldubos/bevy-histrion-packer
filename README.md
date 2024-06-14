# Bevy Histrion Packer

![MIT or Apache 2.0](https://img.shields.io/badge/License-MIT%20or%20Apache%202.0-blue.svg)
[![Docs](https://docs.rs/bevy-histrion-packer/badge.svg)](https://docs.rs/bevy-histrion-packer)
[![Crate](https://img.shields.io/crates/v/bevy-histrion-packer.svg)](https://crates.io/crates/bevy-histrion-packer)

> [!WARNING]
> This crate is in early development and it's API may change in the future.
>
> The purpose of this crate is mainly to pack game assets for the release build, not for development.

A Bevy plugin to allows to efficiently pack all game assets, such as textures, audio files, and other resources, into a single common PAK like file format.

## Usage

### Packing assets

By default, the `pack_assets_folder` function compress metadata with Brotli and uses Deflate for data except for some specific loaders/extensions:

|Compression Method|Extensions/Loaders|
|------------------|------------------|
|None              |.exr, .basis, .ktx2, .qoi, .qoa, .ogg, .oga, .spx, .mp3|
|Brotli            |.ron, .json, .yml, .yaml, .toml, .txt, .ini, .cfg, .gltf, .wgsl, .glsl, .hlsl, .vert, .frag, .vs, .fs, .lua, .svg, .js, .html, .css, .xml, .mtlx, .usda|
|Deflate           |**Default**|

Pack assets folder with `pack_assets_folder` function:

```rust
// build.rs
use std::path::Path;

use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    asset::processor::AssetProcessor,
    prelude::*,
};
use bevy_histrion_packer::pack_assets_folder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // delete the imported_assets folder
    let _ = std::fs::remove_dir_all(&PathBuf::from("imported_assets"));

    // generate the processed assets during build time
    let mut app = App::new();

    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(
        std::time::Duration::from_millis(16),
    )))
    .add_plugins(bevy::asset::AssetPlugin {
        mode: AssetMode::Processed,
        ..Default::default()
    })
    .init_asset::<Shader>()
    .init_asset_loader::<bevy::render::render_resource::ShaderLoader>()
    .add_plugins(bevy::render::texture::ImagePlugin::default())
    .add_plugins(bevy::pbr::PbrPlugin::default())
    .add_plugins(bevy::gltf::GltfPlugin::default())
    .add_systems(
        Update,
        |asset_processor: Res<AssetProcessor>, mut exit_tx: EventWriter<AppExit>| {
            match bevy::tasks::block_on(asset_processor.get_state()) {
                bevy::asset::processor::ProcessorState::Finished => {
                    exit_tx.send(AppExit);
                }
                _ => {}
            }
        },
    );

    app.run();

    // pack the assets folder
    let source = Path::new("imported_assets/Default");
    let destination = Path::new("assets.hpak");

    pack_assets_folder(&source, &destination)?;
}
```

It's possible to do it manually with the `Writer`:

```rust
// build.rs
use std::fs::OpenOptions;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let mut data = File::open("texture_1.png")?;
    let mut meta = File::open("texture_1.png.meta")?;

    writer.add_entry(
        &mut meta,
        &mut data,
        CompressionAlgorithm::Deflate,
    )?;

    // ...

    writer.finish()?;
}
```

### Loading assets

```rust
// src/main.rs
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

|Feature|Description|
|-|-|
|deflate|Enables the deflate compression algorithm.|
|brotli|Enables the brotli compression algorithm.|
|writer|Enables the writer feature, to generate a HPAK file from a folder manually with `Writer`.|

## Bevy Compatibility

| bevy          | bevy-histrion-packer |
|---------------|----------------------|
| `0.14.0-rc.2` | `0.4-rc.1`           |
| `0.13`        | `0.2-0.3`            |
| `0.12`        | `0.1`                |

## License

Dual-licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](/LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](/LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
