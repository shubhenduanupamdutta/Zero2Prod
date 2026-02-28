use crate::domain::{SubscriberEmail, SubscriberName};
use reqwest::{Client, Url};
pub mod email_body;

use email_body::{EmailBody, NameAndEmail, ParsedEmail, ParsedName};
use secrecy::{ExposeSecret, SecretString};

pub struct EmailClient {
    http_client: Client,
    base_url: Url,
    sender_email: ParsedEmail,
    sender_name: ParsedName,
    authorization_token: SecretString,
}

impl EmailClient {
    pub fn new(
        base_url: String,
        sender_email: ParsedEmail,
        sender_name: ParsedName,
        authorization_token: SecretString,
    ) -> Self {
        let base_url = Url::parse(&base_url).expect("Invalid base URL");
        Self {
            http_client: Client::new(),
            base_url,
            sender_email,
            sender_name,
            authorization_token,
        }
    }

    pub async fn send_email(
        &self,
        recipient_email: SubscriberEmail,
        recipient_name: SubscriberName,
        subject: &str,
        html_content: &str,
    ) -> Result<(), String> {
        let url = self
            .base_url
            .join("/email")
            .expect("Failed to construct URL");

        let from = NameAndEmail::new(self.sender_email.as_ref(), self.sender_name.as_ref())?;
        let to = NameAndEmail::new(recipient_email.as_ref(), recipient_name.as_ref())?;
        let request_body = EmailBody::new(from, to, subject.to_string(), html_content.to_string());

        self.http_client
            .post(url)
            .header("Authorization", self.authorization_token.expose_secret())
            .json(&request_body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fake::faker::internet::en::SafeEmail;
    use fake::faker::lorem::en::{Paragraph, Sentence};
    use fake::faker::name::en::Name;
    use fake::{Fake, Faker};
    use wiremock::matchers::any;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn send_email_fires_a_request_to_base_url() {
        // Arrange
        let mock_server = MockServer::start().await;
        let sender_email = SubscriberEmail::parse(SafeEmail().fake()).unwrap();
        let sender_name = SubscriberName::parse(Name().fake()).unwrap();
        let email_client = EmailClient::new(
            mock_server.uri(),
            sender_email,
            sender_name,
            Faker.fake::<String>().into(),
        );

        Mock::given(any())
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let subscriber_email = SubscriberEmail::parse(SafeEmail().fake()).unwrap();
        let subscriber_name = SubscriberName::parse(Name().fake()).unwrap();
        let subject: String = Sentence(1..2).fake();
        let content: String = Paragraph(1..10).fake();

        // Act
        let _ = email_client
            .send_email(subscriber_email, subscriber_name, &subject, &content)
            .await;

        // Assert
    }
}
