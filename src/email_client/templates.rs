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
        context.insert("recipient_name", recipient_name);
        context.insert("confirmation_link", confirmation_link);
        self.0.render("confirmation.html", &context)
    }

    pub fn render_already_subscribed_email(
        &self,
        recipient_name: &str,
    ) -> Result<String, TeraError> {
        let mut context = Context::new();
        context.insert("recipient_name", recipient_name);
        self.0.render("already_subscribed.html", &context)
    }
}
