use super::db::{Segment, SessionMeta};
use crate::error::{AppError, Result};

pub fn speaker_label(track: &str) -> &'static str {
    match track {
        "mic" => "You",
        _ => "Participant",
    }
}

fn hhmmss(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn srt_time(ms: i64) -> String {
    let ms = ms.max(0);
    let (h, m, s, milli) = (
        ms / 3_600_000,
        (ms % 3_600_000) / 60_000,
        (ms % 60_000) / 1000,
        ms % 1000,
    );
    format!("{h:02}:{m:02}:{s:02},{milli:03}")
}

fn local_datetime(unix_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_millis_opt(unix_ms).single() {
        Some(dt) => dt.format("%d %B %Y, %H:%M").to_string(),
        None => "-".into(),
    }
}

pub fn render(meta: &SessionMeta, segments: &[Segment], format: &str) -> Result<String> {
    match format {
        "md" => Ok(render_md(meta, segments)),
        "txt" => Ok(render_txt(segments)),
        "srt" => Ok(render_srt(segments)),
        "json" => Ok(serde_json::to_string_pretty(&serde_json::json!({
            "session": meta,
            "segments": segments,
        }))?),
        other => Err(AppError::Config(format!("unknown export format '{other}'"))),
    }
}

pub fn extension(format: &str) -> &'static str {
    match format {
        "md" => "md",
        "srt" => "srt",
        "json" => "json",
        _ => "txt",
    }
}

fn render_md(meta: &SessionMeta, segments: &[Segment]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", meta.title));
    out.push_str(&format!("- Time: {}\n", local_datetime(meta.started_at)));
    if let Some(d) = meta.duration_ms {
        out.push_str(&format!("- Duration: {}\n", hhmmss(d)));
    }
    if let Some(m) = &meta.model_used {
        out.push_str(&format!("- Model: {m}\n"));
    }
    if let Some(s) = &meta.source_name {
        out.push_str(&format!("- Audio source: {s}\n"));
    }
    out.push_str(&format!("- Segments: {}\n\n---\n\n", segments.len()));

    let mut last_speaker = "";
    for s in segments {
        let label = speaker_label(&s.track);
        if label != last_speaker {
            out.push_str(&format!("\n**{label}**\n\n"));
            last_speaker = label;
        }
        out.push_str(&format!("`{}` {}\n\n", hhmmss(s.start_ms), s.text.trim()));
    }
    out
}

fn render_txt(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|s| {
            format!(
                "[{}] {}: {}",
                hhmmss(s.start_ms),
                speaker_label(&s.track),
                s.text.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_srt(segments: &[Segment]) -> String {
    let mut out = String::new();
    for (i, s) in segments.iter().enumerate() {
        // A 700 ms minimum so a subtitle stays on screen long enough to read.
        let end = s.end_ms.max(s.start_ms + 700);
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            srt_time(s.start_ms),
            srt_time(end),
            s.text.trim()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(id: i64, start: i64, end: i64, track: &str, text: &str) -> Segment {
        Segment {
            id,
            session_id: "s1".into(),
            track: track.into(),
            start_ms: start,
            end_ms: end,
            text: text.into(),
            lang: Some("id".into()),
            confidence: Some(0.9),
            edited: false,
        }
    }

    #[test]
    fn srt_format_valid() {
        let s = render_srt(&[seg(1, 1500, 3200, "system", "halo")]);
        assert!(s.starts_with("1\n00:00:01,500 --> 00:00:03,200\nhalo\n\n"), "got: {s}");
    }

    #[test]
    fn srt_enforces_a_minimum_duration() {
        let s = render_srt(&[seg(1, 1000, 1050, "system", "ya")]);
        assert!(s.contains("--> 00:00:01,700"), "got: {s}");
    }

    #[test]
    fn txt_labels_the_speaker() {
        let s = render_txt(&[
            seg(1, 0, 1000, "system", "hello"),
            seg(2, 1000, 2000, "mic", "hi"),
        ]);
        assert!(s.contains("Participant: hello"));
        assert!(s.contains("You: hi"));
    }
}
