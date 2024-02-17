use std::path::PathBuf;

use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    asset::processor::{AssetProcessor, ProcessorState},
    prelude::*,
    tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task},
};
use bevy_histrion_packer::pack_assets_folder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Default, Resource)]
    struct ProcessorStateTask(pub Option<Task<ProcessorState>>);

    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    visible: false,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .set(AssetPlugin {
                mode: AssetMode::Processed,
                ..Default::default()
            }),
    )
    .add_plugins(ScheduleRunnerPlugin::run_once())
    .init_resource::<ProcessorStateTask>()
    // add your custom assets plugins here
    .add_systems(
        Update,
        |processor: Res<AssetProcessor>,
         mut pst: ResMut<ProcessorStateTask>,
         mut exit_tx: EventWriter<AppExit>| {
            match &mut pst.0 {
                Some(task) => {
                    let state = block_on(future::poll_once(task));

                    if let Some(state) = state {
                        if state == ProcessorState::Finished {
                            // if the processor is finished, we can exit the app
                            exit_tx.send(AppExit);
                        } else {
                            // if the processor is not finished, we can reset the task and restart
                            pst.0 = None;
                        }
                    }
                }
                None => {
                    let thread_pool = AsyncComputeTaskPool::get();
                    let processor = processor.clone();
                    pst.0 = Some(thread_pool.spawn(async move { processor.get_state().await }));
                }
            }
        },
    );

    app.run();

    // pack the assets folder
    pack_assets_folder(
        &PathBuf::from("imported_assets/Default"),
        &PathBuf::from("assets.hpak"),
        false,
    )?;

    Ok(())
}
