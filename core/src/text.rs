//! Bounded text buffers for output received from external processes/devices.

/// Keep the newest complete UTF-8 characters within `max_bytes` bytes.
pub fn keep_tail(text: &mut String, max_bytes: usize) {
    let mut cut = text.len().saturating_sub(max_bytes);
    while !text.is_char_boundary(cut) {
        cut += 1;
    }
    if cut > 0 {
        text.drain(..cut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_complete_characters_within_every_byte_limit() {
        let original = "aé界🦀tail";
        for limit in 0..=original.len() + 1 {
            let mut text = original.to_string();
            keep_tail(&mut text, limit);
            assert!(text.len() <= limit);
            assert!(original.ends_with(&text));
            let expected = original
                .char_indices()
                .map(|(start, _)| &original[start..])
                .find(|tail| tail.len() <= limit)
                .unwrap_or("");
            assert_eq!(text, expected);
        }
    }

    #[test]
    fn handles_empty_and_large_ascii_buffers() {
        let mut empty = String::new();
        keep_tail(&mut empty, 0);
        assert!(empty.is_empty());
        let mut text = format!("{}end", "x".repeat(5000));
        keep_tail(&mut text, 4096);
        assert_eq!(text.len(), 4096);
        assert!(text.ends_with("end"));
    }
}
