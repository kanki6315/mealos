use std::{str::FromStr, time::Duration};

use chrono::{DateTime, Utc};
use sqlx::{
    PgPool,
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
};

use crate::Error;

pub const MIGRATOR: Migrator = sqlx::migrate!();

pub fn pool(url: &str) -> Result<PgPool, sqlx::Error> {
    let options = PgConnectOptions::from_str(url)?.options([
        ("statement_timeout", "10000"),
        ("idle_in_transaction_session_timeout", "10000"),
    ]);
    Ok(PgPoolOptions::new()
        .max_connections(4)
        .min_connections(0)
        .idle_timeout(Duration::from_secs(10))
        .max_lifetime(None)
        .acquire_timeout(Duration::from_secs(5))
        .connect_lazy_with(options))
}

// Same ledger/checksum format as the initial Node prototype; never reapply an
// already-recorded migration or silently change its SQL.
pub async fn migrate(pool: &PgPool) -> Result<(), Error> {
    let legacy_exists = sqlx::query_scalar!(r#"SELECT EXISTS(SELECT FROM information_schema.tables WHERE table_name = 'schema_migrations') AS "legacy_exists!""#).fetch_one(pool).await?;

    if legacy_exists {
        let mut tx = pool.begin().await?;

        sqlx::query!(
            r#"
CREATE TABLE IF NOT EXISTS _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
    success BOOLEAN NOT NULL,
    checksum BYTEA NOT NULL,
    execution_time BIGINT NOT NULL
);
"#
        )
        .execute(&mut *tx)
        .await?;

        for m in MIGRATOR.iter() {
            let legacy_name = format!("{:03}_{}.sql", m.version, m.description.replace(' ', "_"));

            // The one exception to using macros. `schema_migrations` shouldn't be present anymore
            let applied = sqlx::query_scalar::<_, DateTime<Utc>>(
                r#"SELECT applied_at FROM schema_migrations WHERE name = $1"#,
            )
            .bind(&legacy_name)
            .fetch_optional(&mut *tx)
            .await?;

            if let Some(applied_at) = applied {
                sqlx::query!(
                    r#"
INSERT INTO _sqlx_migrations(version, description, installed_on, success, checksum, execution_time)
VALUES ($1, $2, $3, TRUE, $4, 0)
ON CONFLICT DO NOTHING
"#,
                    m.version,
                    &*m.description,
                    applied_at,
                    &*m.checksum
                )
                .execute(&mut *tx)
                .await?;
            }
        }

        sqlx::query!("DROP TABLE schema_migrations")
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
    }

    MIGRATOR.run(pool).await.map_err(|_| {
        Error(
            axum::http::StatusCode::CONFLICT,
            "Migration failed or modified",
        )
    })
}
