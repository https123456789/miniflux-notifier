use anyhow::Result;
use clap::Parser;
use futures::future::join_all;
use futures::FutureExt;
use notify_rust::Notification;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info};

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
        if entries_cache.is_some() {
            sleep(Duration::new(10, 0)).await;
        }

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
                        send_notification_batch(&unread_entries.entries[0..first_new_index + 1])
                            .await;
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
#[tracing::instrument(skip(cache, new))]
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
                return Ok(1);
            }
        }
    }

    // Everything must be "new" then
    Ok(0)
}

#[tracing::instrument(skip(entries))]
async fn send_notification_batch(entries: &[Entry]) -> Result<()> {
    debug!("Got {} entries", entries.len());
    let mut notifs = vec![];
    let mut tasks = vec![];

    // Due to lifetime constraints, we must create a vector of all the notifications first before
    // we can show then and await any potential actions
    for entry in entries {
        let source = match &entry.author.is_empty() {
            true => &entry.feed.title,
            false => &entry.author,
        };
        let notif = Notification::new()
            .summary(format!("New RSS Entry from {}", source).as_str())
            .body(&entry.title)
            .action("open", "Open in web browser")
            .finalize();
        notifs.push(notif);
    }

    // Now we can iterate through all of the notifications, show them and setup any handlers for
    // actions
    for (i, notif) in notifs.iter().enumerate() {
        let entry = Box::new(&entries[i]);
        let task = notif.show_async()
            .then(|handle| async move {
                debug!("Notif shown");
                match handle {
                    Ok(handle) => {
                        debug!("^Action");
                        handle.wait_for_action(move |action| {
                            debug!("Action");
                            #[allow(clippy::single_match)]
                            let res = match action {
                                "open" => open::that_detached(&entry.url),
                                _ => Ok(())
                            };

                            debug!("Action~");

                            if let Err(e) = res {
                                error!("{:?}", e);
                            }
                        });
                        Ok(())
                    },
                    Err(e) => Err(e)
                }
            });
        tasks.push(task);
    }

    for result in join_all(tasks).await {
        if let Err(e) = result {
            error!("{:?}", e);
        }
    }

    debug!("exiting");

    Ok(())
}
