use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Serialize)]
pub struct SubscriberName(String);

impl SubscriberName {
    /// Returns an instance of `SubscriberName` if the input satisfies all our
    /// validation constraints on subscriber names, panics otherwise.
    pub fn parse(name: String) -> Result<SubscriberName, String> {
        let is_empty_or_whitespace = name.trim().is_empty();

        // A grapheme is defined by the Unicode Standard as a user-perceived character.
        // For example, the name "Beyoncé" consists of 7 graphemes, even though it has 8 Unicode code points (the "é" is represented as "e" followed by a combining acute accent).
        let is_too_long = name.graphemes(true).count() > 256;

        // Checking for any of the forbidden characters: '/', '(', ')', '"', '<', '>', '\\', '{', '}'
        let forbidden_characters = ['/', '(', ')', '"', '<', '>', '\\', '{', '}'];
        let contains_forbidden_characters = name.chars().any(|c| forbidden_characters.contains(&c));

        // Return false if any of the validation checks fail
        if is_empty_or_whitespace || is_too_long || contains_forbidden_characters {
            Err(format!("{} is not a valid subscriber name.", name))
        } else {
            Ok(Self(name))
        }
    }
}

impl AsRef<str> for SubscriberName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claims::{assert_err, assert_ok};

    #[test]
    fn a_256_grapheme_long_name_is_valid() {
        let name = "ë".repeat(256);
        assert_ok!(SubscriberName::parse(name));
    }

    #[test]
    fn a_name_longer_than_256_graphemes_is_rejected() {
        let name = "ë".repeat(257);
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn whitespace_only_names_are_rejected() {
        let name = " ".to_string();
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn empty_string_is_rejected() {
        let name = "".to_string();
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn names_containing_forbidden_characters_are_rejected() {
        for name in &['/', '(', ')', '"', '<', '>', '\\', '{', '}'] {
            let name = name.to_string();
            assert_err!(SubscriberName::parse(name));
        }
    }

    #[test]
    fn a_valid_name_is_parsed_successfully() {
        let name = "Ursula Le Guin".to_string();
        assert_ok!(SubscriberName::parse(name));
    }
}
