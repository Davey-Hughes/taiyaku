use super::*;

#[test]
fn a_password_round_trips_and_a_wrong_one_does_not() {
    let phc = hash_password("correct horse").expect("hashes");
    assert!(verify_password("correct horse", &phc));
    assert!(!verify_password("correct horse ", &phc));
    assert!(!verify_password("", &phc));
}

/// Migration 4 writes [`LOCKED`] for the row spec A seeded, which is not a
/// well-formed PHC string. `verify_password` must answer **false** rather than
/// erroring, so a locked account is a failed login and not a 500 that tells a
/// caller the account exists in a different way from every other failure.
#[test]
fn the_locked_placeholder_verifies_against_nothing() {
    assert!(!verify_password("", LOCKED));
    assert!(!verify_password("anything", LOCKED));
    assert!(!verify_password(LOCKED, LOCKED));
    // And nothing else malformed panics either — the branch is on parsing, not
    // on this one value.
    assert!(!verify_password("x", ""));
    assert!(!verify_password("x", "$argon2id$nonsense"));
}

/// Two hashes of one password differ, because each carries its own salt. A
/// scheme that produced the same string twice would let a database leak show
/// which accounts share a password without breaking either.
#[test]
fn the_same_password_hashes_differently_every_time() {
    let a = hash_password("shared").expect("hashes");
    let b = hash_password("shared").expect("hashes");
    assert_ne!(a, b);
    assert!(verify_password("shared", &a));
    assert!(verify_password("shared", &b));
}

/// §2: the whole point of the `tyk_` prefix is that a key never reaches argon2.
/// Asserted on the discriminator itself, because the cost is unobservable
/// otherwise and a later refactor could quietly reintroduce it.
#[test]
fn an_api_key_is_recognisable_without_hashing_anything() {
    let key = mint_api_key();
    assert!(key.starts_with(API_KEY_PREFIX));
    assert!(looks_like_api_key(&key));
    assert!(!looks_like_api_key("a password"));
    // A password that happens to start with the prefix is still routed to the
    // key verifier, which will not find it. That is correct: the alternative is
    // trying both, which is the cost this design avoids.
    assert!(looks_like_api_key("tyk_not-really-a-key"));
}

#[test]
fn a_minted_token_is_256_bits_and_never_repeats() {
    let a = mint_token();
    let b = mint_token();
    assert_ne!(a, b);
    // 32 bytes base64url-unpadded is 43 characters.
    assert_eq!(a.len(), 43, "token is not 32 bytes of entropy");
    assert!(
        !a.contains('='),
        "padding would make the cookie value need quoting"
    );
    assert!(
        a.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "a token must survive a header and a cookie without escaping: {a}"
    );
}

#[test]
fn token_hashing_is_hex_sha256_and_is_not_the_token() {
    let token = mint_token();
    let hashed = token_sha256(&token);
    assert_eq!(hashed.len(), 64);
    assert!(
        hashed
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert_ne!(hashed, token, "the stored value must not be the token");
    assert_eq!(hashed, token_sha256(&token), "hashing is not stable");
    // A known vector, so a change of algorithm fails here rather than silently
    // invalidating every session and key in an existing library.
    assert_eq!(
        token_sha256("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

/// §10: "argon2id cost measured and asserted to be within a band, not merely
/// correct — a parameter set that verifies correctly and takes 2 ms is the
/// defect."
///
/// **The parameters are asserted where they are defined**, as three `const`
/// assertions in `credential.rs` — a compile error rather than a test failure,
/// since the thing that would be lowered by mistake is a constant. What is left
/// here is the one failure those cannot see: a `hash_password` that stopped
/// calling argon2 altogether while the constants sat there looking correct.
///
/// The floor is an order of magnitude under the measurement (release, 2026-08-18:
/// hashing 11.9 ms, verifying 10.9 ms) rather than close to it, because a floor
/// near the real cost fails on faster hardware — reporting a hardware upgrade as
/// a security defect.
#[test]
fn argon2_is_slow_on_purpose() {
    let started = std::time::Instant::now();
    let phc = hash_password("measure me").expect("hashes");
    assert!(verify_password("measure me", &phc));
    let elapsed = started.elapsed();
    assert!(
        elapsed >= std::time::Duration::from_millis(2),
        "argon2id took {elapsed:?} to hash and verify, against 22.9 ms measured \
         in release on 2026-08-18. Something is no longer calling argon2"
    );
}

/// Not an assertion — a measurement, printed so `docs/notes.md` can quote a
/// figure taken here rather than one recalled. Run with
/// `cargo test --release -p taiyaku-library credential::tests::report -- --ignored --nocapture`.
///
/// **`#[ignore]` because it is a tool rather than a test.** Twenty argon2
/// operations cost 5.6 s in the debug profile CI runs, which made a
/// *measurement* the slowest thing in the workspace and ate the slow-timeout
/// margin `.config/nextest.toml` reasons about. It asserts nothing, so nothing
/// is lost by asking for it.
#[test]
#[ignore = "a measurement, not an assertion — see the doc comment"]
fn report_the_argon2_cost() {
    let rounds = 10;
    let started = std::time::Instant::now();
    let mut phc = String::new();
    for _ in 0..rounds {
        phc = hash_password("measure me").expect("hashes");
    }
    let hashing = started.elapsed() / rounds;

    let started = std::time::Instant::now();
    for _ in 0..rounds {
        assert!(verify_password("measure me", &phc));
    }
    let verifying = started.elapsed() / rounds;

    println!(
        "argon2id m={M_COST_KIB}KiB t={T_COST} p={P_COST}: \
         hash {hashing:?}, verify {verifying:?} (mean of {rounds})"
    );
}

/// **The decoy must actually be verified against, or it equalises nothing.**
///
/// `verify_password` answers `false` early for a malformed PHC string — that is
/// what makes `LOCKED` work — so a decoy that does not parse would return in
/// microseconds and leave the timing difference exactly as it was, with a
/// constant in the source claiming otherwise. Asserted by cost rather than by
/// eye.
#[test]
fn the_decoy_hash_costs_what_a_real_one_costs() {
    let real = hash_password("a real password").expect("hashes");

    let started = std::time::Instant::now();
    assert!(!verify_password("a guess", DECOY_HASH));
    let decoy = started.elapsed();

    let started = std::time::Instant::now();
    assert!(!verify_password("a guess", &real));
    let genuine = started.elapsed();

    // Within an order of magnitude of each other, which is all that is needed:
    // the failure this guards is a decoy that costs microseconds against a real
    // hash's milliseconds, and that is a factor of a thousand.
    assert!(
        decoy * 10 >= genuine,
        "the decoy took {decoy:?} against a real hash's {genuine:?}; it is not \
         being verified at all, so an unknown name is still distinguishable by \
         response time"
    );
    // And a malformed one really would fail this, which is what makes the
    // assertion above load-bearing rather than decorative.
    let started = std::time::Instant::now();
    assert!(!verify_password("a guess", "not a phc string"));
    assert!(started.elapsed() * 10 < genuine);
}
