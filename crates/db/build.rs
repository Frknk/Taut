//! Rebuilds the crate when a migration is added: `sqlx::migrate!` embeds the directory at
//! compile time, and without this a new file leaves stale migrations in cached builds.

fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
