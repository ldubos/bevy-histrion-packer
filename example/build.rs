use bevy::prelude::*;
use bevy_histrion_packer as bhp;
use std::{env, path::PathBuf};

use text_asset::{TextAsset, TextAssetLoader};

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // process assets, we can add more assets pre-processing steps here
    App::new()
        .add_plugins(
            DefaultPlugins
                .build()
                .set(bevy::window::WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..default()
                })
                .set(bevy::asset::AssetPlugin {
                    mode: AssetMode::Processed,
                    ..default()
                }),
        )
        .add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(
            std::time::Duration::from_secs_f64(1.0 / 30.0),
        ))
        // Custom Assets
        .init_asset::<TextAsset>()
        .init_asset_loader::<TextAssetLoader>()
        // Process Assets
        .add_systems(
            Update,
            |asset_processor: Res<bevy::asset::processor::AssetProcessor>,
             mut exit_tx: EventWriter<AppExit>| {
                if bevy::tasks::block_on(asset_processor.get_state())
                    == bevy::asset::processor::ProcessorState::Finished
                {
                    exit_tx.write(AppExit::Success);
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
        // use zlib compression method as default for data
        bhp::CompressionMethod::Zlib,
        // use default extensions compression method
        bhp::writer::default_extensions_compression_method(),
        // don't ignore missing meta
        false,
        // don't apply any alignment
        // to align to 4096 bytes you could use:
        // Some(4096),
        None,
    )
    .unwrap();
}
