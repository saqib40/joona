//! Models installed on this PC, one folder each under the models directory.
//!
//! A folder installed by Joona has an `installed.json` manifest. Folders
//! holding a Whisper `ggml-*.bin` file without a manifest, such as a model
//! the user copied in by hand, are recognised too.

use std::path::{Path, PathBuf};

use joona_protocol::{EngineKind, ModelSpec};
use serde::{Deserialize, Serialize};

pub const MANIFEST: &str = "installed.json";
pub const VAD_DIR: &str = "silero-vad";
pub const VAD_FILE: &str = "silero_vad_v4.onnx";

/// What the installer writes next to a model's files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub engine: EngineKind,
    /// The model file, or `.` for engines that load a whole folder.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    /// Understands languages other than English.
    pub multilingual: bool,
    /// Can translate speech into English.
    pub translate: bool,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub bytes: u64,
    /// Hash of the model file when it was installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Where it was downloaded from, or the file it was imported from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledModel {
    pub manifest: Manifest,
    pub dir: PathBuf,
}

impl InstalledModel {
    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    pub fn spec(&self) -> ModelSpec {
        let path = if self.manifest.path == "." {
            self.dir.clone()
        } else {
            self.dir.join(&self.manifest.path)
        };
        ModelSpec {
            id: self.manifest.id.clone(),
            engine: self.manifest.engine,
            path,
            variant: self.manifest.variant.clone(),
        }
    }

    /// Whether this model can transcribe `language` (an ISO 639-1 code).
    pub fn speaks(&self, language: &str) -> bool {
        let m = &self.manifest;
        speaks(m.engine, m.multilingual, &m.languages, language)
    }
}

/// `languages` lists what a model knows; empty means every language its engine knows.
pub fn speaks(
    engine: EngineKind,
    multilingual: bool,
    languages: &[String],
    language: &str,
) -> bool {
    if !languages.is_empty() {
        return languages.iter().any(|l| l == language);
    }
    if !multilingual {
        return language == "en";
    }
    match engine {
        EngineKind::Whisper => crate::languages::find(language).is_none_or(|l| l.whisper),
        _ => true,
    }
}

#[derive(Debug, Clone)]
pub struct Library {
    root: PathBuf,
}

impl Library {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn installed(&self) -> Vec<InstalledModel> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut models: Vec<InstalledModel> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
            .filter_map(|e| read_model(&e.path()))
            .collect();
        models.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        models
    }

    pub fn get(&self, id: &str) -> Option<InstalledModel> {
        if !is_folder_name(id) {
            return None;
        }
        read_model(&self.root.join(id)).filter(|m| m.manifest.id == id)
    }

    /// Removes an installed model's folder.
    pub fn delete(&self, id: &str) -> anyhow::Result<()> {
        anyhow::ensure!(is_folder_name(id) && id != VAD_DIR, "{id} isn't a model");
        let dir = self.root.join(id);
        anyhow::ensure!(dir.is_dir(), "{id} isn't installed");
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    /// Partial downloads and other working files, which never count as models.
    pub fn work_dir(&self, name: &str) -> PathBuf {
        self.root.join(format!(".{name}"))
    }

    pub fn vad(&self) -> Option<PathBuf> {
        let path = self.root.join(VAD_DIR).join(VAD_FILE);
        path.is_file().then_some(path)
    }
}

/// One plain path component, so an id can't point outside the models folder.
fn is_folder_name(id: &str) -> bool {
    !id.is_empty() && !id.starts_with('.') && !id.contains(['/', '\\', ':'])
}

fn read_model(dir: &Path) -> Option<InstalledModel> {
    let name = dir.file_name()?.to_string_lossy().into_owned();
    if name == VAD_DIR {
        return None;
    }
    if let Ok(text) = std::fs::read_to_string(dir.join(MANIFEST)) {
        return match serde_json::from_str::<Manifest>(&text) {
            Ok(manifest) => Some(InstalledModel {
                manifest,
                dir: dir.to_owned(),
            }),
            Err(e) => {
                tracing::warn!(dir = %dir.display(), "ignoring a model with a broken manifest: {e}");
                None
            }
        };
    }
    whisper_without_manifest(dir, name)
}

/// A folder with a single `ggml-<size>.bin` Whisper file.
fn whisper_without_manifest(dir: &Path, id: String) -> Option<InstalledModel> {
    let file = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .find_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (name.starts_with("ggml-") && name.ends_with(".bin") && !name.contains(".part"))
                .then_some(name)
        })?;
    let size = file.trim_start_matches("ggml-").trim_end_matches(".bin");
    let english_only = size.contains(".en");
    let turbo = size.contains("turbo");
    let bytes = std::fs::metadata(dir.join(&file))
        .map(|m| m.len())
        .unwrap_or(0);
    Some(InstalledModel {
        manifest: Manifest {
            id,
            engine: EngineKind::Whisper,
            path: file,
            variant: None,
            multilingual: !english_only,
            // large-v3-turbo was only trained to transcribe.
            translate: !english_only && !turbo,
            languages: Vec::new(),
            bytes,
            sha256: None,
            source: None,
        },
        dir: dir.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("joona-library-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_manifests_and_bare_whisper_files() {
        let root = temp_root("scan");
        std::fs::create_dir_all(root.join("whisper-tiny")).unwrap();
        std::fs::write(root.join("whisper-tiny").join("ggml-tiny.bin"), b"x").unwrap();
        std::fs::create_dir_all(root.join("whisper-base.en")).unwrap();
        std::fs::write(root.join("whisper-base.en").join("ggml-base.en.bin"), b"x").unwrap();
        std::fs::create_dir_all(root.join("parakeet-tdt-0.6b-v3")).unwrap();
        let manifest = Manifest {
            id: "parakeet-tdt-0.6b-v3".into(),
            engine: EngineKind::Parakeet,
            path: ".".into(),
            variant: Some("int8".into()),
            multilingual: true,
            translate: false,
            languages: vec!["en".into(), "de".into()],
            bytes: 640,
            sha256: None,
            source: None,
        };
        std::fs::write(
            root.join("parakeet-tdt-0.6b-v3").join(MANIFEST),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(root.join(VAD_DIR)).unwrap();
        std::fs::write(root.join(VAD_DIR).join(VAD_FILE), b"x").unwrap();
        std::fs::create_dir_all(root.join("empty")).unwrap();
        std::fs::create_dir_all(root.join(".downloads").join("whisper-small")).unwrap();
        std::fs::write(root.join(".downloads").join("ggml-small.bin"), b"x").unwrap();

        let library = Library::new(&root);
        let ids: Vec<_> = library
            .installed()
            .iter()
            .map(|m| m.id().to_owned())
            .collect();
        assert_eq!(
            ids,
            ["parakeet-tdt-0.6b-v3", "whisper-base.en", "whisper-tiny"]
        );

        let tiny = library.get("whisper-tiny").unwrap();
        assert!(tiny.manifest.multilingual && tiny.manifest.translate);
        assert!(tiny.speaks("hi"));
        assert!(!tiny.speaks("kok"), "Whisper doesn't know Konkani");
        assert_eq!(
            tiny.spec().path,
            root.join("whisper-tiny").join("ggml-tiny.bin")
        );

        let base_en = library.get("whisper-base.en").unwrap();
        assert!(!base_en.manifest.multilingual);
        assert!(base_en.speaks("en") && !base_en.speaks("hi"));

        let parakeet = library.get("parakeet-tdt-0.6b-v3").unwrap();
        assert_eq!(parakeet.spec().path, root.join("parakeet-tdt-0.6b-v3"));
        assert!(parakeet.speaks("de") && !parakeet.speaks("hi"));

        assert!(library.vad().is_some());
        assert!(library.get("missing").is_none());
        assert!(library.get("../whisper-tiny").is_none());

        assert!(library.delete(VAD_DIR).is_err());
        assert!(library.delete("..").is_err());
        library.delete("whisper-tiny").unwrap();
        assert!(library.get("whisper-tiny").is_none());
    }
}
