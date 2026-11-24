//! When the server scans each directory: the rule the scheduler reads, and
//! that `next_scan_at` reads to tell the page.
//!
//! A directory's stored `scan_interval_secs` is NULL for Automatic, 0 for
//! Off, and otherwise the interval Every was given
//! (`docs/superpowers/specs/2026-11-10-pre-deploy-design.md`, §4, "The
//! setting").

use std::collections::HashMap;

use rusqlite::Connection;

use crate::scan;

/// The shortest interval a directory is scanned at.
///
/// Each scan holds the job lock through a whole-store derive, so a shorter
/// interval would keep rule saves, stamps and moves refused for most of the
/// time (spec §4, Decision 3). The server refuses a shorter one, and a
/// shorter one already in the store is read as this.
pub const MIN_INTERVAL_SECS: i64 = 300;

/// The interval a directory is scanned at, in seconds, from its stored
/// `scan_interval_secs`; `None` for Off.
///
/// **Automatic is the unwatched interval** while nothing watches a directory:
/// a scan is then the only way a change is seen (spec §4, "The setting").
/// A negative value, which the server refuses, is read as Off, so that a
/// value nobody chose never starts a scan.
#[must_use]
pub fn every(stored: Option<i64>) -> Option<i64> {
    match stored {
        None => Some(crate::watch::UNWATCHED_INTERVAL.as_secs().cast_signed()),
        Some(secs) if secs <= 0 => None,
        Some(secs) => Some(secs.max(MIN_INTERVAL_SECS)),
    }
}

/// `last_scan_at` plus [`every`]`(stored)`, in SQLite's `datetime()` shape,
/// `YYYY-MM-DD HH:MM:SS`, UTC: `None` for Off, for a directory never
/// scanned, and for a stamp not in that shape.
///
/// **Computed here, from the row alone**, because `root_json`, which sends
/// it, holds no connection to ask SQLite's `datetime()`. A time past the
/// shape's last year is that year's last second, so a huge interval reads as
/// never due rather than as a stamp that does not parse.
#[must_use]
pub fn next_scan_at(last_scan_at: Option<&str>, stored: Option<i64>) -> Option<String> {
    let every = every(stored)?;
    let at = Stamp::parse(last_scan_at?)?;
    Some(at.plus(every).to_string())
}

/// A directory the scheduler scans, and whether its time has come.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scheduled {
    pub root: scan::Root,
    pub due: bool,
}

/// Every directory that is not Off or that a scan job holds, by path, as
/// [`crate::reach::walked_by`] sorts, so a directory is walked before one
/// nested in it. One is due where it is not Off and [`next_scan_at`] is `None`
/// or is at or before `now`.
///
/// **A paused directory is left out** however due or held: a cancelled job
/// paused it, and only a pressed scan resumes it ([`scan::pause`]).
///
/// **Compared as text**: `now` ([`now`]) and `next_scan_at` share one
/// fixed-width `datetime()` shape, so text order is time order.
///
/// # Errors
///
/// If the store refuses a read.
pub fn scheduled(conn: &Connection, now: &str) -> Result<Vec<Scheduled>, String> {
    // A directory removed or rescanned between the two reads would otherwise
    // be listed with another state's last scan.
    let _snapshot = crate::one_snapshot(conn)
        .map_err(|e| format!("starting a read of the scan schedule: {e}"))?;
    let roots = scan::roots(conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, last_scan_at, paused_at IS NOT NULL, scan_held_at IS NOT NULL FROM root",
        )
        .map_err(|e| format!("reading when each directory was last scanned: {e}"))?;
    let last: HashMap<i64, (Option<String>, bool, bool)> = stmt
        .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))
        .map_err(|e| format!("reading when each directory was last scanned: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("reading when each directory was last scanned: {e}"))?;
    let paused = |root: &scan::Root| last.get(&root.id).is_some_and(|(_, paused, _)| *paused);
    let held = |root: &scan::Root| last.get(&root.id).is_some_and(|(_, _, held)| *held);
    // A held directory is listed whatever its schedule: at the server's start
    // its mark is a scan a stop cut ([`scan::mark_held`]), which the startup
    // tick takes up, as it takes every directory listed (Design O314 §5, ruling
    // 9). Never due by it, so no later tick takes a held Off directory, while
    // its own job holds it or after a panic left the mark.
    let mut listed: Vec<Scheduled> = roots
        .into_iter()
        .filter(|root| (every(root.scan_interval_secs).is_some() || held(root)) && !paused(root))
        .map(|root| {
            let last_scan_at = last.get(&root.id).and_then(|(at, _, _)| at.as_deref());
            // `next_scan_at` is `None` for Off as for a directory never
            // scanned, and only the second is due.
            let due = every(root.scan_interval_secs).is_some()
                && next_scan_at(last_scan_at, root.scan_interval_secs)
                    .is_none_or(|next| next.as_str() <= now);
            Scheduled { root, due }
        })
        .collect();
    listed.sort_by(|a, b| a.root.path.cmp(&b.root.path));
    Ok(listed)
}

/// One directory's `(id, last_scan_at, last_scan_status)`.
pub type ScanMark = (i64, Option<String>, Option<String>);

/// Every directory's [`ScanMark`], by id: what a walk writes as it ends, in
/// this process or another.
///
/// # Errors
///
/// If the store refuses the read.
pub fn scan_marks(conn: &Connection) -> rusqlite::Result<Vec<ScanMark>> {
    let mut stmt =
        conn.prepare("SELECT id, last_scan_at, last_scan_status FROM root ORDER BY id")?;
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect()
}

/// The store's clock, the one that writes `last_scan_at`, in its
/// `datetime()` shape.
///
/// # Errors
///
/// If the store refuses the read.
pub fn now(conn: &Connection) -> Result<String, String> {
    conn.query_row("SELECT datetime('now')", [], |r| r.get(0))
        .map_err(|e| format!("reading the store's clock: {e}"))
}

/// One `YYYY-MM-DD HH:MM:SS` time on the proleptic Gregorian calendar, as
/// SQLite's `datetime()` reads and writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    year: i64,
    month: i64,
    day: i64,
    second_of_day: i64,
}

const SECONDS_A_DAY: i64 = 24 * 60 * 60;

/// The last year the shape's four digits can write.
const LAST_YEAR: i64 = 9999;

impl Stamp {
    fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 19
            || bytes[4] != b'-'
            || bytes[7] != b'-'
            || bytes[10] != b' '
            || bytes[13] != b':'
            || bytes[16] != b':'
        {
            return None;
        }
        let number = |from: usize, to: usize| -> Option<i64> {
            let digits = &bytes[from..to];
            digits.iter().all(u8::is_ascii_digit).then(|| {
                digits
                    .iter()
                    .fold(0, |n, digit| n * 10 + i64::from(digit - b'0'))
            })
        };
        let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
        let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
        if !(1..=12).contains(&month)
            || !(1..=days_in_month(year, month)).contains(&day)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return None;
        }
        Some(Self {
            year,
            month,
            day,
            second_of_day: hour * 3600 + minute * 60 + second,
        })
    }

    /// This time `seconds` later, which are not negative.
    fn plus(self, seconds: i64) -> Self {
        let Some(total) = self.second_of_day.checked_add(seconds) else {
            return Self::LAST;
        };
        let second_of_day = total % SECONDS_A_DAY;
        // Counted from the first of January, a whole year at a time, then a
        // month. The year bound keeps a huge interval to a few thousand steps.
        let mut days = total / SECONDS_A_DAY
            + (1..self.month)
                .map(|month| days_in_month(self.year, month))
                .sum::<i64>()
            + self.day
            - 1;
        let mut year = self.year;
        while year <= LAST_YEAR && days >= days_in_year(year) {
            days -= days_in_year(year);
            year += 1;
        }
        if year > LAST_YEAR {
            return Self::LAST;
        }
        let mut month = 1;
        while days >= days_in_month(year, month) {
            days -= days_in_month(year, month);
            month += 1;
        }
        Self {
            year,
            month,
            day: days + 1,
            second_of_day,
        }
    }

    const LAST: Self = Self {
        year: LAST_YEAR,
        month: 12,
        day: 31,
        second_of_day: SECONDS_A_DAY - 1,
    };
}

impl std::fmt::Display for Stamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year,
            self.month,
            self.day,
            self.second_of_day / 3600,
            self.second_of_day / 60 % 60,
            self.second_of_day % 60
        )
    }
}

fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_year(year: i64) -> i64 {
    if is_leap(year) { 366 } else { 365 }
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests;
