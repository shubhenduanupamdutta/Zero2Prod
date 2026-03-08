use serde::{Deserialize, Deserializer};

pub struct SubscriptionToken(String);

impl SubscriptionToken {
    pub fn parse(token: String) -> Result<Self, String> {
        let stripped_token = token.trim();
        let token_is_empty_or_whitespace = stripped_token.is_empty();
        let token_is_not_of_correct_size: bool = stripped_token.chars().count() != 25;

        let token_is_malformed = stripped_token
            .chars()
            .any(|c| !c.is_ascii_alphanumeric());

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