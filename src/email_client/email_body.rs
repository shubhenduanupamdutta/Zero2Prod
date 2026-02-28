use serde::{Deserialize, Serialize};

use crate::domain::{SubscriberEmail, SubscriberName};

pub type ParsedEmail = SubscriberEmail;
pub type ParsedName = SubscriberName;

#[derive(Serialize, Deserialize)]
pub struct NameAndEmail {
    address: String,
    name: String,
}

impl NameAndEmail {
    pub(crate) fn new(email: &str, name: &str) -> Result<Self, String> {
        Ok(Self {
            address: email.to_string(),
            name: name.to_string(),
        })
    }
}

#[derive(Serialize)]
pub struct EmailAddress {
    email_address: NameAndEmail,
}

#[derive(Serialize)]
pub struct EmailBody {
    from: NameAndEmail,
    to: Vec<EmailAddress>,
    cc: Option<Vec<EmailAddress>>,
    reply_to: Option<NameAndEmail>,
    subject: String,
    html_body: String,
}

impl EmailBody {
    pub(crate) fn new(
        from: NameAndEmail,
        to: NameAndEmail,
        subject: String,
        html_body: String,
    ) -> Self {
        Self {
            from,
            to: vec![EmailAddress {
                email_address: to,
            }],
            cc: None,
            reply_to: None,
            subject,
            html_body,
        }
    }
}
