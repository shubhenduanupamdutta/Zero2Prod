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
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap();

        Self {
            http_client,
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
        let request_body = EmailBody::new(from, to, subject, html_content);

        self.http_client
            .post(url)
            .header("Authorization", self.authorization_token.expose_secret())
            .json(&request_body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claims::assert_err;
    use fake::faker::internet::en::SafeEmail;
    use fake::faker::lorem::en::{Paragraph, Sentence};
    use fake::faker::name::en::Name;
    use fake::{Fake, Faker};
    use wiremock::matchers::{any, header, header_exists, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    struct SendEmailBodyMatcher;

    impl wiremock::Match for SendEmailBodyMatcher {
        fn matches(&self, request: &Request) -> bool {
            // Try to parse the body as json value
            let result: Result<serde_json::Value, _> = serde_json::from_slice(&request.body);
            if let Ok(body) = result {
                dbg!(&body);
                // Check that all mandatory fields are present
                body.get("from").is_some()
                    && body.get("to").is_some()
                    && body.get("subject").is_some()
                    && body.get("html_body").is_some()
            } else {
                false
            }
        }
    }

    /// Generate a random email subject
    fn subject() -> String {
        Sentence(1..2).fake()
    }

    /// Generate random email content
    fn content() -> String {
        Paragraph(1..10).fake()
    }

    /// Generate a random subscriber email
    fn email() -> SubscriberEmail {
        SubscriberEmail::parse(SafeEmail().fake()).unwrap()
    }

    /// Generate a random subscriber name
    fn name() -> SubscriberName {
        SubscriberName::parse(Name().fake()).unwrap()
    }

    /// Get a test instance of EmailClient
    fn email_client(base_url: String) -> EmailClient {
        EmailClient::new(base_url, email(), name(), Faker.fake::<String>().into())
    }

    #[tokio::test]
    async fn send_email_sends_the_expected_request() {
        // Arrange
        let mock_server = MockServer::start().await;
        let email_client = email_client(mock_server.uri());

        Mock::given(header_exists("Authorization"))
            .and(header("Content-Type", "application/json"))
            .and(method("POST"))
            .and(path("/email"))
            .and(SendEmailBodyMatcher)
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Act
        let _ = email_client
            .send_email(email(), name(), &subject(), &content())
            .await;

        // Assert
        // Mock expectations are asserted on drop
    }

    #[tokio::test]
    async fn send_email_succeeds_if_the_server_returns_200() {
        // Arrange
        let mock_server = MockServer::start().await;
        let email_client = email_client(mock_server.uri());

        // Purpose of this test is not to assert on the request we send out so we
        // add bare minimum needed to trigger the path we want
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Act
        let outcome = email_client
            .send_email(email(), name(), &subject(), &content())
            .await;

        // Assert
        assert!(outcome.is_ok());
    }

    #[tokio::test]
    async fn send_email_fails_if_the_server_returns_500() {
        // Arrange
        let mock_server = MockServer::start().await;
        let email_client = email_client(mock_server.uri());

        // Purpose of this test is not to assert on the request we send out so we
        // add bare minimum needed to trigger the path we want
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Act
        let outcome = email_client
            .send_email(email(), name(), &subject(), &content())
            .await;

        // Assert
        assert_err!(outcome);
    }

    #[tokio::test]
    async fn send_email_times_out_if_the_server_takes_too_long() {
        // Arrange
        let mock_server = MockServer::start().await;
        let email_client = email_client(mock_server.uri());

        // Purpose of this test is not to assert on the request we send out so we
        // add bare minimum needed to trigger the path we want
        let response = ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(180));

        Mock::given(any())
            .respond_with(response)
            .expect(1)
            .mount(&mock_server)
            .await;

        // Act
        let outcome = email_client
            .send_email(email(), name(), &subject(), &content())
            .await;

        // Assert
        assert_err!(outcome);
    }
}
