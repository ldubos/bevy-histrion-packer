use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};

#[derive(Debug, Asset, TypePath)]
pub struct TextAsset(String);

impl std::fmt::Display for TextAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Default)]
pub struct TextAssetLoader;

impl AssetLoader for TextAssetLoader {
    type Asset = TextAsset;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        info!("loading text asset...");

        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(TextAsset(String::from_utf8_lossy(&bytes).to_string()))
    }

    fn extensions(&self) -> &[&str] {
        &["text"]
    }
}
