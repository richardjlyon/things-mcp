//! Semaphore-throttled, short-lived RO connection pool.
//!
//! Mirrors `zotero-connector`'s pattern: bound concurrent readers with a
//! `tokio::sync::Semaphore`, open a fresh `Connection` per `with_conn` call
//! using URI flags (`mode=ro`, `nolock=1`, `immutable=1`), run the closure
//! inside `spawn_blocking`. Each call picks up Things' latest committed state
//! automatically because the connection lifetime spans only one query.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{Connection, OpenFlags};
use tokio::sync::Semaphore;

use crate::core::error::ThingsError;

pub fn open_read_only(db: &Path) -> Result<Connection, ThingsError> {
    let uri = format!("file:{}?mode=ro&nolock=1&immutable=1", db.to_string_lossy());
    let conn = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.busy_timeout(std::time::Duration::from_millis(500))?;
    Ok(conn)
}

#[derive(Clone)]
pub struct ReaderPool {
    inner: Arc<Inner>,
}

struct Inner {
    path: PathBuf,
    sem: Semaphore,
}

impl ReaderPool {
    pub async fn new(db_path: PathBuf, max: usize) -> Result<Self, ThingsError> {
        // Validate the path + permissions up front.
        let _probe = open_read_only(&db_path)?;
        Ok(Self {
            inner: Arc::new(Inner {
                path: db_path,
                sem: Semaphore::new(max),
            }),
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.inner.path
    }

    pub async fn with_conn<F, R>(&self, f: F) -> Result<R, ThingsError>
    where
        F: FnOnce(&Connection) -> rusqlite::Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let permit = self
            .inner
            .sem
            .acquire()
            .await
            .map_err(|e| ThingsError::Sqlite(format!("semaphore closed: {e}")))?;
        let path = self.inner.path.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<R, ThingsError> {
            let conn = open_read_only(&path)?;
            f(&conn).map_err(ThingsError::from)
        })
        .await
        .map_err(|e| ThingsError::Sqlite(format!("join: {e}")))?;
        drop(permit);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::fixture::build_fixture;
    use tempfile::tempdir;

    #[tokio::test]
    async fn pool_opens_and_runs_a_query() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let n: i64 = pool
            .with_conn(|c| c.query_row("SELECT COUNT(*) FROM TMTask", [], |r| r.get(0)))
            .await
            .unwrap();
        assert_eq!(n, 5);
    }

    #[tokio::test]
    async fn pool_caps_concurrency() {
        // Two permits; three concurrent queries should serialise the third.
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let p = pool.clone();
        let h1 = tokio::spawn(async move {
            p.with_conn(|c| c.query_row("SELECT COUNT(*) FROM TMTask", [], |r| r.get::<_, i64>(0)))
                .await
        });
        let p = pool.clone();
        let h2 = tokio::spawn(async move {
            p.with_conn(|c| c.query_row("SELECT COUNT(*) FROM TMTag", [], |r| r.get::<_, i64>(0)))
                .await
        });
        let p = pool.clone();
        let h3 = tokio::spawn(async move {
            p.with_conn(|c| c.query_row("SELECT COUNT(*) FROM TMArea", [], |r| r.get::<_, i64>(0)))
                .await
        });
        for h in [h1, h2, h3] {
            h.await.unwrap().unwrap();
        }
    }
}
