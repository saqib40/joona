//! The models Joona can download, from `models/catalog.json` at the root of
//! the repository. The file is compiled into the app.

use std::sync::LazyLock;

use joona_protocol::EngineKind;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::library::Manifest;

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    Catalog::parse(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../models/catalog.json"
    )))
    .expect("models/catalog.json is checked by the catalog tests")
});

pub fn catalog() -> &'static Catalog {
    &CATALOG
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub version: u32,
    /// Files the OpenAI checkpoint converter reads.
    pub converter: Vec<CatalogFile>,
    /// Silero VAD, which every engine uses to trim silence.
    pub vad: CatalogFile,
    pub models: Vec<CatalogModel>,
    /// The small chat model for Smart polish on this PC, and the server that runs it.
    #[serde(default)]
    pub local_ai: Option<LocalAiFiles>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAiFiles {
    pub name: String,
    pub license: String,
    pub credit: String,
    /// Memory the loaded model needs.
    pub ram_mb: u32,
    /// A GGUF file.
    pub model: CatalogFile,
    /// llama.cpp's release zip for Windows on x64 CPUs.
    pub server: CatalogFile,
}

impl LocalAiFiles {
    pub fn download_size(&self) -> u64 {
        self.model.size + self.server.size
    }
}

impl Catalog {
    pub fn parse(json: &str) -> anyhow::Result<Self> {
        let catalog: Catalog = serde_json::from_str(json)?;
        catalog.check()?;
        Ok(catalog)
    }

    pub fn get(&self, id: &str) -> Option<&CatalogModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Every download must be verifiable, and every id must be a safe folder name.
    fn check(&self) -> anyhow::Result<()> {
        let mut ids = std::collections::HashSet::new();
        for model in &self.models {
            anyhow::ensure!(ids.insert(&model.id), "{} is listed twice", model.id);
            anyhow::ensure!(
                !model.id.is_empty()
                    && model
                        .id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
                    && !model.id.starts_with('.'),
                "{} isn't a valid model id",
                model.id
            );
            anyhow::ensure!(
                !model.sources.is_empty(),
                "{} has no download sources",
                model.id
            );
            for source in &model.sources {
                anyhow::ensure!(
                    source.sha256(model).is_some_and(is_sha256),
                    "{} has no valid SHA-256 for {}",
                    model.id,
                    source.url
                );
                anyhow::ensure!(
                    source.size(model) > 0,
                    "{} has no size for {}",
                    model.id,
                    source.url
                );
                anyhow::ensure!(
                    (source.format == Format::TarGz) == (model.path == "."),
                    "{}: archives install a folder (path \".\"), other formats one file",
                    model.id
                );
            }
        }
        let local_ai = self.local_ai.iter().flat_map(|l| [&l.model, &l.server]);
        for file in self.converter.iter().chain([&self.vad]).chain(local_ai) {
            anyhow::ensure!(
                is_sha256(&file.sha256),
                "{} has no valid SHA-256",
                file.path
            );
            anyhow::ensure!(
                !file.urls.is_empty(),
                "{} has no download sources",
                file.path
            );
        }
        Ok(())
    }
}

fn is_sha256(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// A single file with mirrors.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Engine {
    Whisper,
    Parakeet,
    Moonshine,
}

impl Engine {
    pub fn kind(self) -> EngineKind {
        match self {
            Engine::Whisper => EngineKind::Whisper,
            Engine::Parakeet => EngineKind::Parakeet,
            Engine::Moonshine => EngineKind::Moonshine,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    pub engine: Engine,
    pub summary: String,
    /// ISO 639-1 codes. Empty means every language the engine knows.
    #[serde(default)]
    pub languages: Vec<String>,
    pub multilingual: bool,
    pub translate: bool,
    #[serde(default)]
    pub variant: Option<String>,
    /// 1 (slowest) to 5 (fastest) on a typical laptop CPU.
    pub speed: u8,
    /// 1 to 5.
    pub accuracy: u8,
    /// Memory the loaded model needs.
    pub ram_mb: u32,
    pub license: String,
    pub credit: String,
    /// The installed file, or `.` for a folder.
    pub path: String,
    /// Installed size.
    pub size: u64,
    /// Hash of the installed file. Sources that need converting or unpacking have their own.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Tried in order until one works.
    pub sources: Vec<Source>,
    /// Files an unpacked archive must contain.
    #[serde(default)]
    pub contains: Vec<String>,
}

impl CatalogModel {
    pub fn speaks(&self, language: &str) -> bool {
        super::library::speaks(
            self.engine.kind(),
            self.multilingual,
            &self.languages,
            language,
        )
    }

    /// What the first source downloads.
    pub fn download_size(&self) -> u64 {
        self.sources.first().map_or(self.size, |s| s.size(self))
    }

    pub fn manifest(&self, source: &Source) -> Manifest {
        Manifest {
            id: self.id.clone(),
            engine: self.engine.kind(),
            path: self.path.clone(),
            variant: self.variant.clone(),
            multilingual: self.multilingual,
            translate: self.translate,
            languages: self.languages.clone(),
            bytes: self.size,
            sha256: if self.path == "." {
                None
            } else {
                self.sha256.clone()
            },
            source: Some(source.url.clone()),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Format {
    /// The installed file itself.
    #[default]
    File,
    /// An OpenAI PyTorch checkpoint, converted to GGML after download.
    OpenaiPt,
    /// A folder packed as `.tar.gz`.
    TarGz,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub url: String,
    #[serde(default)]
    pub format: Format,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
}

impl Source {
    pub fn size(&self, model: &CatalogModel) -> u64 {
        self.size.unwrap_or(model.size)
    }

    pub fn sha256<'a>(&'a self, model: &'a CatalogModel) -> Option<&'a str> {
        self.sha256.as_deref().or(model.sha256.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_catalog_is_valid() {
        let catalog = catalog();
        assert!(catalog.models.len() >= 5);
        for model in &catalog.models {
            assert!(
                (1..=5).contains(&model.speed) && (1..=5).contains(&model.accuracy),
                "{}",
                model.id
            );
            if model.translate {
                assert!(
                    model.multilingual,
                    "{} translates but isn't multilingual",
                    model.id
                );
            }
            for code in &model.languages {
                assert!(
                    crate::languages::find(code).is_some(),
                    "{} lists unknown language {code}",
                    model.id
                );
            }
        }
        assert!(catalog.get("whisper-small").is_some());
        let local_ai = catalog.local_ai.as_ref().expect("the local AI model");
        assert!(local_ai.model.path.ends_with(".gguf"));
        assert!(local_ai.server.path.ends_with(".zip"));
        assert!(local_ai.download_size() > local_ai.model.size);
    }

    #[test]
    fn rejects_sources_without_a_hash() {
        let json = r#"{
            "version": 1, "converter": [],
            "vad": { "path": "v.onnx", "size": 1, "sha256": "0000000000000000000000000000000000000000000000000000000000000000", "urls": ["https://x"] },
            "models": [{
                "id": "m", "name": "M", "engine": "whisper", "summary": "", "multilingual": true, "translate": true,
                "speed": 3, "accuracy": 3, "ramMb": 1, "license": "MIT", "credit": "", "path": "m.bin", "size": 1,
                "sources": [{ "url": "https://example.com/m.bin" }]
            }]
        }"#;
        assert!(
            Catalog::parse(json)
                .unwrap_err()
                .to_string()
                .contains("SHA-256")
        );
    }
}
