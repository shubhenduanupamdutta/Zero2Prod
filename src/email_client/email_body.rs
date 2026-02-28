use serde::{Deserialize, Serialize};

use crate::domain::{SubscriberEmail, SubscriberName};

pub type ParsedEmail = SubscriberEmail;
pub type ParsedName = SubscriberName;

#[derive(Serialize, Deserialize)]
pub struct NameAndEmail<'a> {
    address: &'a str,
    name: &'a str,
}

impl<'a> NameAndEmail<'a> {
    pub(crate) fn new(email: &'a str, name: &'a str) -> Result<Self, String> {
        Ok(Self {
            address: email,
            name,
        })
    }
}

#[derive(Serialize)]
pub struct EmailAddress<'a> {
    email_address: NameAndEmail<'a>,
}

#[derive(Serialize)]
pub struct EmailBody<'a> {
    from: NameAndEmail<'a>,
    to: Vec<EmailAddress<'a>>,
    cc: Option<Vec<EmailAddress<'a>>>,
    reply_to: Option<NameAndEmail<'a>>,
    subject: &'a str,
    html_body: &'a str,
}

impl<'a> EmailBody<'a> {
    pub(crate) fn new(
        from: NameAndEmail<'a>,
        to: NameAndEmail<'a>,
        subject: &'a str,
        html_body: &'a str,
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
