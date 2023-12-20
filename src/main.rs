use anyhow::Result;
use clap::Parser;
use tokio::time::{sleep, Duration};
use tracing::{info, error};

mod models;

use crate::models::Entries;

#[derive(Parser, Debug)]
struct Args {
    /// The fully qualified URL to the Miniflux server
    server: String,

    #[clap(long, env)]
    miniflux_api_key: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt::init();

    let server_check = check_for_server_existence(&args.server).await;
    if !server_check.unwrap_or(false) {
        eprintln!(
            "Server was not found! Make sure it is running and the specified URL is correct."
        );
    }

    loop {
        let unread_entries = get_unread_entries(&args.server, &args.miniflux_api_key).await;

        if unread_entries.is_err() {
            error!("Failed to get unread entries!\n\t{:?}", unread_entries);
            sleep(Duration::new(60, 0)).await;
            continue;
        }
        sleep(Duration::new(5, 0)).await;
    }
}

/// Run a simple healthcheck on the provided server
///
/// This serves two purposes:
/// 1. Handling invalid URLs that a user might provide
/// 2. Checking that the server is available
#[tracing::instrument(skip(server))]
async fn check_for_server_existence(server: &String) -> Result<bool> {
    info!("Checking for server existence");
    reqwest::get(format!("{}/healthcheck", server))
        .await?
        .error_for_status()?;
    Ok(true)
}

#[tracing::instrument(skip(server, auth_token))]
async fn get_unread_entries(server: &String, auth_token: &String) -> Result<Entries> {
    let client = reqwest::Client::new();
    let entries = client
        .get(format!(
            "{}/v1/entries?status=unread&direction=desc",
            server
        ))
        .header("X-Auth-Token", auth_token)
        .send()
        .await?
        .error_for_status()?
        .json::<Entries>()
        .await?;
    info!("Found {} unread entries",  &entries.total);
    Ok(entries)
}
