use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::OnceLock;
use tracing::{error, info};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub credentials: Credentials,
    pub server: Server,
    pub service: Service,
    pub database: Database,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Credentials {
    pub user: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Database {
    pub user: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub sid: String,
    #[serde(default = "Database::default_pool_min")]
    pub pool_min: u32,
    #[serde(default = "Database::default_pool_max")]
    pub pool_max: u32,
}

impl Database {
    fn default_pool_min() -> u32 {
        1
    }
    fn default_pool_max() -> u32 {
        5
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Server {
    pub host: String,
    pub port: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Service {
    pub interval: u64,
    pub throttle: u64,
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.credentials.user.trim().is_empty() {
            return Err("The 'user' in [config.credentials] cannot be empty.".to_string());
        }
        if self.credentials.password.trim().is_empty() {
            return Err("The 'password' in [config.credentials] cannot be empty.".to_string());
        }
        if self.database.user.trim().is_empty() {
            return Err("The 'user' in [config.database] cannot be empty.".to_string());
        }
        if self.database.password.trim().is_empty() {
            return Err("The 'password' in [config.database] cannot be empty.".to_string());
        }
        if self.database.host.trim().is_empty() {
            return Err("The 'host' in [config.database] cannot be empty.".to_string());
        }
        if self.database.sid.trim().is_empty() {
            return Err("The 'sid' in [config.database] cannot be empty.".to_string());
        }
        if self.server.host.trim().is_empty() {
            return Err("The 'host' in [config.server] cannot be empty.".to_string());
        }
        if self.server.port == 0 {
            return Err("The 'port' in [config.server] must be a positive integer.".to_string());
        }
        if self.database.pool_min > self.database.pool_max {
            return Err(format!(
                "pool_min ({}) must be <= pool_max ({}) in [config.database].",
                self.database.pool_min, self.database.pool_max
            ));
        }
        Ok(())
    }
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn get() -> &'static Config {
    CONFIG
        .get()
        .expect("config::init() must be called before config::get()")
}

pub fn init() -> anyhow::Result<()> {
    let path = std::path::Path::new("kroma.toml");

    if !path.is_file() {
        anyhow::bail!(
            "Config file not found in '{}'. Create the file with the credentials before starting.",
            path.display()
        );
    }

    info!("Config file found in {}", path.display());

    let config_file = fs::read_to_string(path)
        .with_context(|| format!("Error reading config file in {}", path.display()))?;

    let config: Config = toml::from_str(&config_file).with_context(|| {
        format!(
            "Syntax error or missing required field in TOML ({})",
            path.display()
        )
    })?;

    if let Err(err_msg) = config.validate() {
        error!("Invalid config in TOML file: {}", err_msg);
        anyhow::bail!(
            "Invalid config in TOML file ({}): {}",
            path.display(),
            err_msg
        );
    }

    CONFIG
        .set(config)
        .map_err(|_| anyhow::anyhow!("config::init() was called more than once"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> Config {
        Config {
            credentials: Credentials {
                user: "user@example.com".into(),
                password: "password123".into(),
            },
            server: Server {
                host: "smtp.example.com".into(),
                port: 587,
            },
            service: Service {
                interval: 60,
                throttle: 100,
            },
            database: Database {
                user: "oracle_user".into(),
                password: "oracle_pass".into(),
                host: "oracle.example.com".into(),
                port: 1521,
                sid: "ORCL".into(),
                pool_min: 1,
                pool_max: 5,
            },
        }
    }

    #[test]
    fn test_valid_config() {
        let cfg = valid_config();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_empty_credentials_user() {
        let mut cfg = valid_config();
        cfg.credentials.user = "   ".into();
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("[config.credentials]"));
    }

    #[test]
    fn test_empty_credentials_password() {
        let mut cfg = valid_config();
        cfg.credentials.password = "".into();
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("[config.credentials]"));
    }

    #[test]
    fn test_empty_database_fields() {
        let mut cfg = valid_config();
        cfg.database.user = "".into();
        assert!(cfg.validate().is_err());

        let mut cfg = valid_config();
        cfg.database.password = "".into();
        assert!(cfg.validate().is_err());

        let mut cfg = valid_config();
        cfg.database.host = "".into();
        assert!(cfg.validate().is_err());

        let mut cfg = valid_config();
        cfg.database.sid = "".into();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_invalid_server_fields() {
        let mut cfg = valid_config();
        cfg.server.host = "  ".into();
        assert!(cfg.validate().is_err());

        let mut cfg = valid_config();
        cfg.server.port = 0;
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("positive integer"));
    }

    #[test]
    fn test_pool_min_greater_than_pool_max() {
        let mut cfg = valid_config();
        cfg.database.pool_min = 10;
        cfg.database.pool_max = 5;
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("pool_min (10) must be <= pool_max (5)"));
    }

    #[test]
    fn test_deserialize_toml_with_defaults() {
        let toml_str = r#"
            [credentials]
            user = "test@example.com"
            password = "secret"

            [server]
            host = "smtp.example.com"
            port = 465

            [service]
            interval = 30
            throttle = 50

            [database]
            user = "dbuser"
            password = "dbpassword"
            host = "localhost"
            port = 1521
            sid = "XE"
        "#;
        let cfg: Config = toml::from_str(toml_str).expect("failed to deserialize config");
        assert_eq!(cfg.database.pool_min, 1);
        assert_eq!(cfg.database.pool_max, 5);
        assert!(cfg.validate().is_ok());
    }
}
