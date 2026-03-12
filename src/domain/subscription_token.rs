use serde::{Deserialize, Deserializer};

#[derive(Debug)]
pub struct SubscriptionToken(String);

impl SubscriptionToken {
    pub fn parse(token: String) -> Result<Self, String> {
        let stripped_token = token.trim();
        let token_is_empty_or_whitespace = stripped_token.is_empty();
        let token_is_not_of_correct_size: bool = stripped_token.chars().count() != 25;

        let token_is_malformed = stripped_token.chars().any(|c| !c.is_ascii_alphanumeric());

        if token_is_empty_or_whitespace || token_is_not_of_correct_size || token_is_malformed {
            Err(format!("{} is not a valid subscription token.", token))
        } else {
            Ok(Self(stripped_token.to_string()))
        }
    }
}

impl AsRef<str> for SubscriptionToken {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for SubscriptionToken {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        SubscriptionToken::parse(raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use claims::{assert_err, assert_ok};

    use super::*;

    #[test]
    fn a_valid_25_character_alphanumeric_token_is_parsed_successfully() {
        let token = "a234567890123495678901234".to_string();
        let parsed_token = SubscriptionToken::parse(token.clone());
        assert_ok!(&parsed_token);
        assert_eq!(parsed_token.unwrap().as_ref(), token);
    }

    #[test]
    fn a_token_with_24_characters_is_rejected() {
        let token = "a2345678901234567890123".to_string();
        assert_err!(SubscriptionToken::parse(token));
    }

    #[test]
    fn a_token_with_26_characters_is_rejected() {
        let token = "a234567890123456789012345k".to_string();
        assert_err!(SubscriptionToken::parse(token));
    }

    #[test]
    fn an_empty_token_is_rejected() {
        let token = "".to_string();
        assert_err!(SubscriptionToken::parse(token));
    }

    #[test]
    fn a_token_with_non_alphanumeric_characters_is_rejected() {
        let tokens = vec![
            "a2345678901234567890123$".to_string(),
            "aБ3456789012345678901234".to_string(),
            "adf\x0056789012345678901234".to_string(),
        ];
        for token in tokens {
            assert_err!(SubscriptionToken::parse(token));
        }
    }

    #[test]
    fn a_token_uppercase_and_lowercase_characters_is_parsed_successfully() {
        let token = "a234567890123456789012C3A".to_string();
        let parsed_token = SubscriptionToken::parse(token.clone());
        assert_ok!(&parsed_token);
        assert_eq!(parsed_token.unwrap().as_ref(), token);
    }
}
