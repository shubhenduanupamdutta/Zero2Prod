use std::net::TcpListener;

use actix_web::{App, HttpServer, dev::Server, web};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tracing_actix_web::TracingLogger;

use crate::{
    configuration::{DatabaseSettings, Settings},
    email_client::{EmailClient, EmailTemplateEngine},
    routes::{confirm, health_check, publish_newsletter, subscribe},
    token_cleanup::spawn_token_cleanup_task,
};

/// A new type to hold the newly built server and its port
pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub async fn build(configuration: Settings) -> Result<Self, std::io::Error> {
        let connection_pool = get_connection_pool(&configuration.database);

        spawn_token_cleanup_task(
            connection_pool.clone(),
            configuration.subscription.token_cleanup_interval_hours,
            configuration.subscription.token_retention_hours,
        );

        let (sender_name, sender_email) = configuration
            .email_client
            .sender_name_end_email()
            .expect("Invalid sender email or name.");
        let timeout = configuration.email_client.timeout();
        let email_client = EmailClient::new(
            configuration.email_client.base_url,
            sender_email,
            sender_name,
            configuration.email_client.authorization_token,
            timeout,
        );

        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port
        );
        let listener = TcpListener::bind(address)?;
        let port = listener.local_addr().unwrap().port();

        let server = run(
            listener,
            connection_pool,
            email_client,
            configuration.application.base_url,
            configuration.subscription.token_expiration_seconds,
        )?;
        Ok(Self {
            port,
            server,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// This function only returns when the application is stopped
    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new().connect_lazy_with(configuration.connect_options())
}

pub struct ApplicationBaseUrl(pub String);

pub fn run(
    listener: TcpListener,
    db_pool: PgPool,
    email_client: EmailClient,
    base_url: String,
    token_expiration_seconds: u32,
) -> Result<Server, std::io::Error> {
    let pool = web::Data::new(db_pool);
    let email_client = web::Data::new(email_client);
    let base_url = web::Data::new(ApplicationBaseUrl(base_url));
    let token_expiration_seconds = web::Data::new(token_expiration_seconds);
    let email_template_engine = web::Data::new(
        EmailTemplateEngine::new("templates").expect("Failed to initialize email template engine"),
    );
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .route("/subscriptions/confirm", web::get().to(confirm))
            .route("/newsletters", web::post().to(publish_newsletter))
            .app_data(pool.clone())
            .app_data(email_client.clone())
            .app_data(base_url.clone())
            .app_data(token_expiration_seconds.clone())
            .app_data(email_template_engine.clone())
    })
    .listen(listener)?
    .run();
    Ok(server)
}
