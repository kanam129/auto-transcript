use super::{chunker, prompts, Summary, SummaryRequest, Summarizer};
use crate::error::{AppError, Result};
use crate::settings::LlmSettings;
use serde::Deserialize;
use std::time::Duration;

const KEYRING_SERVICE: &str = "auto-transcript";
const KEYRING_ACCOUNT: &str = "llm_api_key";
const MAX_RETRIES: u32 = 3;

pub fn store_api_key(key: &str) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| AppError::Config(format!("keychain: {e}")))?;
    entry
        .set_password(key)
        .map_err(|e| AppError::Config(format!("could not store the key: {e}")))
}

pub fn has_api_key() -> bool {
    read_api_key().map(|k| !k.is_empty()).unwrap_or(false)
}

pub fn clear_api_key() -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| AppError::Config(format!("keychain: {e}")))?;
    match entry.delete_credential() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Config(format!("could not delete the key: {e}"))),
    }
}

fn read_api_key() -> Result<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| AppError::Config(format!("keychain: {e}")))?;
    entry
        .get_password()
        .map_err(|_| AppError::Config("no API key has been stored in the keychain".into()))
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}
#[derive(Deserialize)]
struct Choice {
    message: Message,
}
#[derive(Deserialize)]
struct Message {
    content: String,
}

/// A client for any endpoint that speaks OpenAI's `chat/completions` protocol: OpenAI,
/// OpenRouter, Groq, LM Studio, Ollama, or an in-house gateway.
pub struct OpenAiCompatible {
    cfg: LlmSettings,
    client: reqwest::Client,
}

impl OpenAiCompatible {
    pub fn new(cfg: LlmSettings) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build()?;
        Ok(Self { cfg, client })
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.cfg.base_url.trim_end_matches('/'))
    }

    async fn complete_json(&self, user: &str) -> Result<String> {
        let key = read_api_key()?;
        let body = serde_json::json!({
            "model": self.cfg.model,
            "temperature": 0.2,
            "response_format": {"type": "json_object"},
            "messages": [
                {"role": "system", "content": prompts::SYSTEM},
                {"role": "user", "content": user}
            ]
        });

        let mut delay = Duration::from_secs(2);
        let mut last_err = String::new();
        for attempt in 1..=MAX_RETRIES {
            let resp = self
                .client
                .post(self.endpoint())
                .bearer_auth(&key)
                .json(&body)
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    let parsed: ChatResponse = r.json().await?;
                    return parsed
                        .choices
                        .into_iter()
                        .next()
                        .map(|c| c.message.content)
                        .ok_or_else(|| AppError::Http("the model returned an empty response".into()));
                }
                Ok(r) => {
                    let status = r.status();
                    let text = r.text().await.unwrap_or_default();
                    last_err = format!("{status}: {}", text.chars().take(300).collect::<String>());
                    // Only rate limits and server errors are worth retrying.
                    if !(status.as_u16() == 429 || status.is_server_error()) {
                        return Err(AppError::Http(last_err));
                    }
                }
                Err(e) => last_err = e.to_string(),
            }
            if attempt < MAX_RETRIES {
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
        }
        Err(AppError::Http(format!(
            "failed after {MAX_RETRIES} attempts - {last_err}"
        )))
    }
}

/// Models sometimes wrap JSON in code fences anyway, however clearly you ask them not to.
fn extract_json(raw: &str) -> &str {
    let t = raw.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t);
    let t = t.trim();
    match (t.find('{'), t.rfind('}')) {
        (Some(a), Some(b)) if b > a => &t[a..=b],
        _ => t,
    }
}

impl Summarizer for OpenAiCompatible {
    #[allow(clippy::manual_async_fn)]
    fn summarize<F>(
        &self,
        req: SummaryRequest,
        mut on_progress: F,
    ) -> impl std::future::Future<Output = Result<Summary>> + Send
    where
        F: FnMut(&str, usize, usize) + Send,
    {
        async move {
        let chunks = chunker::split(&req.lines, chunker::CHUNK_CHARS);
        if chunks.is_empty() {
            return Err(AppError::Config("the transcript is empty, there is nothing to summarize".into()));
        }

        let total = chunks.len();
        // A short meeting fits in one call; no need for map-reduce.
        if total == 1 {
            on_progress("chunk", 0, 1);
            let body = chunker::render(&chunks[0]);
            let raw = self
                .complete_json(&prompts::map_prompt(&req.title, 1, 1, &body))
                .await?;
            on_progress("chunk", 1, 1);
            return serde_json::from_str(extract_json(&raw))
                .map_err(|e| AppError::Config(format!("the model did not return the expected JSON: {e}")));
        }

        let mut partials = Vec::with_capacity(total);
        for (i, chunk) in chunks.iter().enumerate() {
            on_progress("chunk", i, total);
            let body = chunker::render(chunk);
            let raw = self
                .complete_json(&prompts::map_prompt(&req.title, i + 1, total, &body))
                .await?;
            partials.push(format!("--- Potongan {} ---\n{}", i + 1, extract_json(&raw)));
        }
        on_progress("reduce", 0, 1);
        let raw = self
            .complete_json(&prompts::reduce_prompt(&req.title, &partials.join("\n\n")))
            .await?;
        on_progress("reduce", 1, 1);
        serde_json::from_str(extract_json(&raw))
            .map_err(|e| AppError::Config(format!("the model did not return the expected JSON: {e}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_code_fences() {
        assert_eq!(extract_json("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(extract_json("  {\"a\":1}  "), "{\"a\":1}");
        assert_eq!(extract_json("Ini hasilnya: {\"a\":1} selesai"), "{\"a\":1}");
    }
}
