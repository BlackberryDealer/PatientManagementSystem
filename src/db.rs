use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use log::info;

/// Create a connection pool for the SQLite database.
pub async fn create_pool(database_url: &str) -> SqlitePool {
    info!("Connecting to database: {}", database_url);
    SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .expect("Failed to create database pool. Is the DATABASE_URL correct?")
}

/// Run all pending SQLx migrations from the `migrations/` directory.
pub async fn run_migrations(pool: &SqlitePool) {
    info!("Running database migrations...");
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run database migrations. Check your migration SQL files.");
    info!("Database migrations completed successfully.");
}
