use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::RecvTimeoutError;
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

/// How long a waiting-lease test waits for the waiter's two callbacks before
/// it fails.
///
/// Generous because a starved waiter, one whose thread waits for a processor,
/// starts its wait late and wakes late from each sleep, so it calls back late.
/// The file-backed waiter is later still: SQLite counts a busy wait in the
/// sleeps it asks for, not in the time that passes, so each late wake stretches
/// the slice. A passing test lets go at the second callback and never waits
/// this out.
const CALLS_BACK_WITHIN: Duration = Duration::from_secs(30);

#[test]
fn a_waiting_lease_returns_only_once_the_holder_drops_and_says_it_waits() {
    let (_dir, path) = store("waiting");
    let (holder, waiter_library, third) = (open(&path), open(&path), open(&path));
    let (called, calls) = std::sync::mpsc::channel();

    // A scoped thread, so the waiter's lease, which borrows its library, comes
    // back here to be checked. The hold lives inside the scope: a failure
    // there drops it before the scope joins the waiter, which would otherwise
    // wait for it for good.
    let (got, returned_at, dropped_at, waited) = std::thread::scope(|scope| {
        let held = holder
            .derive_lease(Wait::No)
            .expect("takes the lease")
            .expect("the lease is free");
        let waiter_library = &waiter_library;
        let waiter = scope.spawn(move || {
            let mut call = |waited: Duration| {
                let _ = called.send(waited);
            };
            let got = waiter_library
                .derive_lease(Wait::Yes(&mut call))
                .expect("waits for the lease");
            (got, Instant::now())
        });
        // The hold ends on the waiter's own callbacks rather than a clock, so it
        // covers the wait however late the waiter's thread runs. It ends at the
        // second, not the first, so a waiter that returns right after its first
        // callback ends the channel here instead of racing the drop.
        let deadline = Instant::now() + CALLS_BACK_WITHIN;
        let mut waited = [Duration::ZERO; 2];
        for (nth, slot) in ["first", "second"].into_iter().zip(&mut waited) {
            match calls.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(after) => *slot = after,
                Err(RecvTimeoutError::Timeout) => {
                    panic!(
                        "a waiter calls back after each slice the lease is still held: \
                         no {nth} callback within {CALLS_BACK_WITHIN:?}"
                    )
                }
                Err(RecvTimeoutError::Disconnected) => {
                    drop(waiter.join().expect("the waiter did not panic"));
                    panic!("the waiter returned before the holder let go");
                }
            }
        }
        let dropped_at = Instant::now();
        drop(held);
        let (got, returned_at) = waiter.join().expect("the waiter did not panic");
        (got, returned_at, dropped_at, waited)
    });

    assert!(got.is_some(), "a waiting lease is given, never refused");
    assert!(
        returned_at >= dropped_at,
        "the waiter returned before the holder let go"
    );
    // Lower bounds only. `take_file` calls back once SQLite's busy handler has
    // slept the whole slice it was given, and a sleep never ends early, so a
    // slow machine can only make these values larger; a ceiling would fail a
    // starved waiter, whose slices have no upper bound.
    let [first, second] = waited;
    assert!(
        first >= SLICE,
        "a waiter calls back with the time waited, at least a slice by its first callback: \
         {first:?}"
    );
    assert!(
        second >= first + SLICE,
        "a waiter calls back once each slice the lease is still held: {first:?}, then {second:?}"
    );
    // Asked once the holder has let go, so only the waiter's own lease can
    // refuse it: one handed out while the holder still held the lock holds none.
    assert!(
        third
            .derive_lease(Wait::No)
            .expect("asks for the lease")
            .is_none(),
        "the lease a waiter is given holds the lock: a third asker is refused"
    );
    drop(got);
    assert!(
        third
            .derive_lease(Wait::No)
            .expect("asks for the lease")
            .is_some(),
        "a third asker is given the lease once the waiter's drops"
    );
}

/// An in-memory library stands in for a file-backed one in tests across the
/// workspace, so its lease keeps [`Wait`]'s promises too.
#[test]
fn an_in_memory_lease_asked_not_to_wait_is_refused_only_after_its_try() {
    let library = Library::open_in_memory().expect("an in-memory library");
    let held = library
        .derive_lease(Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");

    let asked = Instant::now();
    let refused = library
        .derive_lease(Wait::No)
        .expect("asks for the lease")
        .is_none();
    let took = asked.elapsed();

    assert!(refused, "sanity: the lease is held");
    assert!(
        took >= TRY,
        "a lease asked not to wait is refused only once it has tried for TRY: {took:?}"
    );
    drop(held);
}

/// An in-memory waiter looks at the clock on a schedule of slices counted
/// from its start, so its second callback comes two slices in however late
/// its first came.
#[test]
fn an_in_memory_waiting_lease_returns_only_once_the_holder_drops_and_says_it_waits() {
    let library = Library::open_in_memory().expect("an in-memory library");
    let (called, calls) = std::sync::mpsc::channel();
    let let_go = AtomicBool::new(false);

    // The hold lives inside the scope, as in the file-backed test, so a
    // failure there frees the waiter before the scope joins it.
    let (got, returned_at, dropped_at, waited) = std::thread::scope(|scope| {
        let held = library
            .derive_lease(Wait::No)
            .expect("takes the lease")
            .expect("the lease is free");
        let (library, let_go) = (&library, &let_go);
        let waiter = scope.spawn(move || {
            // A flag that never clears would keep the waiter, and the join
            // below, waiting for good. One callback can be under way as the
            // holder lets go; a second after it fails the waiter instead.
            let mut after_let_go = 0;
            let mut call = |waited: Duration| {
                if let_go.load(Ordering::SeqCst) {
                    after_let_go += 1;
                    assert!(
                        after_let_go < 2,
                        "a waiter is given the lease once the holder lets go"
                    );
                }
                let _ = called.send(waited);
            };
            let got = library
                .derive_lease(Wait::Yes(&mut call))
                .expect("waits for the lease");
            (got, Instant::now())
        });
        let deadline = Instant::now() + CALLS_BACK_WITHIN;
        let mut waited = [Duration::ZERO; 2];
        for slot in &mut waited {
            *slot = calls
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("a waiter calls back after each slice the lease is still held");
        }
        let dropped_at = Instant::now();
        drop(held);
        let_go.store(true, Ordering::SeqCst);
        let (got, returned_at) = waiter.join().expect("the waiter did not panic");
        (got, returned_at, dropped_at, waited)
    });

    assert!(
        got.is_some(),
        "a waiting lease is given once the holder drops"
    );
    assert!(
        returned_at >= dropped_at,
        "the waiter returned before the holder let go"
    );
    let [first, second] = waited;
    assert!(
        first >= SLICE,
        "a waiter calls back with the time waited, at least a slice by its first callback: \
         {first:?}"
    );
    assert!(
        second >= SLICE * 2,
        "a waiter calls back once each slice the lease is still held, two slices in by its \
         second: {second:?}"
    );
    // Asked once the holder has let go, so only the waiter's own lease can
    // refuse it: one handed out while the holder still held the flag holds none.
    assert!(
        library
            .derive_lease(Wait::No)
            .expect("asks for the lease")
            .is_none(),
        "the lease a waiter is given holds the flag: another asker is refused"
    );
    drop(got);
}

/// An error other than busy while a derive waits, as from a lock file damaged
/// under it, ends the wait with that error: read as another holder's, it
/// would keep the waiter calling back for good.
#[test]
fn a_lock_file_damaged_while_a_derive_waits_for_it_ends_the_wait_with_its_error() {
    let (_dir, path) = store("damaged-while-waiting");
    let (holder, waiter) = (open(&path), open(&path));
    let held = holder
        .derive_lease(Wait::No)
        .expect("takes the lease")
        .expect("the lease is free");
    let lock = lock_path(&path);

    let mut damaged = false;
    let mut call = |_: Duration| {
        assert!(
            !damaged,
            "a waiter called back again once the lock file was damaged: its error was read \
             as another holder"
        );
        let mut header = std::fs::read(&lock).expect("reads the lock file");
        header[..100].fill(0);
        std::fs::write(&lock, header).expect("writes the lock file");
        damaged = true;
    };
    let ended = waiter
        .derive_lease(Wait::Yes(&mut call))
        .map(|lease| lease.is_some());
    drop(held);

    assert_eq!(
        ended,
        Err(format!(
            "holding the derive lock file {}: file is not a database",
            lock.display()
        ))
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

/// Make the lock file of the store at `path`, then give its header a write
/// version above 2, which SQLite reads as a read-only file, and the user
/// version `user_version`. Returns the lock file's path.
fn header_names_a_newer_write_format(path: &Path, user_version: u32) -> PathBuf {
    drop(
        open(path)
            .derive_lease(Wait::No)
            .expect("takes the lease")
            .expect("the lease is free"),
    );
    let lock = lock_path(path);
    let mut header = std::fs::read(&lock).expect("reads the lock file");
    header[18] = 3;
    header[60..64].copy_from_slice(&user_version.to_be_bytes());
    std::fs::write(&lock, header).expect("writes the lock file");
    lock
}

/// O436: SQLite learns that such a file is read-only only by reading its
/// header, which `initialise` does after `take_file` has asked whether the
/// file opened read-only, and there `BEGIN IMMEDIATE` begins a read
/// transaction and answers OK: a lease taken on it would exclude nothing.
#[test]
fn a_lock_file_whose_header_sqlite_reads_as_read_only_is_refused_naming_it() {
    let (_dir, path) = store("read-only-header");
    let lock = header_names_a_newer_write_format(&path, 1);
    let library = open(&path);

    let refused = library.derive_lease(Wait::No).map(|lease| lease.is_some());

    assert_eq!(
        refused,
        Err(format!(
            "the derive lock file {} took no write lock, so no derive can \
             take its turn: SQLite reads it as read-only, as it does a file whose header \
             is damaged. It holds no data, so remove it: the next derive makes it afresh",
            lock.display()
        ))
    );
    assert!(
        !library.derive_running(),
        "a refused lease leaves no derive running"
    );
}

/// Two derives can open a new lock file at once and both read it
/// uninitialised; the one that gives it its page then holds it, and the
/// other's write is answered busy, which means another holds the lease. A
/// hold on a lock file nobody has initialised puts an asker in that place.
#[test]
fn a_lock_file_held_before_an_asker_could_initialise_it_refuses_the_asker() {
    let (_dir, path) = store("held-uninitialised");
    let hold = rusqlite::Connection::open(lock_path(&path)).expect("opens the lock file");
    hold.execute_batch("BEGIN IMMEDIATE")
        .expect("holds the lock file");
    let library = open(&path);

    let refused = library.derive_lease(Wait::No).map(|lease| lease.is_some());
    assert_eq!(
        refused,
        Ok(false),
        "an asker whose initialising write is answered busy is refused, not failed"
    );

    drop(hold);
    let given = library.derive_lease(Wait::No).map(|lease| lease.is_some());
    assert_eq!(
        given,
        Ok(true),
        "sanity: the lease is given once it is free"
    );
}

/// An error other than busy while giving a new lock file its page is that
/// error, naming the step: passed over, a new file left with no page would be
/// held as it is, and a hold there writes one.
#[test]
fn a_lock_file_sqlite_will_not_initialise_is_refused_with_sqlites_error() {
    let (_dir, path) = store("read-only-header-uninitialised");
    let lock = header_names_a_newer_write_format(&path, 0);
    let library = open(&path);

    let refused = library.derive_lease(Wait::No).map(|lease| lease.is_some());

    assert_eq!(
        refused,
        Err(format!(
            "initialising the derive lock file {}: attempt to write a readonly database",
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
