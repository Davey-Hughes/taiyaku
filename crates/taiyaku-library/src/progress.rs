//! What a scan reports while it runs, and how its caller stops it.
//!
//! The walk and the derive report through a [`Watch`] the caller hands in, so
//! `taiyaku-serve` can show each step's count and cancel a job without this
//! crate depending on it (spec 2026-11-15-first-scan-progress-and-cancel §7).

/// The step a scan job is in, in the order a job passes through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Waiting for the job lock, before any directory is walked, or for the
    /// derive lease, after the walks.
    Waiting,
    /// Walking a directory's tree; counts the files found.
    Listing,
    /// Reading the entries that are new or have moved; counts entries.
    Resolving,
    /// Digesting archives; counts bytes.
    Hashing,
    /// Writing works; counts works.
    Writing,
    /// Folding works together and marking the missing; no count.
    Finishing,
    /// The fingerprint pass's one step, which no scan reaches: reading each
    /// file due; counts files.
    Fingerprinting,
    /// The match pass's one step, which no scan reaches: looking up each file
    /// due; counts files.
    Matching,
}

impl Step {
    /// The word the feed and the web client name this step by.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Listing => "listing",
            Self::Resolving => "resolving",
            Self::Hashing => "hashing",
            Self::Writing => "writing",
            Self::Finishing => "finishing",
            Self::Fingerprinting => "fingerprinting",
            Self::Matching => "matching",
        }
    }
}

/// Where a running scan reports its counts, and asks whether to stop.
pub trait Watch {
    /// The job is at `step` with `done` counted, of `of` where the total is
    /// known before the step ends.
    fn at(&self, step: Step, done: u64, of: Option<u64>);

    /// Whether the job has been asked to stop. It is asked only between units
    /// of work, so a cancel leaves what a stop at that point would.
    fn cancelled(&self) -> bool;

    /// The volumes this job's early passes have written so far, told after
    /// each pass that wrote at least one (O314), so a caller can show them
    /// ready while the step stays Hashing. Nothing by default: only a scan
    /// job's watch shows the count.
    fn written(&self, _works: u64) {}
}

/// A [`Watch`] that keeps nothing and is never cancelled, for every caller
/// that neither shows progress nor stops a scan.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unwatched;

impl Watch for Unwatched {
    fn at(&self, _step: Step, _done: u64, _of: Option<u64>) {}

    fn cancelled(&self) -> bool {
        false
    }
}

/// How a watched scan ended: its report, or the step it stopped in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome<T> {
    Done(T),
    Cancelled(Step),
}

#[cfg(test)]
mod tests;
