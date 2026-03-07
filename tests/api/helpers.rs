use std::sync::LazyLock;

use chrono::Utc;
use rand::{Rng, distr::Alphanumeric, rng};
use secrecy::SecretString;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
use wiremock::MockServer;
use zero2prod::{
    configuration::{get_configuration, DatabaseSettings},
    startup::{get_connection_pool, Application},
    telemetry::{get_subscriber, init_subscriber},
};

// Ensure that the `tracing` stack is only initialized once when the first test is run, and is not
// re-initialized for each test
static TRACING: LazyLock<()> = LazyLock::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();

    // We cannot assign the output of `get_subscriber` to a variable based on the
    // value TEST_LOG` because the sink is part of the type returned by
    // `get_subscriber`, therefore they are not the same type. We could work around
    // it, but this is the most straight-forward way of moving forward.
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);
    };
});

pub struct TestApp {
    pub port: u16,
    pub address: String,
    pub db_pool: PgPool,
    pub email_server: MockServer,
}

/// Confirmation Links embedded in the request to the email API
pub struct ConfirmationLinks {
    pub link: reqwest::Url,
}

impl TestApp {
    pub async fn post_subscriptions(&self, body: String) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}/subscriptions", &self.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub async fn confirm_subscriptions(&self, token: &str) -> reqwest::Response {
        reqwest::Client::new()
            .get(format!(
                "{}/subscriptions/confirm?subscription_token={}",
                &self.address, token
            ))
            .send()
            .await
            .expect("Failed to execute request")
    }

    /// Extract the confirmation links embedded in the request to the email API
    pub fn get_confirmation_links(&self, email_request: &wiremock::Request) -> ConfirmationLinks {
        let body: serde_json::Value = serde_json::from_slice(&email_request.body).unwrap();

        // Extract the link from one of the request fields
        let get_link = |s: &str| {
            let links: Vec<_> = linkify::LinkFinder::new()
                .links(s)
                .filter(|l| *l.kind() == linkify::LinkKind::Url)
                .collect();
            assert_eq!(links.len(), 1);
            let raw_link = links[0].as_str().to_owned();
            let mut confirmation_link = reqwest::Url::parse(&raw_link).unwrap();
            // Let's make sure we don't call random API on the web
            assert_eq!(confirmation_link.host_str().unwrap(), "127.0.0.1");
            confirmation_link.set_port(Some(self.port)).unwrap();
            confirmation_link
        };
        let link = get_link(&body["htmlbody"].as_str().unwrap());
        ConfirmationLinks {
            link,
        }
    }

    pub async fn insert_subscriber(&self, email: &str, name: &str, status: Option<&str>) -> Uuid {
        let subscriber_id = Uuid::new_v4();
        let status = status.unwrap_or("pending_confirmation");
        sqlx::query!(
            r#"
            INSERT INTO subscriptions (id, email, name, subscribed_at, status)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            subscriber_id,
            email,
            name,
            Utc::now(),
            status
        )
        .execute(&self.db_pool)
        .await
        .expect("Failed to insert test subscriber.");
        subscriber_id
    }

    pub async fn insert_subscription_token(&self, subscriber_id: Uuid, token: &str, created_at: chrono::DateTime<Utc>, consumed_at: Option<chrono::DateTime<Utc>>) {
        sqlx::query!(
            r#"
            INSERT INTO subscription_tokens (subscriber_id, subscription_token, created_at, consumed_at)
            VALUES ($1, $2, $3, $4)
            "#,
            subscriber_id,
            token,
            created_at,
            consumed_at
        )
        .execute(&self.db_pool)
        .await
        .expect("Failed to insert test subscription token.");
    }
}

pub async fn spawn_app() -> TestApp {
    LazyLock::force(&TRACING);

    // Launch a mock server to stand in for ZeptoMail API
    let email_server = MockServer::start().await;

    // Randomise configuration to ensure test isolation
    let configuration = {
        let mut c = get_configuration().expect("Failed to read configuration.");
        // Use a different database for each test case
        c.database.database_name = Uuid::new_v4().to_string();
        // Use a random OS port
        c.application.port = 0;
        // Point the email client at the mock server
        c.email_client.base_url = email_server.uri();
        c
    };

    // Create and migrate the database
    configure_database(&configuration.database).await;

    // Launch the application as a background task
    let application = Application::build(configuration.clone())
        .await
        .expect("Failed to build application.");
    let application_port = application.port();
    // Get the port before spawning the application
    let address = format!("http://127.0.0.1:{}", application_port);

    let _server_handle = tokio::spawn(application.run_until_stopped());

    TestApp {
        port: application_port,
        address,
        db_pool: get_connection_pool(&configuration.database),
        email_server,
    }
}

async fn configure_database(config: &DatabaseSettings) -> PgPool {
    // Create database
    let maintenance_settings = DatabaseSettings {
        database_name: "postgres".to_string(),
        username: "postgres".to_string(),
        password: SecretString::new("password".into()),
        ..config.clone()
    };

    let mut connection = PgConnection::connect_with(&maintenance_settings.connect_options())
        .await
        .expect("Failed to connect to Postgres for database creation.");

    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("Failed to create database.");

    // Migrate database
    let connection_pool = PgPool::connect_with(config.connect_options())
        .await
        .expect("Failed to connect to Postgres for database migration.");
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to run database migrations.");

    connection_pool
}

pub fn generate_token() -> String {
    let mut rng = rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}