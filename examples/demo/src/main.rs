use bevy::prelude::*;
use bevy_histrion_packer::HistrionPackerPlugin;

fn main() {
    let mut app = App::new();

    app.add_plugins((
        HistrionPackerPlugin {
            source: "assets.hpak".into(),
            mode: bevy_histrion_packer::HistrionPackerMode::ReplaceDefaultProcessed,
        },
        DefaultPlugins,
    ))
    .insert_resource(AmbientLight {
        brightness: 150.0,
        ..default()
    })
    .add_systems(Startup, setup);

    app.run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(0.0, 0.0, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..Default::default()
    });

    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            color: Color::rgb(1.0, 1.0, 1.0),
            ..Default::default()
        },
        ..Default::default()
    });

    commands.spawn(SceneBundle {
        scene: asset_server.load("Avocado.gltf#Scene0"),
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..Default::default()
    });
}
