use bevy::asset::{
    meta::{AssetAction, AssetMeta},
    processor::Process,
    AssetLoader,
};
use serde::{Deserialize, Serialize};

use crate::errors::Error;

pub fn get_meta_loader_type_path(meta: &[u8]) -> Result<String, Error> {
    let meta = AssetMeta::<DummyLoader, DummyProcessor<DummyLoader>>::deserialize(meta)
        .map_err(|e| Error::InvalidAssetMeta(e.to_string()))?;

    let loader_type_path = if let AssetAction::Load { loader, .. } = meta.asset {
        loader
    } else {
        return Err(Error::InvalidAssetMeta("Invalid asset action".to_string()));
    };

    Ok(loader_type_path)
}

pub fn get_meta_loader_settings<L: AssetLoader>(meta: &[u8]) -> Result<L::Settings, Error> {
    let meta = AssetMeta::<L, DummyProcessor<L>>::deserialize(meta)
        .map_err(|e| Error::InvalidAssetMeta(e.to_string()))?;

    let settings = if let AssetAction::Load { settings, .. } = meta.asset {
        settings
    } else {
        return Err(Error::InvalidAssetMeta("Invalid asset action".to_string()));
    };

    Ok(settings)
}

#[cfg(feature = "writer")]
pub use writer::*;

#[cfg(feature = "writer")]
mod writer {
    use std::{
        fs::{File, OpenOptions},
        path::{Path, PathBuf},
        time::Duration,
    };

    use bevy::asset::processor::AssetProcessor;

    use crate::{utils::get_meta_loader_settings, CompressionAlgorithm, WriterBuilder};

    use super::get_meta_loader_type_path;

    fn get_meta_path(path: &Path) -> PathBuf {
        let mut meta_path = path.to_path_buf();
        let mut extension = path
            .extension()
            .expect("asset paths must have extensions")
            .to_os_string();
        extension.push(".meta");
        meta_path.set_extension(extension);
        meta_path
    }

    /// This function will create and return an headless bevy app with the `processed` asset mode.
    ///
    /// By default the app will have the following plugins:
    /// - `bevy::asset::AssetPlugin` with the `processed` asset mode.
    /// - `bevy::render::render_resource::ShaderLoader`
    /// - `bevy::render::texture::ImagePlugin`
    /// - `bevy::pbr::PbrPlugin`
    /// - `bevy::gltf::GltfPlugin`
    ///
    /// # Example
    ///
    /// ```no_run
    /// use bevy_histrion_packer::utils::get_processing_app;
    ///
    /// let app = get_processing_app(DefaultPlugins).unwrap();
    ///
    /// // app.add_plugins(my_extra_assets_plugins);
    ///
    /// app.run();
    /// ```
    pub fn get_processing_app() -> Result<bevy::app::App, Box<dyn std::error::Error>> {
        use bevy::app::ScheduleRunnerPlugin;
        use bevy::prelude::*;

        let mut app = App::new();

        app.add_plugins(
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(33))),
        )
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
                        exit_tx.send(AppExit::Success);
                    }
                    _ => {}
                }
            },
        );

        Ok(app)
    }

    /// Read the `source` folder recursively and pack all it's assets into a HPAK file.
    ///
    /// ```no_run
    /// use std::fs::{File, OpenOptions};
    /// use std::path::Path;
    /// use bevy_histrion_packer::{WriterBuilder, CompressionAlgorithm, pack_assets_folder};
    ///
    /// let source = Path::new("imported_assets/Default");
    /// let destination = Path::new("assets.hpak");
    ///
    /// pack_assets_folder(&source, &destination).unwrap();
    /// ```
    pub fn pack_assets_folder(
        source: &Path,
        destination: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut writer = WriterBuilder::new(
            OpenOptions::new()
                .write(true)
                .truncate(true)
                .create(true)
                .open(destination)?,
        )
        .build()?;

        for entry in walkdir::WalkDir::new(source)
            .into_iter()
            .filter_map(Result::ok)
        {
            let data_path = entry.path();
            let extension = data_path.extension().unwrap_or_default().to_os_string();

            if data_path.is_file() && !extension.eq("meta") {
                let meta_path = get_meta_path(data_path);

                if !meta_path.exists() {
                    continue;
                }

                let mut meta_file = File::open(meta_path)?;
                let mut data_file = File::open(data_path)?;

                let mut meta_buffer = Vec::new();
                std::io::Read::read_to_end(&mut meta_file, &mut meta_buffer)?;

                let compression_method =
                    if let Ok(loader_type) = get_meta_loader_type_path(&meta_buffer) {
                        match loader_type.as_str() {
                            "bevy_render::render_resource::shader::ShaderLoader" => {
                                CompressionAlgorithm::Deflate
                            }
                            "bevy_render::texture::image_loader::ImageLoader" => {
                                handle_image_loader(&meta_buffer)
                            }
                            _ => handle_extensions(data_path.into()),
                        }
                    } else {
                        handle_extensions(data_path.into())
                    };

                writer.add_entry(
                    data_path.strip_prefix(source)?,
                    &mut meta_file,
                    &mut data_file,
                    compression_method,
                )?;
            }
        }

        writer.finish()?;
        Ok(())
    }

    #[inline(always)]
    fn handle_image_loader(meta: &[u8]) -> CompressionAlgorithm {
        use bevy::render::texture::{ImageFormat, ImageFormatSetting, ImageLoader};

        match get_meta_loader_settings::<ImageLoader>(meta) {
            Ok(settings) => match settings.format {
                // Don't compress images that already greatly benefits from compression and/or can
                // be decompressed directly by the GPU to avoid unnecessary CPU
                // overhead during asset loading.
                ImageFormatSetting::Format(
                    ImageFormat::OpenExr | ImageFormat::Basis | ImageFormat::Ktx2,
                ) => CompressionAlgorithm::None,
                _ => CompressionAlgorithm::Deflate,
            },
            _ => CompressionAlgorithm::Deflate,
        }
    }

    #[inline(always)]
    fn handle_extensions(path: PathBuf) -> CompressionAlgorithm {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str().map(|s| s.to_ascii_lowercase()))
            .unwrap_or_default();

        match extension.as_str() {
            "ogg" | "oga" | "spx" | "mp3" | "ktx2" | "exr" | "basis" | "qoi" | "qoa" => {
                CompressionAlgorithm::None
            }
            _ => CompressionAlgorithm::Deflate,
        }
    }
}

// hack to deserialize any AssetLoader
#[derive(Default, Deserialize, Serialize)]
struct DummySettings {
    #[serde(default)]
    _dummy: bool,
}

struct DummyLoader;

impl AssetLoader for DummyLoader {
    type Asset = ();

    type Settings = DummySettings;

    type Error = std::io::Error;

    async fn load<'a>(
        &'a self,
        _reader: &'a mut bevy::asset::io::Reader<'_>,
        _settings: &'a Self::Settings,
        _load_context: &'a mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        Ok(())
    }
}
struct DummyProcessor<L: AssetLoader>(std::marker::PhantomData<L>);

impl<L: AssetLoader> Process for DummyProcessor<L> {
    type Settings = ();

    type OutputLoader = L;

    async fn process<'a>(
        &'a self,
        _context: &'a mut bevy::asset::processor::ProcessContext<'_>,
        _meta: AssetMeta<(), Self>,
        _writer: &'a mut bevy::asset::io::Writer,
    ) -> Result<<Self::OutputLoader as AssetLoader>::Settings, bevy::asset::processor::ProcessError>
    {
        Ok(<Self::OutputLoader as AssetLoader>::Settings::default())
    }
}
