//! Structs representing the various response playloads the Miniflux API might respond with

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Entries {
    pub total: u32,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
pub struct Entry {
    pub id: u32,
    pub title: String,
}
