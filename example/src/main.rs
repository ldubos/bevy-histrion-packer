use bevy::{app::ScheduleRunnerPlugin, prelude::*};

use bevy_histrion_packer::HistrionPackerPlugin;
use text_asset::{TextAsset, TextAssetLoader};

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .build()
                .set(ScheduleRunnerPlugin::run_loop(
                    std::time::Duration::from_secs_f64(1.0 / 30.0),
                ))
                .add_before::<AssetPlugin>(HistrionPackerPlugin {
                    source: env!("CARGO_MANIFEST_DIR").to_string() + "/assets.hpak",
                    mode: bevy_histrion_packer::HistrionPackerMode::ReplaceDefaultProcessed,
                })
                .set(AssetPlugin {
                    mode: AssetMode::Processed,
                    ..default()
                }),
        )
        .init_asset::<TextAsset>()
        .init_asset_loader::<TextAssetLoader>()
        .init_resource::<State>()
        .add_systems(Startup, setup)
        .add_systems(Update, print_on_load)
        .run();
}

#[derive(Default, Resource)]
struct State {
    a: Handle<TextAsset>,
    b: Handle<TextAsset>,
}

fn setup(mut state: ResMut<State>, asset_server: Res<AssetServer>) {
    state.a = asset_server.load("asset.text");
    state.b = asset_server.load("sub/图.text");
}

fn print_on_load(
    state: Res<State>,
    text_assets: Res<Assets<TextAsset>>,
    mut exit_tx: MessageWriter<AppExit>,
) {
    let a = match text_assets.get(&state.a) {
        Some(a) => a,
        None => return,
    };

    let b = match text_assets.get(&state.b) {
        Some(b) => b,
        None => return,
    };

    info!("TextAsset A: {}", a);
    info!("TextAsset B: {}", b);

    exit_tx.write(AppExit::Success);
}
