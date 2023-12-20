use anyhow::Result;
use clap::Parser;
use notify_rust::Notification;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

mod models;

use crate::models::{Entries, Entry};

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
    let mut entries_cache: Option<Entries> = None;

    tracing_subscriber::fmt::init();

    let server_check = check_for_server_existence(&args.server).await;
    if !server_check.unwrap_or(false) {
        eprintln!(
            "Server was not found! Make sure it is running and the specified URL is correct."
        );
    }

    loop {
        sleep(Duration::new(60, 0)).await;

        let unread_entries = get_unread_entries(&args.server, &args.miniflux_api_key).await;

        if unread_entries.is_err() {
            error!("Failed to get unread entries!\n\t{:?}", unread_entries);
            continue;
        }

        let unread_entries = unread_entries.unwrap();

        // Don't consider any "new" entries when there is no cache
        if let Some(entries_cache) = entries_cache {
            let first_new_index = find_new_entries(&entries_cache.entries, &unread_entries.entries);

            if let Ok(first_new_index) = first_new_index {
                if first_new_index != 0 {
                    let nb =
                        send_notification_batch(&unread_entries.entries[0..first_new_index + 1]);
                    if nb.is_err() {
                        error!("{:?}", nb);
                    }
                }
            } else {
                error!("{:?}", first_new_index);
            }
        }

        entries_cache = Some(unread_entries);
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
    info!("Found {} unread entries", &entries.total);
    Ok(entries)
}

/// Finds the index in a list of entries to the first "new" entry.
///
/// We want to figure out which entries are "new". To do this, we start with the first
/// entry in the cache and look through the list of unread entries. If we find a match,
/// we can note where the match is and know that all of the entries that came before it
/// are "new". If we don't find a match, we repeat the process using the second entry in
/// the cache and continue in this manner. If every entry in the cache has been
/// exhausted, then everything in the unread entry list is "new".
fn find_new_entries(cache: &Vec<Entry>, new: &Vec<Entry>) -> Result<usize> {
    if new.is_empty() {
        return Err(anyhow::anyhow!(
            "No entries provided when searching for new entries"
        ));
    }

    for cached_entry in cache {
        for (i, new_entry) in new.iter().enumerate() {
            // Found first "new" entry
            if cached_entry.hash == new_entry.hash {
                return Ok(i);
            }
        }
    }

    // Everything must be "new" then
    Ok(0)
}

fn send_notification_batch(entries: &[Entry]) -> Result<()> {
    for entry in entries {
        Notification::new()
            .summary(format!("New RSS Entry from {}", &entry.author).as_str())
            .body(&entry.title)
            .show()?;
    }

    Ok(())
}
