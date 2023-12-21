# Miniflux Notifier

Get desktop notifications at select intervals about new, unread articles from a Miniflux server.

The program is a simple daemon for linux that can be run on a per-user basis.

## Building

Make sure you have the rust toolchain installed (see [rustup.rs](https://rustup.rs)).

```
cargo build --release
```

The build binary is a single file located at `target/release/miniflux-notifier`.
