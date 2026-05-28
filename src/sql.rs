use std::{path::Path, sync::LazyLock};

use anyhow::Context as _;
use include_dir::Dir;
use rusqlite::{Connection, OpenFlags};
use rusqlite_migration::{M, Migrations};

include!(concat!(env!("OUT_DIR"), "/sql/out/generated.rs"));

static MIGRATIONS_DIR: Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/sql/migrations");

static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
    let mut ms = MIGRATIONS_DIR.files().collect::<Vec<_>>();
    ms.sort_by(|a, b| a.path().cmp(b.path()));
    let ms = ms
        .into_iter()
        .map(|it| it.contents_utf8().expect("sql stmt"))
        .map(M::up)
        .collect::<Vec<_>>();
    Migrations::new(ms)
});

/// # Errors
///
/// `Err` if `path` cannot be converted to a C-compatible string
/// or if the underlying `rusqlite`/`rusqlite_migration` call fails.
pub fn db_open(
    path: impl AsRef<Path>,
    flags: OpenFlags,
) -> anyhow::Result<Connection> {
    let mut conn = Connection::open_with_flags(path, flags)
        .context("Connection::open_with_flags")?;

    conn.pragma_update(None, "journal_mode", "WAL")
        .context("PRAGMA journal_mode = WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .context("PRAGMA foreign_keys = ON")?;

    MIGRATIONS
        .to_latest(&mut conn)
        .context("MIGRATIONS.to_latest")?;
    Ok(conn)
}

#[derive(Debug, derive_more::Deref)]
pub struct Transaction {
    conn: Connection,
}

impl Transaction {
    /// wrap the connection as if it's already in a transaction
    pub const fn wrap(conn: Connection) -> Self {
        Self { conn }
    }

    /// # Errors
    ///
    /// if underlying rusqlite call fails.
    pub fn rollback(&self) -> rusqlite::Result<()> {
        let _ = self.conn.execute("ROLLBACK", ())?;
        Ok(())
    }

    /// # Errors
    ///
    /// if underlying rusqlite call fails.
    pub fn commit(&self) -> rusqlite::Result<()> {
        let _ = self.conn.execute("COMMIT", ())?;
        Ok(())
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        let _ = self.rollback();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn validate() {
        super::MIGRATIONS.validate().unwrap();
    }
}
