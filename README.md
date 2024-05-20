# Bevy Histrion Packer

![MIT or Apache 2.0](https://img.shields.io/badge/License-MIT%20or%20Apache%202.0-blue.svg)
[![Docs](https://docs.rs/bevy-histrion-packer/badge.svg)](https://docs.rs/bevy-histrion-packer)
[![Crate](https://img.shields.io/crates/v/bevy-histrion-packer.svg)](https://crates.io/crates/bevy-histrion-packer)

A Bevy plugin to allows to efficiently pack all game assets, such as textures, audio files, and other resources, into a single common PAK like file format.

## Usage

### Packing assets

By default, the `Writer` compress metadata with Brotli and uses Deflate for data except for theses specific loaders/extensions:

|Extension|Compression Method|
|-|-|
|`ImageFormat::OpenExr`/.exr|**None**|
|`ImageFormat::Basis`/.basis|**None**|
|`ImageFormat::Ktx2`/.ktx2|**None**|
|.qoi|**None**|
|.ogg|**None**|
|.oga|**None**|
|.spx|**None**|
|.mp3|**None**|
|.qoa|**None**|
|.ron|**Brotli**|
|.json|**Brotli**|
|.y(a)ml|**Brotli**|
|.toml|**Brotli**|
|.txt|**Brotli**|
|.ini|**Brotli**|
|.cfg|**Brotli**|
|.gltf|**Brotli**|
|.wgsl|**Brotli**|
|.glsl|**Brotli**|
|.hlsl|**Brotli**|
|.vert|**Brotli**|
|.frag|**Brotli**|
|.vs|**Brotli**|
|.fs|**Brotli**|
|.lua|**Brotli**|
|.js|**Brotli**|
|.html|**Brotli**|
|.css|**Brotli**|
|.xml|**Brotli**|
|.mtlx|**Brotli**|
|.usda|**Brotli**|

These exceptions are in place because Basis Universal and KTX2 formats can be decompressed directly on the GPU, avoiding unnecessary CPU resource usage and potential slowdowns during asset loading.

Same thing for OGG, OGA, SPX and MP3 which are highly compressed audio formats.

Brotli is used for text based formats like metadata (ron), json, yaml, wgsl, glsl etc. because it gives better ratio in such cases.

You can personalize the compression algorithm used with `Writer::meta_compression` and `Writer::data_compression_fn`.


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
    let mut destination = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("assets.hpak")?;

    let mut writer = bevy_histrion_packer::WriterBuilder::new(&mut destination).build()?;
    pack_assets_folder(&source, &mut writer)?;
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
|gzip|Enables the gzip compression algorithm.|
|zlib|Enables the zlib compression algorithm.|
|brotli|Enables the brotli compression algorithm.|
|packing|Enables the packing feature, to generate a HPAK file from a folder (`pack_assets_folder`) or manually with `Writer`.|

## Bevy Compatibility

| bevy   | bevy-histrion-packer |
|--------|----------------------|
| `0.13` | `0.2-0.3`            |
| `0.12` | `0.1`                |

## License

Dual-licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](/LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](/LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
