use bevy::prelude::*;


use bevy_histrion_packer::HistrionPackerPlugin;
use text_asset::{TextAsset, TextAssetLoader};

fn main() {
    App::new()
        .add_plugins(
            HeadlessPlugins
                .build()
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

#[derive(Resource, Default)]
struct State {
    a: Handle<TextAsset>,
    b: Handle<TextAsset>,
    printed: bool,
}

fn setup(mut state: ResMut<State>, asset_server: Res<AssetServer>) {
    state.a = asset_server.load("asset.text");
    state.b = asset_server.load("sub/图.text");
}

fn print_on_load(mut state: ResMut<State>, text_assets: Res<Assets<TextAsset>>) {
    let a = text_assets.get(&state.a);
    let b = text_assets.get(&state.b);

    if state.printed {
        return;
    }

    if a.is_none() {
        return;
    }

    if b.is_none() {
        return;
    }

    info!("TextAsset A: {}", a.unwrap());
    info!("TextAsset B: {}", b.unwrap());

    state.printed = true;
}
