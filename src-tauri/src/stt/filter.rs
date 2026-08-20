/// Above this `no_speech_prob`, a segment is treated as not speech.
pub const NO_SPEECH_THRESHOLD: f32 = 0.6;

/// Phrases Whisper hallucinates when the audio is actually silence or just music. The model
/// has seen a great many subtitle files that end this way, so it invents one when there is
/// nothing to transcribe.
const HALLUCINATIONS: &[&str] = &[
    "terima kasih telah menonton",
    "terima kasih sudah menonton",
    "terima kasih telah menyaksikan",
    "jangan lupa like dan subscribe",
    "jangan lupa subscribe",
    "sampai jumpa di video selanjutnya",
    "sampai jumpa di video berikutnya",
    "thanks for watching",
    "thank you for watching",
    "please subscribe",
    "subscribe to my channel",
    "like and subscribe",
    "subtitles by",
    "subtitle by",
    "amara.org",
    "transcribed by",
    // Short interjections that are almost always hallucinations when they stand alone as a
    // whole segment — usually produced by a breath, a throat clear, or a stretch of silence.
    // Observed for real: the very first test session produced one segment reading
    // "Thank you." from a microphone in a quiet room.
    "you",
    "thank you",
    "thanks",
    "bye",
    "bye bye",
    "terima kasih",
    "makasih",
    "terimakasih",
];

/// The phrases above are only dropped when they dominate the segment. A long sentence that
/// happens to contain "thank you" in the middle is kept.
const DOMINANCE: f32 = 0.8;

pub fn normalize(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '.')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches('.')
        .to_string()
}

fn is_bracket_tag(s: &str) -> bool {
    let t = s.trim();
    (t.starts_with('[') && t.ends_with(']'))
        || (t.starts_with('(') && t.ends_with(')'))
        || (t.starts_with('*') && t.ends_with('*'))
        || t.chars().all(|c| c == '♪' || c.is_whitespace())
}

fn has_excessive_repetition(norm: &str) -> bool {
    let words: Vec<&str> = norm.split_whitespace().collect();
    if words.len() < 4 {
        return false;
    }
    let mut run = 1;
    for w in words.windows(2) {
        if w[0] == w[1] {
            run += 1;
            if run > 3 {
                return true;
            }
        } else {
            run = 1;
        }
    }
    // Also catch two-word phrase loops: "okay okay okay okay".
    let unique: std::collections::HashSet<&&str> = words.iter().collect();
    words.len() >= 8 && unique.len() <= 2
}

/// Returns the cleaned text, or `None` if this segment should be dropped.
pub fn accept(text: &str, no_speech: f32) -> Option<String> {
    if no_speech > NO_SPEECH_THRESHOLD {
        return None;
    }
    let trimmed = text.trim();
    if trimmed.is_empty() || is_bracket_tag(trimmed) {
        return None;
    }
    let norm = normalize(trimmed);
    if norm.is_empty() {
        return None;
    }
    if has_excessive_repetition(&norm) {
        return None;
    }
    for &phrase in HALLUCINATIONS {
        if norm == phrase {
            return None;
        }
        if norm.contains(phrase) && (phrase.len() as f32 / norm.len() as f32) >= DOMINANCE {
            return None;
        }
    }
    Some(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_the_classic_hallucinations() {
        assert!(accept("Terima kasih telah menonton.", 0.1).is_none());
        assert!(accept("Thanks for watching!", 0.1).is_none());
        assert!(accept("[Music]", 0.1).is_none());
        assert!(accept("♪♪♪", 0.1).is_none());
        assert!(accept("   ", 0.1).is_none());
    }

    #[test]
    fn drops_segments_with_high_no_speech() {
        assert!(accept("halo semuanya", 0.9).is_none());
    }

    #[test]
    fn keeps_ordinary_sentences() {
        assert!(accept("Oke, jadi blocker utamanya di payment gateway.", 0.1).is_some());
        assert!(
            accept(
                "Terima kasih Pak Budi, tapi saya masih menunggu konfirmasi dari tim finance.",
                0.1
            )
            .is_some(),
            "a common phrase inside a long sentence must not be dropped"
        );
    }

    #[test]
    fn drops_short_standalone_interjections() {
        // A real case from the first test session: a microphone in a quiet room produced a
        // single segment reading "Thank you." with a low no_speech score.
        assert!(accept("Thank you.", 0.0).is_none());
        assert!(accept(" Terima kasih ", 0.05).is_none());
        assert!(accept("Bye bye", 0.1).is_none());
        // But a sentence that merely contains the phrase is kept.
        assert!(accept("Thank you for sending the spec yesterday.", 0.1).is_some());
    }

    #[test]
    fn drops_excessive_repetition() {
        assert!(accept("oke oke oke oke oke", 0.1).is_none());
        assert!(accept("ya ya ya ya ya ya ya ya", 0.1).is_none());
    }
}
