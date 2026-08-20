use super::TranscriptLine;

/// Rough per-chunk limit in characters. About 12k characters is 3–4k tokens, which is safe
/// for nearly every model and leaves room for the instructions and the reply.
pub const CHUNK_CHARS: usize = 12_000;

/// Splits a transcript into chunks whose boundaries never cut through a line, with one line
/// of overlap so the opening sentence of each chunk has some context.
pub fn split(lines: &[TranscriptLine], max_chars: usize) -> Vec<Vec<TranscriptLine>> {
    if lines.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut current: Vec<TranscriptLine> = Vec::new();
    let mut size = 0usize;

    for line in lines {
        let cost = line.text.len() + line.speaker.len() + 16;
        if size + cost > max_chars && !current.is_empty() {
            let overlap = current.last().cloned();
            chunks.push(std::mem::take(&mut current));
            if let Some(o) = overlap {
                size = o.text.len() + o.speaker.len() + 16;
                current.push(o);
            } else {
                size = 0;
            }
        }
        current.push(line.clone());
        size += cost;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

pub fn render(lines: &[TranscriptLine]) -> String {
    lines
        .iter()
        .map(|l| {
            let s = l.at_ms / 1000;
            format!("[{:02}:{:02}:{:02}] {}: {}", s / 3600, (s % 3600) / 60, s % 60, l.speaker, l.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(i: i64, n: usize) -> TranscriptLine {
        TranscriptLine {
            at_ms: i * 1000,
            speaker: "Peserta".into(),
            text: "x".repeat(n),
        }
    }

    #[test]
    fn never_splits_in_the_middle_of_a_line() {
        let lines: Vec<_> = (0..50).map(|i| line(i, 100)).collect();
        let chunks = split(&lines, 1000);
        assert!(chunks.len() > 1);
        for c in &chunks {
            for l in c {
                assert_eq!(l.text.len(), 100, "a line must never be split");
            }
        }
    }

    #[test]
    fn overlaps_by_one_line() {
        let lines: Vec<_> = (0..50).map(|i| line(i, 100)).collect();
        let chunks = split(&lines, 1000);
        for w in chunks.windows(2) {
            assert_eq!(
                w[0].last().unwrap().at_ms,
                w[1].first().unwrap().at_ms,
                "the last line must repeat in the next chunk"
            );
        }
    }

    #[test]
    fn an_empty_transcript_yields_no_chunks() {
        assert!(split(&[], 1000).is_empty());
    }
}
