use bevy::{
    app::ScheduleRunnerPlugin, asset::processor::AssetProcessor, prelude::*,
    render::render_resource::ShaderLoader,
};
use bevy_histrion_packer as bhp;
use std::{env, path::PathBuf, time::Duration};

use text_asset::{TextAsset, TextAssetLoader};

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // process assets, we can add more assets pre-processing steps here
    App::new()
        .add_plugins(
            HeadlessPlugins
                .set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(33)))
                .set(bevy::asset::AssetPlugin {
                    mode: AssetMode::Processed,
                    ..Default::default()
                }),
        )
        .init_asset::<TextAsset>()
        .init_asset_loader::<TextAssetLoader>()
        .init_asset::<Shader>()
        .init_asset_loader::<ShaderLoader>()
        .init_asset::<Mesh>()
        .init_asset_loader::<bevy::render::render_resource::ShaderLoader>()
        .add_plugins(bevy::render::texture::ImagePlugin::default())
        .add_plugins(bevy::pbr::PbrPlugin::default())
        .add_plugins(bevy::gltf::GltfPlugin::default())
        .add_systems(
            Update,
            |asset_processor: Res<AssetProcessor>, mut exit_tx: EventWriter<AppExit>| {
                if bevy::tasks::block_on(asset_processor.get_state())
                    == bevy::asset::processor::ProcessorState::Finished
                {
                    exit_tx.send(AppExit::Success);
                }
            },
        )
        .run();

    // pack assets
    bhp::writer::pack_assets_folder(
        // processed assets directory
        crate_dir.join("imported_assets/Default"),
        // output file
        crate_dir.join("assets.hpak"),
        // do not compress metadata
        bhp::CompressionMethod::None,
        // use deflate compression method as default for data
        bhp::CompressionMethod::Deflate,
        // use default extensions compression method
        bhp::writer::default_extensions_compression_method(),
        // don't ignore missing meta
        false,
        // don't add padding
        false,
    )
    .unwrap();
}
