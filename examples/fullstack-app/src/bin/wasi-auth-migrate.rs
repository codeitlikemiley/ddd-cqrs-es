//! Native PostgreSQL migration entrypoint for the generated application.

#[cfg(feature = "migrate")]
use std::process::ExitCode;

#[cfg(feature = "migrate")]
use wasi_auth::schema::native::{MigrationAction, MigrationRunner};

#[cfg(feature = "migrate")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match execute().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("wasi-auth migration failed: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(feature = "migrate")]
async fn execute() -> Result<(), Box<dyn std::error::Error>> {
    let action = match std::env::args().nth(1).as_deref() {
        Some("apply") => MigrationAction::Apply,
        Some("plan") => MigrationAction::Plan,
        Some("verify") => MigrationAction::Verify,
        Some("verify-database") => MigrationAction::VerifyDatabase,
        Some("status") | None => MigrationAction::Status,
        Some(command) => return Err(format!("unknown migration command {command}").into()),
    };
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL is required and is never accepted as a CLI argument")?;
    let report = MigrationRunner::run(&database_url, action).await?;
    apply_or_verify_app_schema(&database_url, action).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(feature = "migrate")]
async fn apply_or_verify_app_schema(
    database_url: &str,
    action: MigrationAction,
) -> Result<(), Box<dyn std::error::Error>> {
    const VERSION: &str = "0001_app_storage";
    const MIGRATION: &str = include_str!("../../migrations/postgres/0001_app_storage.sql");
    let (mut client, connection) =
        tokio_postgres::connect(database_url, tokio_postgres::NoTls).await?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("app schema connection failed: {error}");
        }
    });

    if action == MigrationAction::Apply {
        let transaction = client.transaction().await?;
        transaction.batch_execute(MIGRATION).await?;
        transaction.commit().await?;
    }

    let applied = client
        .query_opt(
            "SELECT version FROM fullstack_app.schema_migrations WHERE version = $1",
            &[&VERSION],
        )
        .await;
    match (action, applied) {
        (MigrationAction::Plan | MigrationAction::Status, Err(_)) => {
            eprintln!("pending app migration: {VERSION}");
            Ok(())
        }
        (_, Ok(Some(_))) => Ok(()),
        (_, Ok(None)) => {
            Err(format!("fullstack app schema has pending migration: {VERSION}").into())
        }
        (_, Err(error)) => Err(format!("fullstack app schema is unavailable: {error}").into()),
    }
}

#[cfg(not(feature = "migrate"))]
fn main() {
    eprintln!("rebuild this binary with --features migrate");
}
