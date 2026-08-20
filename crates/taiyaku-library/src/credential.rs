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
/// Not a well-formed PHC string, so [`verify_password`] refuses it for every
/// input, the way `/etc/shadow` refuses a locked account. It exists as a
/// constant because two other things read it: migration 4 writes it, and
/// `account::admin_exists` asks whether any admin has replaced it — which is
/// what makes "an admin row exists" and "an admin can log in" different
/// questions, and spec A2 §5's claim route turns on the second.
pub const LOCKED: &str = "!";

/// A real argon2id hash of a value nothing can present, so a login for a name
/// that does not exist can cost what a real one costs.
///
/// **Timing is a shape.** Returning early for an unknown name answered 401 in
/// under a millisecond where a known name took the measured ~11 ms, so response
/// time enumerated the account roster — and because the rate limiter keys on
/// `(address, name)`, every new name opened its own window and never approached
/// the lockout, so the walk was unlimited. Verifying against this instead makes
/// the two paths cost the same.
///
/// Generated once and pinned rather than hashed at startup: it must be a
/// well-formed PHC string that `verify_password` actually runs argon2 over, and
/// a constant is checkable by eye. It is a hash of 32 random bytes nobody kept.
pub const DECOY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$3jVJ8kQnW2mXcR7LtYbHqA$Qx6yZ8vK1mN4pR7sT2wA9cE3gJ5kL8nQ0rU4xY7zB1E";

/// OWASP's current recommendation for argon2id, and a **starting point rather
/// than a measurement**.
///
/// Spec A2 §14 records that these are unverified on the host that will run
/// this. [`tests::argon2_is_slow_on_purpose`] is the floor that stops them
/// being lowered silently; the measurement on the deployment host is what makes
/// them final. A parameter set that is fast on a workstation and slow on the
/// server is the failure this project's measurement discipline exists to catch.
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
/// test. Measured in release on 2026-08-18: hashing 11.9 ms, verifying 10.9 ms.
/// A clock floor set anywhere near that fails on faster hardware, reporting a
/// hardware upgrade as a security defect. `tests::argon2_is_slow_on_purpose`
/// keeps a 2 ms backstop for the different failure these three lines cannot
/// see — a `hash_password` that stopped calling argon2 at all.
const _: () = assert!(M_COST_KIB >= 19_456, "m is below OWASP's recommendation");
const _: () = assert!(T_COST >= 2, "t is below OWASP's recommendation");
const _: () = assert!(P_COST >= 1, "p is not a parallelism");

/// Sixteen bytes, which is what `password_hash` recommends and twice what it
/// requires. Encoded rather than passed raw: `SaltString` stores the B64 form.
const SALT_BYTES: usize = 16;

fn hasher() -> argon2::Argon2<'static> {
    let params = argon2::Params::new(M_COST_KIB, T_COST, P_COST, None)
        .expect("OWASP's argon2id parameters are in range");
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
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
/// # Errors
///
/// Returns an error if the salt cannot be encoded or the hasher rejects the
/// input, which it does only for inputs beyond argon2's own maximum length.
pub fn hash_password(password: &str) -> Result<String, String> {
    use argon2::password_hash::{PasswordHasher as _, SaltString};

    let mut salt = [0u8; SALT_BYTES];
    rand::fill(&mut salt);
    let salt = SaltString::encode_b64(&salt).map_err(|e| format!("encoding the salt: {e}"))?;
    hasher()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| format!("hashing the password: {e}"))
}

/// Whether `password` matches the stored PHC string.
///
/// **Answers `false` rather than erroring on a malformed hash**, which is what
/// [`LOCKED`] is: a locked account must be a failed login, not a 500 that tells
/// a caller the account exists in a different way from every other failure.
#[must_use]
pub fn verify_password(password: &str, phc: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier as _};

    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    hasher()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
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
