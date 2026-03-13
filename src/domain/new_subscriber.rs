use crate::domain::{subscriber_email::SubscriberEmail, subscriber_name::SubscriberName};

pub struct NewSubscriber {
    pub email: SubscriberEmail,
    pub name: SubscriberName,
}

impl NewSubscriber {
    pub fn parse(name: String, email: String) -> Result<Self, anyhow::Error> {
        Ok(Self {
            email: SubscriberEmail::parse(email).map_err(|e| anyhow::anyhow!(e))?,
            name: SubscriberName::parse(name).map_err(|e| anyhow::anyhow!(e))?,
        })
    }
}
