//! The two hashing algorithms, chosen by the entropy of what they protect.
//!
//! | Secret | Hash | Why |
//! | --- | --- | --- |
//! | Account password | argon2id | low entropy, human-chosen. Slowness *is* the defence |
//! | Session token | sha256 | 256 bits from a CSPRNG. Nothing to brute-force |
//! | API key secret | sha256 | same |
//!
//! **Argon2 on an API key would be a performance defect rather than a
//! conservative choice.** Every page request from an OPDS client carries the
//! credential, so a deliberately slow KDF at ~100 ms puts roughly 40 seconds of
//! pure hashing on a 400-page volume. Fast hashing is correct for a
//! high-entropy secret precisely because there is nothing for slowness to buy:
//! a 256-bit random token has no dictionary and no structure to attack.
//!
//! [`API_KEY_PREFIX`] is what makes that split reachable in one comparison
//! rather than by attempting an argon2 verify and falling back — which would
//! reintroduce the exact cost the table above exists to avoid.
//!
//! **Both token kinds are stored hashed and never raw**, so a database leak
//! yields no usable session and no usable API key: the same reason a password
//! is not stored raw, applied to the two secrets that are easier to forget
//! about. [`token_sha256`] is the only function that produces a stored form,
//! and `session.token_sha256` and `api_key.secret_sha256` are the only columns
//! either is written to.
//!
//! Spec A2 §2 is the design; `docs/superpowers/specs/2026-08-14-library-auth-design.md`.

use base64::Engine as _;
use sha2::Digest as _;

/// What every API key starts with, and the whole of how a key is told from a
/// password in Basic auth's opaque password field.
///
/// Load-bearing rather than cosmetic: without a discriminator the server would
/// have to attempt an argon2 verify on every OPDS request before falling back
/// to the key verifier.
pub const API_KEY_PREFIX: &str = "tyk_";

/// The locked placeholder migration 4 writes for an account that has never had
/// a password — the row spec A seeded, before `user promote` reaches it.
///
/// Not a well-formed PHC string, so [`verify_password_in`] refuses it for every
/// input, the way `/etc/shadow` refuses a locked account. It exists as a
/// constant because other code reads it: `account::admin_exists` asks whether
/// any admin has replaced it, which is what makes "an admin row exists" and
/// "an admin can log in" different questions.
pub const LOCKED: &str = "!";

/// A real argon2id hash of a value nothing can present, so a login for a name
/// that does not exist can cost what a real one costs.
///
/// **Timing is a shape.** Returning early for an unknown name would let
/// response time enumerate the account roster. Verifying against this instead
/// makes the two paths cost the same.
///
/// Generated once and pinned rather than hashed at startup: it must be a
/// well-formed PHC string that `verify_password_in` actually runs argon2 over,
/// and a constant is checkable by eye. It is a hash of 32 random bytes nobody
/// kept.
pub const DECOY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$3jVJ8kQnW2mXcR7LtYbHqA$Qx6yZ8vK1mN4pR7sT2wA9cE3gJ5kL8nQ0rU4xY7zB1E";

/// OWASP's current recommendation for argon2id, and a **starting point rather
/// than a measurement**.
///
/// Spec A2 §14 records that these are unverified on the host that will run
/// this. The `const` assertions below are the floor that stops them being
/// lowered silently; the measurement on the deployment host is what makes them
/// final. A parameter set that is fast on a workstation and slow on the server
/// is the failure this project's measurement discipline exists to catch.
const M_COST_KIB: u32 = 19_456;
const T_COST: u32 = 2;
const P_COST: u32 = 1;

/// OWASP's floor, as a **compile error rather than a test failure** — the same
/// shape as `schema.rs`'s `MIGRATIONS.len() == SCHEMA_VERSION`, and for the same
/// reason: the thing that would actually be lowered by mistake is a constant, so
/// the check belongs where the constant is.
///
/// §10 asks for the cost to be "asserted to be within a band, not merely
/// correct", and a *timing* floor is the obvious reading of that and the wrong
/// test. A clock floor set near the cost of a hash fails on faster hardware,
/// reporting a hardware upgrade as a security defect.
/// `tests::argon2_is_slow_on_purpose` keeps a 2 ms backstop for the different
/// failure these three lines cannot see — a `hash_password` that stopped
/// calling argon2 at all.
const _: () = assert!(M_COST_KIB >= 19_456, "m is below OWASP's recommendation");
const _: () = assert!(T_COST >= 2, "t is below OWASP's recommendation");
const _: () = assert!(P_COST >= 1, "p is not a parallelism");

/// Sixteen bytes, which is what `password_hash` recommends and twice what it
/// requires. Encoded rather than passed raw: `SaltString` stores the B64 form.
const SALT_BYTES: usize = 16;

/// The algorithm and the version of every hash this module makes. A stored
/// hash names both, so [`hasher`] is built from these and the string
/// [`hash_password`] writes is too.
const ALGORITHM: argon2::Algorithm = argon2::Algorithm::Argon2id;
const VERSION: argon2::Version = argon2::Version::V0x13;

fn hasher() -> argon2::Argon2<'static> {
    let params = argon2::Params::new(M_COST_KIB, T_COST, P_COST, None)
        .expect("OWASP's argon2id parameters are in range");
    argon2::Argon2::new(ALGORITHM, VERSION, params)
}

/// The memory one argon2 run works in, kept so that the next run works in it
/// too.
///
/// **A run needs the memory and nothing that was in it.** Argon2 writes each
/// block before it reads it, so no answer depends on what the memory held
/// when the run began, and one of these serves any number of runs, one at a
/// time. `argon2`'s own hasher and verifier allocate a buffer for every run
/// and free it afterwards, and how much of that a process ends up holding
/// after many runs is then the allocator's decision. A caller that keeps one
/// of these and hands it to [`verify_password_in`] holds one run's memory
/// however many passwords it verifies.
///
/// **Between runs it holds nothing of the last one.** What a run writes into
/// its blocks is made from the password, so the blocks are zeroed when the
/// run ends, whether it answered, was refused or panicked. A memory that is
/// not in a run is all zeros, however long it is kept.
///
/// **It grows and never shrinks.** A new one holds nothing. It grows to what
/// its first run needs, and again only for a stored hash that needs more
/// memory than any it has run.
///
/// The blocks are `argon2`'s and stay inside this module: nothing a caller
/// can reach names their type. `Debug` prints how many are held and none of
/// them.
pub struct WorkingMemory {
    blocks: Vec<argon2::Block>,
    /// How many times this has grown.
    #[cfg(test)]
    grown: usize,
}

impl WorkingMemory {
    /// A memory that holds nothing yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            blocks: Vec::new(),
            #[cfg(test)]
            grown: 0,
        }
    }

    /// The blocks a run at `params` fills: as many as it needs, from the front
    /// of what is held, growing first if that is too few. They are zeroed
    /// again when what this returns is dropped.
    ///
    /// **Grown to the count asked for and not past it.** `Vec`'s own growth
    /// at least doubles what it holds, and double this is the memory of a
    /// second run.
    fn blocks_for(&mut self, params: &argon2::Params) -> RunBlocks<'_> {
        let needed = params.block_count();
        if self.blocks.len() < needed {
            #[cfg(test)]
            {
                self.grown += 1;
            }
            self.blocks.reserve_exact(needed - self.blocks.len());
            self.blocks.resize(needed, argon2::Block::default());
        }
        RunBlocks(&mut self.blocks[..needed])
    }

    /// How many blocks are held.
    #[cfg(test)]
    fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// How many times this has grown. A run in a memory that already holds
    /// what it needs leaves this where it was.
    #[cfg(test)]
    fn grown(&self) -> usize {
        self.grown
    }

    /// How many blocks there is room for, which is what the memory costs
    /// whether or not they are all in use.
    #[cfg(test)]
    fn capacity(&self) -> usize {
        self.blocks.capacity()
    }

    /// Whether every block held is all zeros, which is how a run leaves the
    /// memory it used.
    #[cfg(test)]
    fn is_all_zero(&self) -> bool {
        self.blocks
            .iter()
            .all(|block| block.as_ref().iter().all(|word| *word == 0))
    }

    /// Fills every block held with words that are not zero: a memory as it
    /// would be if a run had been left in it.
    #[cfg(test)]
    fn scribble(&mut self) {
        for (block, mark) in self.blocks.iter_mut().zip(1u64..) {
            block.as_mut().fill(mark);
        }
    }
}

impl Default for WorkingMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for WorkingMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkingMemory")
            .field("blocks", &self.blocks.len())
            .finish()
    }
}

/// The blocks of a [`WorkingMemory`] that one run fills, for as long as the
/// run lasts.
///
/// **They are zeroed when this drops, and that is the only way a run ends.**
/// What argon2 writes into them is made from the password, and the memory
/// outlives the run by as long as the process lives. Zeroing in `Drop` covers
/// the run that answers, the run argon2 refuses and the run that panics alike,
/// with no exit to remember it on.
///
/// **A plain `fill`, and no `zeroize`.** The write `zeroize` exists to protect
/// is the one before a buffer is freed, which a compiler may drop because
/// nothing can read the buffer afterwards. A run does not free this memory.
/// It hands it back, and a caller that keeps one puts it into a store that
/// other threads take it out of, so a compiler cannot show that the zeros are
/// never read and has to write them.
struct RunBlocks<'a>(&'a mut [argon2::Block]);

impl Drop for RunBlocks<'_> {
    fn drop(&mut self) {
        self.0.fill(argon2::Block::default());
    }
}

/// `argon2`'s `PasswordHasher::hash_password`, for a context of this
/// algorithm, version and parameters, with its one allocation taken out: the
/// run fills `memory` where the crate's fills a buffer it makes and frees,
/// and the blocks it filled are zeroed again as the run ends.
///
/// **Every other step is the crate's, in the crate's order**, because each is
/// part of what a stored hash means. The salt that is hashed is the salt
/// string decoded from base64, never its text. The output is as long as the
/// parameters say, and the crate's default length where they say nothing. The
/// hash returned names the algorithm, version and parameters the run was
/// computed with, since it is built from the same three values.
fn hash_in<'a>(
    memory: &mut WorkingMemory,
    algorithm: argon2::Algorithm,
    version: argon2::Version,
    params: argon2::Params,
    password: &[u8],
    salt: argon2::password_hash::Salt<'a>,
) -> argon2::password_hash::Result<argon2::password_hash::PasswordHash<'a>> {
    use argon2::password_hash::{Output, ParamsString, PasswordHash, Salt};

    let context = argon2::Argon2::new(algorithm, version, params);
    let params = context.params();

    let mut salt_bytes = [0u8; Salt::MAX_LENGTH];
    let salt_bytes = salt.decode_b64(&mut salt_bytes)?;

    let output_len = params
        .output_len()
        .unwrap_or(argon2::Params::DEFAULT_OUTPUT_LEN);
    let output = Output::init_with(output_len, |out| {
        let blocks = memory.blocks_for(params);
        context
            .hash_password_into_with_memory(password, salt_bytes, out, &mut *blocks.0)
            .map_err(argon2::password_hash::Error::from)
    })?;

    Ok(PasswordHash {
        algorithm: algorithm.ident(),
        version: Some(version.into()),
        params: ParamsString::try_from(params)?,
        salt: Some(salt),
        hash: Some(output),
    })
}

/// Hash a human-chosen password for storage in `user.password_hash`.
///
/// **The salt comes from `rand` rather than from `password_hash`'s own
/// `OsRng`**, so this module has one CSPRNG rather than two. `argon2` 0.5
/// carries `rand_core` 0.6 while this workspace is on `rand` 0.10, and reaching
/// for the former's `OsRng` would mean enabling a second random source with a
/// second version to keep track of, to do a job [`mint_token`] already needs
/// done.
///
/// **An empty password is refused here**, which is the one place every writer
/// of `user.password_hash` passes through.
///
/// `hash_password("")` would produce a *well-formed* PHC string rather than
/// [`LOCKED`], so [`verify_password_in`] would answer true for it and anybody
/// who presses Enter could sign in — and `account::admin_exists` counts
/// `password_hash <> '!'`, so the account would then read as a
/// working administrator.
///
/// **The refusal is here and not in each writer**, so that a writer that has
/// not been written yet cannot miss it. The claim route, `change_password` and
/// the CLI's `accept` keep their own checks: each fires earlier and words the
/// refusal for the screen it is on, and none of them is the only thing standing
/// between an empty string and the column.
///
/// **It runs in a working memory this process keeps for hashing.** It takes
/// one out, runs argon2 in it and puts it back, holding nothing of the run.
/// It waits for no other hash: no lock is held across a run. `HASH_MEMORIES`
/// has how many of them a process comes to keep.
///
/// # Errors
///
/// Returns an error if `password` is empty, if the salt cannot be encoded, or
/// if the hasher rejects the input, which it does only for inputs beyond
/// argon2's own maximum length.
pub fn hash_password(password: &str) -> Result<String, String> {
    let mut salt = [0u8; SALT_BYTES];
    rand::fill(&mut salt);
    hash_with_salt(&HASH_MEMORIES, password, &salt)
}

/// Working memories kept between the runs that use them.
///
/// **A run takes one out, works in it and puts it back**, and a memory is
/// made only when there is none to take. So there come to be as many as have
/// been out at once, and no more however many runs follow.
///
/// **The lock is held to take a memory out and to put one back, and never
/// across the run between**, so nothing here waits for a run to finish. It is
/// a leaf: nothing else is taken or waited for while it is held, and a
/// poisoned one is taken all the same, since all that is done under it is to
/// move a memory.
struct KeptMemories {
    spare: std::sync::Mutex<Vec<WorkingMemory>>,
    /// How many runs have been lent a memory.
    #[cfg(test)]
    lent: std::sync::atomic::AtomicUsize,
    /// How many memories have been made, for the runs that found none to
    /// take.
    #[cfg(test)]
    made: std::sync::atomic::AtomicUsize,
}

impl KeptMemories {
    const fn new() -> Self {
        Self {
            spare: std::sync::Mutex::new(Vec::new()),
            #[cfg(test)]
            lent: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            made: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Runs `run` in one of these memories, and puts the memory back when
    /// `run` is over.
    ///
    /// **However it is over**: as this returns, and as a panic in `run`
    /// unwinds through it. A memory dropped on the way out would not be
    /// missed, and the next run would make another, which is the allocation
    /// this exists to make once. What goes back holds nothing of the run
    /// either way, since the run's blocks are zeroed before this is reached.
    fn lend<T>(&self, run: impl FnOnce(&mut WorkingMemory) -> T) -> T {
        #[cfg(test)]
        self.lent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let spare = self
            .spare
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop();
        let memory = spare.unwrap_or_else(|| {
            #[cfg(test)]
            self.made.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            WorkingMemory::new()
        });
        let mut lent = Lent {
            memory,
            memories: self,
        };
        run(&mut lent.memory)
    }

    /// What `read` says of each memory that is not out.
    #[cfg(test)]
    fn each<T>(&self, read: impl Fn(&WorkingMemory) -> T) -> Vec<T> {
        self.spare
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(read)
            .collect()
    }

    /// How many runs have been lent a memory.
    #[cfg(test)]
    fn lent(&self) -> usize {
        self.lent.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// How many memories have been made. How many are kept does not say: a
    /// store that made one for every run and kept the last would keep as many.
    #[cfg(test)]
    fn made(&self) -> usize {
        self.made.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// A memory out of a [`KeptMemories`], put back when this drops.
struct Lent<'a> {
    memory: WorkingMemory,
    memories: &'a KeptMemories,
}

impl Drop for Lent<'_> {
    fn drop(&mut self) {
        let memory = std::mem::take(&mut self.memory);
        self.memories
            .spare
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(memory);
    }
}

/// The working memories [`hash_password`] runs in, kept for as long as the
/// process runs.
///
/// **Kept, because what a freed buffer costs is the allocator's decision.** A
/// password is hashed on whichever thread sets it, and a buffer allocated and
/// freed for each hash may be kept by the allocator once for every thread
/// that has hashed. A memory taken from here and put back is one run's cost
/// however many threads hash.
///
/// **In the server this is one memory.** Every route that sets a password
/// hashes while it holds the library's one connection, so no two hashes run
/// at once and each takes the memory the last one put back. Nothing here
/// makes that so. Where hashes do run at once there come to be as many
/// memories as ran at once, each kept and used again, and none of them is
/// given back.
static HASH_MEMORIES: KeptMemories = KeptMemories::new();

/// [`hash_password`] with the memories it runs in and the salt named by the
/// caller.
///
/// A stored hash's salt is random and its memory is the process's, so
/// `hash_password` is the only caller outside the tests. They name a salt to
/// hold the string this writes against the one `argon2`'s own hasher writes
/// for it, and a store of their own to see what is kept in it.
///
/// **The empty password is refused in here**, where the string is made, so
/// that nothing in this module can make one for it.
fn hash_with_salt(memories: &KeptMemories, password: &str, salt: &[u8]) -> Result<String, String> {
    use argon2::password_hash::SaltString;

    if password.is_empty() {
        return Err("an empty password would lock the account rather than open it".to_string());
    }

    let salt = SaltString::encode_b64(salt).map_err(|e| format!("encoding the salt: {e}"))?;
    memories
        .lend(|memory| {
            hash_in(
                memory,
                ALGORITHM,
                VERSION,
                hasher().params().clone(),
                password.as_bytes(),
                salt.as_salt(),
            )
            .map(|hash| hash.to_string())
        })
        .map_err(|e| format!("hashing the password: {e}"))
}

/// Whether `password` matches the stored PHC string, computed in `memory`.
///
/// **Answers `false` rather than erroring on a malformed hash**, which is what
/// [`LOCKED`] is: a locked account must be a failed login, not a 500 that tells
/// a caller the account exists in a different way from every other failure.
///
/// **This is the only verifier this crate offers, and it works in the memory
/// it is handed and no other.** `argon2`'s own verifier allocates a buffer
/// for every call and frees it afterwards, which is the cost a
/// [`WorkingMemory`] is kept to avoid, so a caller that could reach it could
/// bring that cost back in one line. It is not exported. It is what this is
/// tested against: `tests::the_two_verifiers_agree` puts each of its cases to
/// both, and they answer the same, `true` only for the password the hash was
/// made from.
///
/// **The cost is the stored hash's, not this module's.** A hash made at other
/// parameters than the ones above is verified at its own, and `memory` grows
/// for one that needs more.
///
/// **`memory` comes back holding nothing of the verify.** The blocks the run
/// used are zeroed before this returns, whatever it answers.
#[must_use]
pub fn verify_password_in(memory: &mut WorkingMemory, password: &str, phc: &str) -> bool {
    use argon2::password_hash::PasswordHash;

    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    verified_in(memory, password.as_bytes(), &parsed).is_ok()
}

/// What `PasswordVerifier::verify_password` does for `argon2`, over
/// [`hash_in`] where the crate's is over its own `hash_password`.
///
/// **Nothing here is this module's choice.** The algorithm, the version and
/// the parameters are the ones the stored hash names, the version being the
/// crate's default where the hash names none, and the salt is the stored
/// hash's. The password is hashed again from those to an output as long as
/// the stored one, and the two outputs are compared by `Output`'s own `==`,
/// which takes the same time wherever they differ. A hash with no salt or no
/// output verifies nothing.
fn verified_in(
    memory: &mut WorkingMemory,
    password: &[u8],
    hash: &argon2::password_hash::PasswordHash<'_>,
) -> argon2::password_hash::Result<()> {
    if let (Some(salt), Some(expected)) = (&hash.salt, &hash.hash) {
        let params = argon2::Params::try_from(hash)?;
        let algorithm = argon2::Algorithm::try_from(hash.algorithm)?;
        let version = hash
            .version
            .map(argon2::Version::try_from)
            .transpose()?
            .unwrap_or_default();
        let computed = hash_in(memory, algorithm, version, params, password, *salt)?;

        if let Some(computed) = &computed.hash
            && expected == computed
        {
            return Ok(());
        }
    }

    Err(argon2::password_hash::Error::Password)
}

/// 32 bytes from the CSPRNG, base64url without padding — 43 characters.
///
/// Unpadded deliberately: `=` in a cookie value is legal but needs quoting, and
/// a token that round-trips through a header without escaping is one less thing
/// to get wrong.
#[must_use]
pub fn mint_token() -> String {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A minted API key: [`API_KEY_PREFIX`], then a token.
#[must_use]
pub fn mint_api_key() -> String {
    format!("{API_KEY_PREFIX}{}", mint_token())
}

/// Whether a Basic-auth password field holds an API key rather than a password.
///
/// A password that happens to start with `tyk_` is routed to the key verifier
/// and simply not found. That is correct rather than a gap: the alternative is
/// trying both, which is the cost this design exists to avoid.
#[must_use]
pub fn looks_like_api_key(secret: &str) -> bool {
    secret.starts_with(API_KEY_PREFIX)
}

/// The stored form of a session token or an API key: lowercase hex sha256.
///
/// The **only** form either is ever written in. `session` is keyed by this and
/// `api_key.secret_sha256` is unique on it, so there is no column a raw token
/// could land in by accident — the column that would hold it does not exist.
#[must_use]
pub fn token_sha256(token: &str) -> String {
    use std::fmt::Write as _;

    let digest = sha2::Sha256::digest(token.as_bytes());
    digest
        .iter()
        .fold(String::with_capacity(64), |mut acc, byte| {
            let _ = write!(acc, "{byte:02x}");
            acc
        })
}

#[cfg(test)]
mod tests;
