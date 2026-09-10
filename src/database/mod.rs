use anyhow::{Context, Result};
use sibyl::SessionPool;
use std::sync::OnceLock;

static ORACLE_POOL: OnceLock<SessionPool<'static>> = OnceLock::new();

pub async fn init_pool() -> Result<()> {
    let env = match sibyl::env() {
        // SAFETY: We intentionally leak the OCI environment to obtain a `&'static` reference.
        // `ORACLE_POOL` is a `OnceLock<SessionPool<'static>>`, which requires the env to outlive
        // the pool for the entire process lifetime. Since this service runs until the process exits
        // and there is no clean-shutdown path that would need to reclaim the env, leaking it here
        // is safe and correct. The env is always successfully initialised before any pool usage.
        Ok(env) => Box::leak(Box::new(env)),
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to initialize Oracle OCI environment: {}",
                e
            ));
        }
    };

    let cfg = &crate::config::get().database;
    let db = format!("{}:{}/{}", cfg.host, cfg.port, cfg.sid);

    let pool = env
        .create_session_pool(&db, &cfg.user, &cfg.password, cfg.pool_min as usize, 1, cfg.pool_max as usize)
        .await
        .with_context(|| {
            format!(
                "Failed to create Oracle session pool for user '{}' on '{}:{}/{}'",
                cfg.user, cfg.host, cfg.port, cfg.sid
            )
        })?;

    ORACLE_POOL
        .set(pool)
        .map_err(|_| anyhow::anyhow!("database::init_pool() was called more than once"))?;

    Ok(())
}

pub fn get_pool() -> &'static SessionPool<'static> {
    ORACLE_POOL
        .get()
        .expect("database::init_pool() must be called before get_pool()")
}
