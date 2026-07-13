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
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(not(feature = "migrate"))]
fn main() {
    eprintln!("rebuild this binary with --features migrate");
}
