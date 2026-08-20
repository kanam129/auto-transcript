pub mod chunker;
pub mod llm;
pub mod prompts;

use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptLine {
    pub at_ms: i64,
    pub speaker: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct SummaryRequest {
    pub title: String,
    pub lines: Vec<TranscriptLine>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Point {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub at_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionItem {
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub pic: String,
    #[serde(default)]
    pub due: String,
    #[serde(default)]
    pub at_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Topic {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub start_ms: i64,
    #[serde(default)]
    pub end_ms: i64,
    #[serde(default)]
    pub gist: String,
}

/// Every item carries `at_ms` so it can later be clicked to jump to that point in the
/// transcript — which is what makes a summary verifiable rather than merely believable.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Summary {
    #[serde(default)]
    pub overview: String,
    #[serde(default)]
    pub decisions: Vec<Point>,
    #[serde(default)]
    pub action_items: Vec<ActionItem>,
    #[serde(default)]
    pub open_questions: Vec<String>,
    #[serde(default)]
    pub topics: Vec<Topic>,
}

/// The only implementation today is `llm::OpenAiCompatible`. The trait exists so a local
/// backend (llama.cpp) can be added later without changing any caller.
pub trait Summarizer {
    fn summarize<F>(
        &self,
        req: SummaryRequest,
        on_progress: F,
    ) -> impl std::future::Future<Output = Result<Summary>> + Send
    where
        F: FnMut(&str, usize, usize) + Send;
}
