//! Deterministic regex-based fallback.
//!
//! Safety net for the case where the Trie mask misses a banned word.
//! Uses whole-word, case-insensitive matching to avoid false positives
//! on innocent substrings (e.g., "ass" inside "class").
//!
//! Known limitation: inflections of banned words ("shitting") are not matched
//! by a banned root ("shit") unless explicitly added to the list. Add the
//! inflected forms to `banned_words.txt` if needed.

pub struct FallbackRegex {
    patterns: Vec<regex::Regex>,
}

impl FallbackRegex {
    pub fn new(banned_words: &[String]) -> Self {
        let patterns = banned_words
            .iter()
            .map(|word| {
                let pattern = format!(r"(?i)\b{}\b", regex::escape(word));
                regex::Regex::new(&pattern).unwrap_or_else(|e| {
                    panic!("Invalid banned word {:?}: {}", word, e)
                })
            })
            .collect();
        Self { patterns }
    }

    pub fn apply(&self, text: &str) -> String {
        let mut result = text.to_string();
        for re in &self.patterns {
            result = re.replace_all(&result, "[redacted]").to_string();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(words: &[&str]) -> FallbackRegex {
        let owned: Vec<String> = words.iter().map(|s| s.to_string()).collect();
        FallbackRegex::new(&owned)
    }

    #[test]
    fn test_no_match_clean_text() {
        let fb = make(&["badword"]);
        assert_eq!(
            fb.apply("This is a perfectly clean sentence."),
            "This is a perfectly clean sentence."
        );
    }

    #[test]
    fn test_single_match() {
        let fb = make(&["badword"]);
        assert_eq!(fb.apply("You are a badword."), "You are a [redacted].");
    }

    #[test]
    fn test_case_insensitive() {
        let fb = make(&["badword"]);
        assert_eq!(fb.apply("BADWORD"), "[redacted]");
        assert_eq!(fb.apply("BadWord"), "[redacted]");
        assert_eq!(fb.apply("badWord"), "[redacted]");
    }

    #[test]
    fn test_word_boundary_no_false_positive() {
        // "ass" must not match inside innocent words.
        let fb = make(&["ass"]);
        assert_eq!(fb.apply("This class is a pass."), "This class is a pass.");
        assert_eq!(fb.apply("The assassin struck."), "The assassin struck.");
        assert_eq!(fb.apply("You ass."), "You [redacted].");
    }

    #[test]
    fn test_multi_word_phrase() {
        let fb = make(&["holy shit"]);
        assert_eq!(fb.apply("holy shit that hurt"), "[redacted] that hurt");
        assert_eq!(fb.apply("Holy Shit!"), "[redacted]!");
    }

    #[test]
    fn test_multiple_words() {
        let fb = make(&["badword", "worseword"]);
        assert_eq!(
            fb.apply("You are a badword and a worseword."),
            "You are a [redacted] and a [redacted]."
        );
    }

    #[test]
    fn test_regex_special_characters_escaped() {
        // Words containing regex metacharacters must be treated literally.
        // Note: the banned word must start and end with a word character for `\b`
        // to anchor correctly. This matches realistic banned-word usage.
        let fb = make(&["test.word"]);
        assert_eq!(fb.apply("This is test.word."), "This is [redacted].");
        // The dot must be escaped, not treated as "any character".
        assert_eq!(fb.apply("testXword"), "testXword");
        // Case-insensitive.
        assert_eq!(fb.apply("TEST.WORD"), "[redacted]");
    }

    #[test]
    fn test_punctuation_adjacent() {
        let fb = make(&["badword"]);
        assert_eq!(fb.apply("badword!"), "[redacted]!");
        assert_eq!(fb.apply("(badword)"), "([redacted])");
        assert_eq!(fb.apply("badword,"), "[redacted],");
    }

    #[test]
    fn test_multiple_occurrences() {
        let fb = make(&["badword"]);
        assert_eq!(
            fb.apply("badword and badword"),
            "[redacted] and [redacted]"
        );
    }
}