//! Startup backup of the live Things SQLite.
//!
//! Uses the SQLite online-backup API via `rusqlite::backup` rather than
//! filesystem copy — safe to run while Things itself is writing.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{backup, Connection, OpenFlags};

pub struct Backup {
    pub path: PathBuf,
    pub bytes: u64,
}

pub fn snapshot(live_db: &Path, backup_dir: &Path) -> anyhow::Result<Backup> {
    std::fs::create_dir_all(backup_dir)?;
    let stamp = utc_stamp();
    let out = backup_dir.join(format!("things-{stamp}.sqlite"));

    let src = Connection::open_with_flags(
        live_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut dst = Connection::open(&out)?;
    let backup = backup::Backup::new(&src, &mut dst)?;
    backup.run_to_completion(64, std::time::Duration::from_millis(10), None)?;
    drop(backup);
    drop(dst);
    drop(src);

    let bytes = std::fs::metadata(&out)?.len();
    Ok(Backup { path: out, bytes })
}

pub fn rotate(backup_dir: &Path, retain: u32) -> anyhow::Result<usize> {
    if !backup_dir.exists() {
        return Ok(0);
    }
    let mut entries: Vec<_> = std::fs::read_dir(backup_dir)?
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name().to_string_lossy().starts_with("things-")
                && e.file_name().to_string_lossy().ends_with(".sqlite")
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    // oldest first; keep the newest `retain`
    let drop_n = entries.len().saturating_sub(retain as usize);
    for entry in entries.iter().take(drop_n) {
        std::fs::remove_file(entry.path())?;
    }
    Ok(drop_n)
}

fn utc_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = __test_only_unix_to_ymdhms(secs as i64);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// Decompose Unix epoch seconds into (year, month, day, hour, minute, second).
/// Pure UTC. Year ≥ 1970 assumed; negatives are clamped to 0 (i.e. 1970-01-01).
/// Exposed via this name so other modules can render ISO-8601 timestamps
/// without pulling in `chrono`.
pub(crate) fn __test_only_unix_to_ymdhms(unix_secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let unix_secs = unix_secs.max(0) as u64;
    let secs = unix_secs as i64;
    let s = secs.rem_euclid(60) as u32;
    let m_total = secs.div_euclid(60);
    let mi = m_total.rem_euclid(60) as u32;
    let h_total = m_total.div_euclid(60);
    let h = h_total.rem_euclid(24) as u32;
    let mut days = h_total.div_euclid(24);
    // 1970-01-01 was a Thursday; compute date by stepping years and months.
    let mut y: i32 = 1970;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
        let year_days = if leap { 366 } else { 365 };
        if days < year_days {
            break;
        }
        days -= year_days;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
    let months_len = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo: u32 = 1;
    for len in months_len.iter() {
        if days < *len {
            break;
        }
        days -= *len;
        mo += 1;
    }
    let d = (days as u32) + 1;
    (y, mo, d, h, mi, s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn snapshot_copies_a_sqlite_file() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("live.sqlite");
        {
            let c = Connection::open(&src).unwrap();
            c.execute_batch("CREATE TABLE t(x INTEGER); INSERT INTO t VALUES (42);")
                .unwrap();
        }
        let dir = tmp.path().join("backups");
        let backup = snapshot(&src, &dir).unwrap();
        assert!(backup.path.exists());
        assert!(backup.bytes > 0);
        // verify the copy is a valid SQLite with the row
        let c = Connection::open(&backup.path).unwrap();
        let v: i64 = c.query_row("SELECT x FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 42);
    }

    #[test]
    fn rotate_keeps_only_n_newest() {
        let tmp = tempdir().unwrap();
        for i in 0..5 {
            let name = format!("things-2026010{i}-000000.sqlite");
            std::fs::write(tmp.path().join(&name), b"").unwrap();
        }
        let dropped = rotate(tmp.path(), 2).unwrap();
        assert_eq!(dropped, 3);
        let kept: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().any(|n| n.contains("20260103")));
        assert!(kept.iter().any(|n| n.contains("20260104")));
    }
}
