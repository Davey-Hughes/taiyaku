use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::*;
use crate::Library;
use crate::tests::{TempDir, tempdir};

/// A file-backed store in a directory of its own, and its path.
fn store(label: &str) -> (TempDir, PathBuf) {
    let dir = tempdir(&format!("lease-{label}"));
    let path = dir.join("library.db");
    Library::open(&path).expect("creates the store");
    (dir, path)
}

fn open(path: &Path) -> Library {
    Library::open(path).expect("opens the store")
}

fn journal(store: &Path) -> PathBuf {
    let mut name = lock_path(store).into_os_string();
    name.push("-journal");
    PathBuf::from(name)
}

#[test]
fn a_second_lease_is_refused_while_the_first_lives_and_given_once_it_drops() {
    let (_dir, path) = store("exclusion");
    let (first, second) = (open(&path), open(&path));

    let held = first
        .derive_lease(Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");
    let refused = second.derive_lease(Wait::No).expect("asks for the lease");
    assert!(
        refused.is_none(),
        "a second derive must not start while the first holds the lease"
    );

    drop(held);
    let given = second.derive_lease(Wait::No).expect("asks for the lease");
    assert!(
        given.is_some(),
        "the lease is free once its holder drops it"
    );
}

#[test]
fn a_waiting_lease_returns_only_once_the_holder_drops_and_says_it_waits() {
    let (_dir, path) = store("waiting");
    let holder = open(&path);
    let held = holder
        .derive_lease(Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");

    let waiter_path = path.clone();
    let waiter = std::thread::spawn(move || {
        let library = open(&waiter_path);
        let mut calls = Vec::new();
        let mut call = |waited: Duration| calls.push(waited);
        let got = library
            .derive_lease(Wait::Yes(&mut call))
            .expect("waits for the lease")
            .is_some();
        (got, Instant::now(), calls)
    });
    std::thread::sleep(Duration::from_millis(1500));
    let dropped_at = Instant::now();
    drop(held);
    let (got, returned_at, calls) = waiter.join().expect("the waiter did not panic");

    assert!(got, "a waiting lease is given, never refused");
    assert!(
        returned_at >= dropped_at,
        "the waiter returned before the holder let go"
    );
    assert!(
        !calls.is_empty(),
        "a wait of over a second calls back at least once"
    );
}

#[test]
fn a_held_lease_leaves_no_journal_beside_its_lock_file() {
    let (_dir, path) = store("no-journal");
    let library = open(&path);
    let held = library
        .derive_lease(Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");

    assert!(lock_path(&path).exists(), "sanity: the lock file is made");
    assert!(
        !journal(&path).exists(),
        "a hold writes no page, so no journal sits beside the lock file"
    );
    drop(held);
}

/// The environment variable that names the store to
/// [`holds_the_lease_until_killed`].
const HOLDER: &str = "TAIYAKU_TEST_LEASE_HOLDER";

/// What [`holds_the_lease_until_killed`] prints once it holds the lease.
const HELD: &str = "the holder holds the lease";

#[test]
#[ignore = "a role, not a test: a_killed_holders_lease_is_free_at_once runs this binary again \
            to hold the lease in a process of its own"]
fn holds_the_lease_until_killed() {
    let Some(store) = std::env::var_os(HOLDER) else {
        return;
    };
    let library = open(Path::new(&store));
    let _held = library
        .derive_lease(Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");
    println!("{HELD}");
    std::io::stdout().flush().expect("says it holds the lease");
    loop {
        std::thread::park();
    }
}

/// What [`tries_the_lease_once`] prints where it was given the lease.
const GIVEN: &str = "the lease was given";

#[test]
#[ignore = "a role, not a test: a_probes_close_leaves_the_lease_held_for_another_process runs \
            this binary again to ask for the lease from a process of its own"]
fn tries_the_lease_once() {
    let Some(store) = std::env::var_os(HOLDER) else {
        return;
    };
    let given = open(Path::new(&store))
        .derive_lease(Wait::No)
        .expect("asks for the lease")
        .is_some();
    if given {
        println!("{GIVEN}");
    }
}

/// Whether [`tries_the_lease_once`], run in a process of its own, was given
/// the lease on the store at `path`.
fn another_process_is_given(path: &Path) -> bool {
    let run = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args([
            "--exact",
            "lease::tests::tries_the_lease_once",
            "--ignored",
            "--nocapture",
        ])
        .env(HOLDER, path)
        .output()
        .expect("runs the other process");
    assert!(run.status.success(), "the other process asked: {run:?}");
    String::from_utf8_lossy(&run.stdout)
        .lines()
        .any(|line| line.trim() == GIVEN)
}

/// A POSIX `close` drops every fcntl lock its process holds on the file, so a
/// probe that closed the lock file outside SQLite would free this process's
/// lease for every other process. Only another process can see that: inside
/// one, SQLite keeps its own table of which connection holds what.
#[test]
fn a_probes_close_leaves_the_lease_held_for_another_process() {
    let (_dir, path) = store("probe-close");
    let (holder, watcher) = (open(&path), open(&path));
    let held = holder
        .derive_lease(Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");

    assert!(watcher.derive_running(), "sanity: the probe ran and closed");
    assert!(
        !another_process_is_given(&path),
        "after a probe in the holder's process, another process is refused"
    );

    drop(held);
    assert!(
        another_process_is_given(&path),
        "sanity: another process is given the lease once it is free"
    );
}

/// The holder's process, killed if the test ends first.
struct Holder(std::process::Child);

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A holder killed with SIGKILL runs no destructor, so only a lock the kernel
/// drops with the process frees its lease.
#[test]
fn a_killed_holders_lease_is_free_at_once() {
    let (_dir, path) = store("killed");
    let mut holder = Holder(
        std::process::Command::new(std::env::current_exe().expect("the test binary"))
            .args([
                "--exact",
                "lease::tests::holds_the_lease_until_killed",
                "--ignored",
                "--nocapture",
            ])
            .env(HOLDER, &path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("starts the holder"),
    );
    let stdout = holder.0.stdout.take().expect("the holder's stdout");
    let said_held = std::io::BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
        .any(|line| line.trim() == HELD);
    assert!(said_held, "the holder took the lease before the kill");
    let library = open(&path);
    assert!(
        library
            .derive_lease(Wait::No)
            .expect("asks for the lease")
            .is_none(),
        "sanity: the holder's lease is held from this process's side"
    );

    holder.0.kill().expect("kills the holder");
    holder.0.wait().expect("reaps the holder");

    assert!(
        library
            .derive_lease(Wait::No)
            .expect("asks for the lease")
            .is_some(),
        "a killed holder's lease is free"
    );
}

#[test]
fn derive_running_sees_a_lease_another_library_holds() {
    let (_dir, path) = store("probe");
    let (watcher, holder) = (open(&path), open(&path));

    let held = holder
        .derive_lease(Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");
    let asked = Instant::now();
    assert!(watcher.derive_running(), "the holder's lease reads running");
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "a probe never waits for the lease: {:?}",
        asked.elapsed()
    );

    drop(held);
    assert!(
        !watcher.derive_running(),
        "a dropped lease reads not running"
    );
}

#[test]
fn derive_running_sees_an_in_memory_librarys_own_lease() {
    let library = Library::open_in_memory().expect("an in-memory library");
    assert!(!library.derive_running(), "sanity: nothing holds it yet");

    let held = library
        .derive_lease(Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");
    assert!(library.derive_running(), "its own lease reads running");
    assert!(
        library
            .derive_lease(Wait::No)
            .expect("asks for the lease")
            .is_none(),
        "an in-memory library's flag is its lease"
    );

    drop(held);
    assert!(
        !library.derive_running(),
        "a dropped lease reads not running"
    );
}

#[test]
fn derive_running_writes_nothing_to_the_lock_file() {
    let (_dir, path) = store("probe-writes-nothing");
    let library = open(&path);
    drop(
        library
            .derive_lease(Wait::No)
            .expect("takes the lease")
            .expect("the lease is free"),
    );
    let before = std::fs::metadata(lock_path(&path)).expect("the lock file is made");

    let watcher = open(&path);
    for _ in 0..50 {
        assert!(!watcher.derive_running(), "sanity: nothing holds it");
    }

    let after = std::fs::metadata(lock_path(&path)).expect("the lock file stays");
    assert_eq!(
        (
            after.len(),
            after.modified().expect("an mtime"),
            journal(&path).exists()
        ),
        (before.len(), before.modified().expect("an mtime"), false),
        "a probe writes nothing: size, modification time, journal"
    );
}

#[test]
fn derive_running_answers_no_and_makes_no_file_where_no_derive_ever_ran() {
    let (_dir, path) = store("never");
    assert!(!open(&path).derive_running());
    assert!(
        !lock_path(&path).exists(),
        "a probe never creates the lock file"
    );
}

/// Whether this process can write `path` whatever its mode says, as root
/// can: the read-only cases below cannot be set up for such a process.
///
/// A plain open and close of the lock file drops every fcntl lock this
/// process holds on it, so it is asked only while none is held.
fn writes_through_modes(path: &Path) -> bool {
    std::fs::OpenOptions::new().write(true).open(path).is_ok()
}

/// O51: a lock file this process cannot write opens read-only, where
/// `BEGIN IMMEDIATE` takes no write lock, so a lease taken there would exclude
/// nothing. It is refused, naming the file and the fix.
#[test]
fn a_lock_file_this_process_cannot_write_is_refused_naming_it() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, path) = store("read-only");
    let library = open(&path);
    drop(
        library
            .derive_lease(Wait::No)
            .expect("takes the lease")
            .expect("the lease is free"),
    );
    let lock = lock_path(&path);
    std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o444)).expect("chmod");
    if writes_through_modes(&lock) {
        return;
    }

    let refused = library.derive_lease(Wait::No).map(|lease| lease.is_some());

    assert_eq!(
        refused,
        Err(format!(
            "the derive lock file {} cannot be written by this process, so no derive can \
             take its turn: chown it to the user taiyaku runs as, or make it writable \
             (chmod u+w)",
            lock.display()
        ))
    );
}

/// O51: a new lock file takes the store's mode, as SQLite's `-wal` does, so
/// the user the store is shared with can still take the lease.
#[test]
fn a_new_lock_file_takes_the_stores_mode() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, path) = store("mode");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).expect("chmod");

    drop(
        open(&path)
            .derive_lease(Wait::No)
            .expect("takes the lease")
            .expect("the lease is free"),
    );

    let mode = std::fs::metadata(lock_path(&path))
        .expect("the lock file is made")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o660);
}
