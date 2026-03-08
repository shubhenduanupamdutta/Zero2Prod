use tera::{Context, Error as TeraError, Tera};

pub struct EmailTemplateEngine(Tera);

impl EmailTemplateEngine {
    pub fn new(templates_dir: &str) -> Result<Self, TeraError> {
        let glob = format!("{}/emails/**/*", templates_dir);
        let mut tera = Tera::new(&glob)?;
        tera.autoescape_on(vec![".html"]);
        Ok(Self(tera))
    }

    pub fn render_confirmation_email(
        &self,
        recipient_name: &str,
        confirmation_link: &str,
    ) -> Result<String, TeraError> {
        let mut context = Context::new();
        context.insert("name", recipient_name);
        context.insert("confirmation_link", confirmation_link);
        self.0.render("confirmation.html", &context)
    }

    pub fn render_already_subscribed_email(
        &self,
        recipient_name: &str,
    ) -> Result<String, TeraError> {
        let mut context = Context::new();
        context.insert("name", recipient_name);
        self.0.render("already_confirmed.html", &context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claims::{assert_err, assert_ok};

    #[test]
    fn confirmation_email_contains_subscriber_name_and_link() {
        let engine = EmailTemplateEngine::new("templates").unwrap();
        let recipient_name = "John Doe";
        let confirmation_link = "http://example.com/confirm?token=abc123";
        let result = engine.render_confirmation_email(recipient_name, confirmation_link);
        assert_ok!(&result);
        let email_body = result.unwrap();

        // Confirm that the email body contains the recipient's name
        assert!(email_body.contains(recipient_name));

        // Confirm that email body contains link as href
        assert!(email_body.contains(&format!("href=\"{}\"", confirmation_link)));
        // Confirm that the email body contains the link as text but not as href
        let href_index = email_body
            .find(&format!("href=\"{}\"", confirmation_link))
            .unwrap();
        let index_after_href = href_index + format!("href=\"{}\"", confirmation_link).len();
        assert!(email_body[index_after_href..].contains(confirmation_link));
    }

    #[test]
    fn confirm_link_is_twice_in_the_email_body() {
        let engine = EmailTemplateEngine::new("templates").unwrap();
        let recipient_name = "John Doe";
        let confirmation_link = "http://example.com/confirm?token=abc123";
        let result = engine.render_confirmation_email(recipient_name, confirmation_link);
        assert_ok!(&result);
        let email_body = result.unwrap();

        // Confirm that the confirmation link appears twice in the email body
        let occurrences = email_body.matches(confirmation_link).count();
        assert!(
            occurrences > 2,
            "Expected confirmation link to appear at least twice in the email body"
        );
    }

    #[test]
    fn already_subscribed_email_contains_subscriber_name_and_does_not_contain_confirmation_link() {
        let engine = EmailTemplateEngine::new("templates").unwrap();
        let recipient_name = "John Doe";
        let result = engine.render_already_subscribed_email(recipient_name);
        assert_ok!(&result);
        let email_body = result.unwrap();

        // Confirm that the email body contains the recipient's name
        assert!(email_body.contains(recipient_name));

        // Confirm that the email body does not contain any confirmation link
        assert!(
            !email_body.contains("http://"),
            "already_confirmed email should not contain any URLs"
        );
    }

    #[test]
    fn unknown_template_returns_error() {
        let engine = EmailTemplateEngine::new("templates").unwrap();
        let result = engine
            .0
            .render("non_existent_template.html", &Context::new());
        assert_err!(result);
    }
}
