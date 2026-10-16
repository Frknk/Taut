//! Configuration for taut services.
//!
//! Every service builds a [`Settings`] value once at startup with [`Settings::load`].
//! Values come from environment variables with the `TAUT_` prefix, using `__` to nest
//! (`TAUT_HTTP__ADDR=0.0.0.0:8080` sets `http.addr`). `DATABASE_URL`, `REDIS_URL` and
//! `NATS_URL` are also read without prefix because `sqlx-cli` and the official container
//! images use those names; the prefixed variable wins when both are set.
//!
//! Outside production a `.env` file in the working directory is loaded first, so local
//! development only needs `cp .env.example .env`.

use std::net::SocketAddr;

use figment::Figment;
use figment::providers::{Env, Serialized};
use serde::{Deserialize, Serialize};

/// Deployment environment the service runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    /// Local development.
    Dev,
    /// Automated tests.
    Test,
    /// Shared pre-production namespace in the cluster.
    Staging,
    /// Production.
    Prod,
}

impl Environment {
    /// Returns `true` in production.
    pub fn is_prod(self) -> bool {
        matches!(self, Self::Prod)
    }
}

/// Identity of the running service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Product name, used as a prefix in logs and metrics.
    pub name: String,
    /// Deployment environment.
    pub env: Environment,
    /// Git commit the binary was built from (`TAUT_APP__COMMIT`, set by the Dockerfile).
    pub commit: String,
}

/// HTTP server settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpSettings {
    /// Socket the server binds to.
    pub addr: SocketAddr,
    /// Comma-separated list of allowed CORS origins, or `*` for any origin.
    pub cors_origins: String,
    /// Maximum time a handler may take before the server answers `408 Request Timeout`.
    pub request_timeout_secs: u64,
}

impl HttpSettings {
    /// Allowed CORS origins as individual values, trimmed and without empty entries.
    pub fn cors_origins(&self) -> Vec<String> {
        self.cors_origins
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}

/// PostgreSQL settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSettings {
    /// Connection URL (`postgres://user:password@host:port/database`).
    pub url: String,
    /// Maximum number of pooled connections.
    pub max_connections: u32,
    /// Apply pending migrations when the service starts.
    pub migrate_on_start: bool,
}

/// Redis settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisSettings {
    /// Connection URL (`redis://host:port`).
    pub url: String,
}

/// NATS settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatsSettings {
    /// Connection URL (`nats://host:port`).
    pub url: String,
}

/// Log output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// One JSON object per line, for Filebeat and Elasticsearch.
    Json,
    /// Human-readable lines, for local development.
    Pretty,
}

/// Logging and metrics settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySettings {
    /// Log output format.
    pub log_format: LogFormat,
}

/// Settings for the checks that monitors run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecksSettings {
    /// Allow monitors that target loopback, private or link-local addresses. Off by default
    /// so a tenant cannot use taut to probe the cluster's internal network; `.env.example`
    /// turns it on so local development can monitor services on the same machine.
    pub allow_private_targets: bool,
    /// Region label of this checker, stored with every result (`local` in development).
    pub region: String,
    /// Checks one checker process runs at the same time.
    pub concurrency: usize,
}

/// Scheduler settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerSettings {
    /// Seconds between full reloads of the active monitors from the database. Events from
    /// the API apply changes immediately; the reload repairs any event that was missed.
    pub resync_secs: u64,
    /// Seconds the leader lock lives in Redis without renewal. Only the leader schedules.
    pub lock_ttl_secs: u64,
}

/// Notifier settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifierSettings {
    /// SMTP server: `smtp://host:1025` without TLS (Mailpit in development),
    /// `smtp://user:pass@host:587?tls=required` for STARTTLS or `smtps://user:pass@host:465`.
    /// Carries credentials in production, so it only comes from the environment.
    pub smtp_url: String,
    /// Sender of alert emails.
    pub email_from: String,
    /// Base URL of the panel, for links in alerts.
    pub panel_url: String,
}

/// Complete configuration of a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Service identity.
    pub app: AppSettings,
    /// HTTP server.
    pub http: HttpSettings,
    /// PostgreSQL.
    pub database: DatabaseSettings,
    /// Redis.
    pub redis: RedisSettings,
    /// NATS.
    pub nats: NatsSettings,
    /// Logging and metrics.
    pub telemetry: TelemetrySettings,
    /// Monitor checks.
    pub checks: ChecksSettings,
    /// Scheduler.
    pub scheduler: SchedulerSettings,
    /// Notifier.
    pub notifier: NotifierSettings,
}

impl Default for Settings {
    /// Development defaults matching `compose.yaml` and `.env.example`.
    fn default() -> Self {
        Self {
            app: AppSettings {
                name: "taut".to_owned(),
                env: Environment::Dev,
                commit: "unknown".to_owned(),
            },
            http: HttpSettings {
                addr: SocketAddr::from(([0, 0, 0, 0], 8080)),
                cors_origins: "http://localhost:5173".to_owned(),
                request_timeout_secs: 30,
            },
            database: DatabaseSettings {
                url: "postgres://taut:taut@localhost:5432/taut".to_owned(),
                max_connections: 10,
                migrate_on_start: true,
            },
            redis: RedisSettings {
                url: "redis://localhost:6379".to_owned(),
            },
            nats: NatsSettings {
                url: "nats://localhost:4222".to_owned(),
            },
            telemetry: TelemetrySettings {
                log_format: LogFormat::Pretty,
            },
            checks: ChecksSettings {
                allow_private_targets: false,
                region: "local".to_owned(),
                concurrency: 100,
            },
            scheduler: SchedulerSettings {
                resync_secs: 30,
                lock_ttl_secs: 10,
            },
            notifier: NotifierSettings {
                smtp_url: "smtp://localhost:1025".to_owned(),
                email_from: "taut <alertas@taut.localhost>".to_owned(),
                panel_url: "http://localhost:5173".to_owned(),
            },
        }
    }
}

/// Error returned when the environment does not describe a valid configuration.
#[derive(Debug, thiserror::Error)]
#[error("invalid configuration: {0}")]
pub struct ConfigError(#[source] Box<figment::Error>);

impl Settings {
    /// Environment variable that selects the deployment environment.
    pub const ENV_VAR: &'static str = "TAUT_APP__ENV";

    /// Loads the configuration from the environment (and `.env` outside production).
    pub fn load() -> Result<Self, ConfigError> {
        // The .env file is read only outside production, so a stray file inside a
        // container cannot override real settings. The check happens before dotenv.
        let is_prod =
            std::env::var(Self::ENV_VAR).is_ok_and(|value| value.eq_ignore_ascii_case("prod"));
        if !is_prod {
            dotenvy::dotenv().ok();
        }
        Self::figment()
            .extract()
            .map_err(|err| ConfigError(Box::new(err)))
    }

    /// The layered configuration sources, lowest priority first. Exposed for tests.
    pub fn figment() -> Figment {
        Figment::from(Serialized::defaults(Self::default()))
            .merge(
                Env::raw()
                    .only(&["DATABASE_URL"])
                    .map(|_| "database.url".into()),
            )
            .merge(Env::raw().only(&["REDIS_URL"]).map(|_| "redis.url".into()))
            .merge(Env::raw().only(&["NATS_URL"]).map(|_| "nats.url".into()))
            .merge(Env::prefixed("TAUT_").split("__"))
    }
}

#[cfg(test)]
// `Jail::expect_with` fixes the closure's error type to `figment::Error`, which is large.
#[allow(clippy::result_large_err)]
mod tests {
    use figment::Jail;

    use super::*;

    fn extract() -> figment::error::Result<Settings> {
        Settings::figment().extract()
    }

    #[test]
    fn defaults_are_valid() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            let settings = extract()?;
            assert_eq!(settings.app.env, Environment::Dev);
            assert_eq!(settings.http.addr.port(), 8080);
            assert!(settings.database.migrate_on_start);
            assert_eq!(settings.telemetry.log_format, LogFormat::Pretty);
            assert!(!settings.checks.allow_private_targets);
            Ok(())
        });
    }

    #[test]
    fn prefixed_env_overrides_defaults() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            jail.set_env("TAUT_APP__ENV", "staging");
            jail.set_env("TAUT_HTTP__ADDR", "127.0.0.1:9090");
            jail.set_env("TAUT_DATABASE__MAX_CONNECTIONS", "3");
            jail.set_env("TAUT_TELEMETRY__LOG_FORMAT", "json");
            let settings = extract()?;
            assert_eq!(settings.app.env, Environment::Staging);
            assert_eq!(settings.http.addr.to_string(), "127.0.0.1:9090");
            assert_eq!(settings.database.max_connections, 3);
            assert_eq!(settings.telemetry.log_format, LogFormat::Json);
            Ok(())
        });
    }

    #[test]
    fn plain_urls_are_read_without_prefix() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            jail.set_env("DATABASE_URL", "postgres://a:b@db:5432/x");
            jail.set_env("REDIS_URL", "redis://cache:6379");
            jail.set_env("NATS_URL", "nats://bus:4222");
            let settings = extract()?;
            assert_eq!(settings.database.url, "postgres://a:b@db:5432/x");
            assert_eq!(settings.redis.url, "redis://cache:6379");
            assert_eq!(settings.nats.url, "nats://bus:4222");
            Ok(())
        });
    }

    #[test]
    fn prefixed_url_wins_over_plain_one() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            jail.set_env("DATABASE_URL", "postgres://plain");
            jail.set_env("TAUT_DATABASE__URL", "postgres://prefixed");
            let settings = extract()?;
            assert_eq!(settings.database.url, "postgres://prefixed");
            Ok(())
        });
    }

    #[test]
    fn invalid_environment_is_an_error() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            jail.set_env("TAUT_APP__ENV", "banana");
            assert!(extract().is_err());
            Ok(())
        });
    }

    #[test]
    fn cors_origins_are_split_and_trimmed() {
        let http = HttpSettings {
            cors_origins: " http://a.test , http://b.test ,, ".to_owned(),
            ..Settings::default().http
        };
        assert_eq!(http.cors_origins(), vec!["http://a.test", "http://b.test"]);
    }
}
