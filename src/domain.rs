use unicode_segmentation::UnicodeSegmentation;

pub struct NewSubscriber {
    pub email: String,
    pub name: SubscriberName,
}

pub struct SubscriberName(String);

impl SubscriberName {
    /// Returns an instance of `SubscriberName` if the input satisfies all our
    /// validation constraints on subscriber names, panics otherwise.
    pub fn parse(name: String) -> SubscriberName {
        let is_empty_or_whitespace = name.trim().is_empty();

        // A grapheme is defined by the Unicode Standard as a user-perceived character.
        // For example, the name "Beyoncé" consists of 7 graphemes, even though it has 8 Unicode code points (the "é" is represented as "e" followed by a combining acute accent).
        let is_too_long = name.graphemes(true).count() > 256;

        // Checking for any of the forbidden characters: '/', '(', ')', '"', '<', '>', '\\', '{', '}'
        let forbidden_characters = ['/', '(', ')', '"', '<', '>', '\\', '{', '}'];
        let contains_forbidden_characters = name.chars().any(|c| forbidden_characters.contains(&c));

        // Return false if any of the validation checks fail
        if is_empty_or_whitespace || is_too_long || contains_forbidden_characters {
            panic!("{} is not a valid subscriber name.", name);
        } else {
            Self(name)
        }
    }
}

impl AsRef<str> for SubscriberName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}