pub const SYSTEM: &str = r#"You are a meeting-notes assistant. The transcript you receive was produced
automatically by Whisper from a meeting recording, so it may contain mishearings, and the
language may switch back and forth within a single meeting.

Rules:
- Write the summary in whichever language dominates the meeting.
- Do not invent anything. If something is unclear in the transcript, put it under
  open_questions rather than guessing.
- The label "You" is the owner of this application; "Participant" is whoever they were
  talking to.
- Every item must carry at_ms: the timestamp in milliseconds of the transcript line it is
  based on. Take the number from the time marker at the start of that line.
- Reply with valid JSON only. No commentary, no code fences."#;

pub const SCHEMA_HINT: &str = r#"{
  "overview": "3-5 sentences",
  "decisions": [{"text": "a decision that was actually made", "at_ms": 0}],
  "action_items": [{"task": "", "pic": "name or empty", "due": "deadline or empty", "at_ms": 0}],
  "open_questions": ["a question that was left unanswered"],
  "topics": [{"title": "", "start_ms": 0, "end_ms": 0, "gist": ""}]
}"#;

pub fn map_prompt(title: &str, part: usize, total: usize, body: &str) -> String {
    format!(
        "Meeting: {title}\nThis is part {part} of {total} of the transcript.\n\n\
         Summarize ONLY what is in this part. Do not draw conclusions about anything that has \
         not happened yet within this part.\n\n\
         Output format:\n{SCHEMA_HINT}\n\n=== TRANSCRIPT ===\n{body}"
    )
}

pub fn reduce_prompt(title: &str, partials: &str) -> String {
    format!(
        "Meeting: {title}\nBelow are the per-part summaries of the meeting, in order.\n\n\
         Merge them into ONE final summary. Remove duplicates, combine action items that are \
         the same, and keep the at_ms of the first occurrence of each item. Sort topics by \
         time.\n\nOutput format:\n{SCHEMA_HINT}\n\n=== PART SUMMARIES ===\n{partials}"
    )
}
