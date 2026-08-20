use crate::error::{AppError, Result};
use crate::paths;
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub filename: &'static str,
    pub url: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
    pub note: &'static str,
}

const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub fn catalog() -> Vec<ModelSpec> {
    vec![
        ModelSpec {
            id: "base-q5_1",
            name: "Base (q5_1)",
            filename: "ggml-base-q5_1.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin",
            size_bytes: 59_707_625,
            sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
            note: "Lightest. Used for the realtime preview and as the emergency fallback.",
        },
        ModelSpec {
            id: "small-q5_1",
            name: "Small (q5_1)",
            filename: "ggml-small-q5_1.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
            size_bytes: 190_085_487,
            sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
            note: "Balanced. Used for the realtime preview; reliable on mixed EN/ID speech.",
        },
        ModelSpec {
            id: "large-v3-turbo-q5_0",
            name: "Large v3 Turbo (q5_0)",
            filename: "ggml-large-v3-turbo-q5_0.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
            size_bytes: 574_041_195,
            sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
            note: "Best accuracy on mixed-language speech. Needs more memory.",
        },
    ]
}

/// One ready-made choice: a main model together with its preview model.
///
/// Pairing them is not a reasonable thing to ask a user to reason about. The app always runs
/// two models — a large one for final text, a small one for the realtime preview — and which
/// pairings work came out of measurement rather than taste. See BENCHMARK.md: `small` as the
/// main model silently dropped English sentences from mixed audio, while `base` as the
/// preview model once emitted Russian.
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub id: &'static str,
    pub name: &'static str,
    pub model_id: &'static str,
    pub partial_model_id: &'static str,
    pub note: &'static str,
    pub recommended: bool,
}

pub fn profiles() -> Vec<Profile> {
    vec![
        Profile {
            id: "best",
            name: "Best accuracy",
            model_id: "large-v3-turbo-q5_0",
            partial_model_id: "small-q5_1",
            note: "Handles meetings that switch language mid-sentence. Needs a capable machine.",
            recommended: true,
        },
        Profile {
            id: "balanced",
            name: "Balanced",
            model_id: "small-q5_1",
            partial_model_id: "base-q5_1",
            note: "Lighter on memory, but can silently drop a sentence when the language switches.",
            recommended: false,
        },
        Profile {
            id: "light",
            name: "Lightest",
            model_id: "base-q5_1",
            partial_model_id: "base-q5_1",
            note: "For low-powered machines. Noticeably more mistakes, especially on names.",
            recommended: false,
        },
    ]
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileStatus {
    #[serde(flatten)]
    pub profile: Profile,
    /// How much still needs downloading, with already-present files subtracted.
    pub download_bytes: u64,
    pub total_bytes: u64,
    pub ready: bool,
}

pub fn profile_status() -> Result<Vec<ProfileStatus>> {
    let mut out = Vec::new();
    for p in profiles() {
        // The main and preview model can be the same one; do not count it twice.
        let mut ids = vec![p.model_id];
        if p.partial_model_id != p.model_id {
            ids.push(p.partial_model_id);
        }
        let mut total = 0u64;
        let mut missing = 0u64;
        let mut ready = true;
        for id in &ids {
            let spec = spec(id)?;
            total += spec.size_bytes;
            if !is_downloaded(id) {
                missing += spec.size_bytes;
                ready = false;
            }
        }
        out.push(ProfileStatus {
            profile: p,
            download_bytes: missing,
            total_bytes: total,
            ready,
        });
    }
    Ok(out)
}

pub fn spec(id: &str) -> Result<ModelSpec> {
    catalog()
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| AppError::NotFound(format!("model '{id}'")))
}

pub fn model_path(id: &str) -> Result<PathBuf> {
    Ok(paths::models_dir()?.join(spec(id)?.filename))
}

pub fn is_downloaded(id: &str) -> bool {
    match (model_path(id), spec(id)) {
        (Ok(p), Ok(s)) => std::fs::metadata(&p)
            .map(|m| m.len() == s.size_bytes)
            .unwrap_or(false),
        _ => false,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    #[serde(flatten)]
    pub spec: ModelSpec,
    pub downloaded: bool,
    pub path: String,
}

pub fn status_all() -> Result<Vec<ModelStatus>> {
    let dir = paths::models_dir()?;
    Ok(catalog()
        .into_iter()
        .map(|s| ModelStatus {
            downloaded: dir
                .join(s.filename)
                .metadata()
                .map(|m| m.len() == s.size_bytes)
                .unwrap_or(false),
            path: dir.join(s.filename).to_string_lossy().to_string(),
            spec: s,
        })
        .collect())
}

/// Downloads a model with resume support and SHA-256 verification. The file is only moved
/// to its final name once the hash matches, so a half-finished download can never look like
/// a usable model.
pub async fn download<F>(id: &str, mut on_progress: F) -> Result<PathBuf>
where
    F: FnMut(u64, u64) + Send,
{
    let spec = spec(id)?;
    let final_path = paths::models_dir()?.join(spec.filename);
    if final_path.metadata().map(|m| m.len()).unwrap_or(0) == spec.size_bytes {
        on_progress(spec.size_bytes, spec.size_bytes);
        return Ok(final_path);
    }
    let part_path = final_path.with_extension("part");
    let mut have = part_path.metadata().map(|m| m.len()).unwrap_or(0);
    if have > spec.size_bytes {
        let _ = std::fs::remove_file(&part_path);
        have = 0;
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60 * 60))
        .build()?;
    let mut req = client.get(spec.url);
    if have > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Http(format!(
            "download failed ({}) for {}",
            resp.status(),
            spec.filename
        )));
    }
    // The server ignored our Range header, so start from zero again.
    let resuming = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if !resuming {
        have = 0;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resuming)
        .truncate(!resuming)
        .open(&part_path)?;

    let mut downloaded = have;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, spec.size_bytes);
    }
    file.flush()?;
    drop(file);

    let actual = sha256_file(&part_path)?;
    if actual != spec.sha256 {
        let _ = std::fs::remove_file(&part_path);
        return Err(AppError::Model(format!(
            "model file is corrupt (sha256 {actual} != {}), download discarded",
            spec.sha256
        )));
    }
    std::fs::rename(&part_path, &final_path)?;
    Ok(final_path)
}

fn sha256_file(path: &std::path::Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn delete(id: &str) -> Result<()> {
    let p = model_path(id)?;
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

#[allow(dead_code)]
pub fn base_url() -> &'static str {
    BASE_URL
}
