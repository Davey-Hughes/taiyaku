use super::*;

/// `verify_password_in` in a memory made for the one call, for a test that
/// verifies once and is not about the memory.
fn verifies(password: &str, phc: &str) -> bool {
    verify_password_in(&mut WorkingMemory::new(), password, phc)
}

/// `argon2`'s own verifier, which [`the_two_verifiers_agree`] holds
/// `verify_password_in` against.
///
/// **It is here and not in the module.** It allocates a buffer for the call
/// and frees it afterwards, which is the cost a kept memory is there to
/// avoid, so nothing that ships can be allowed to verify with it. Like the
/// module's verifier it answers `false` for a string that is not a PHC string.
fn argon2s_own_verifier(password: &str, phc: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier as _};

    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    hasher()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[test]
fn a_password_round_trips_and_a_wrong_one_does_not() {
    let phc = hash_password("correct horse").expect("hashes");
    assert!(verifies("correct horse", &phc));
    assert!(!verifies("correct horse ", &phc));
    assert!(!verifies("", &phc));
}

/// Migration 4 writes [`LOCKED`] for the row spec A seeded, which is not a
/// well-formed PHC string. `verify_password_in` must answer **false** rather than
/// erroring, so a locked account is a failed login and not a 500 that tells a
/// caller the account exists in a different way from every other failure.
#[test]
fn the_locked_placeholder_verifies_against_nothing() {
    assert!(!verifies("", LOCKED));
    assert!(!verifies("anything", LOCKED));
    assert!(!verifies(LOCKED, LOCKED));
    // And nothing else malformed panics either — the branch is on parsing, not
    // on this one value.
    assert!(!verifies("x", ""));
    assert!(!verifies("x", "$argon2id$nonsense"));
}

/// Two hashes of one password differ, because each carries its own salt. A
/// scheme that produced the same string twice would let a database leak show
/// which accounts share a password without breaking either.
#[test]
fn the_same_password_hashes_differently_every_time() {
    let a = hash_password("shared").expect("hashes");
    let b = hash_password("shared").expect("hashes");
    assert_ne!(a, b);
    assert!(verifies("shared", &a));
    assert!(verifies("shared", &b));
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
/// The floor is an order of magnitude under the cost of the hash and the verify
/// this times rather than close to it, because a floor near the real cost fails
/// on faster hardware — reporting a hardware upgrade as a security defect.
#[test]
fn argon2_is_slow_on_purpose() {
    let started = std::time::Instant::now();
    let phc = hash_password("measure me").expect("hashes");
    assert!(verifies("measure me", &phc));
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
/// It times a hash, a verify by `argon2`'s own verifier, which allocates for
/// each call, and the verify the server runs: `verify_password_in` in a memory
/// that is kept, with the zeroing after each run timed on its own beside it.
///
/// **`#[ignore]` because it is a tool rather than a test.** It runs argon2 for
/// many rounds, which in the debug profile CI runs would spend the slow-timeout
/// margin `.config/nextest.toml` reasons about on a measurement. It asserts
/// nothing, so nothing is lost by asking for it.
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
        assert!(argon2s_own_verifier("measure me", &phc));
    }
    let verifying = started.elapsed() / rounds;

    // What the server runs: the same verify in a memory it keeps, which is
    // grown once before the clock starts and zeroed after every run.
    let mut memory = WorkingMemory::new();
    assert!(verify_password_in(&mut memory, "measure me", &phc));
    let started = std::time::Instant::now();
    for _ in 0..rounds {
        assert!(verify_password_in(&mut memory, "measure me", &phc));
    }
    let verifying_kept = started.elapsed() / rounds;

    // The zeroing on its own: a run's blocks taken and dropped. Each round
    // fills the memory first and shows it to `black_box` after, so that every
    // zeroing has something to zero and none can be folded into the next.
    let mut zeroing = std::time::Duration::ZERO;
    for _ in 0..rounds {
        memory.scribble();
        let started = std::time::Instant::now();
        drop(memory.blocks_for(hasher().params()));
        std::hint::black_box(&mut memory);
        zeroing += started.elapsed();
    }
    let zeroing = zeroing / rounds;

    println!(
        "argon2id m={M_COST_KIB}KiB t={T_COST} p={P_COST}: \
         hash {hashing:?}, argon2's own verify {verifying:?}, verify in a kept \
         memory {verifying_kept:?} of which zeroing it {zeroing:?} (mean of {rounds})"
    );
}

/// **The decoy must actually be verified against, or it equalises nothing.**
///
/// `verify_password_in` answers `false` early for a malformed PHC string — that is
/// what makes `LOCKED` work — so a decoy that does not parse would return in
/// microseconds and leave the timing difference exactly as it was, with a
/// constant in the source claiming otherwise. Asserted by cost rather than by
/// eye.
#[test]
fn the_decoy_hash_costs_what_a_real_one_costs() {
    let real = hash_password("a real password").expect("hashes");

    let started = std::time::Instant::now();
    assert!(!verifies("a guess", DECOY_HASH));
    let decoy = started.elapsed();

    let started = std::time::Instant::now();
    assert!(!verifies("a guess", &real));
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
    assert!(!verifies("a guess", "not a phc string"));
    assert!(started.elapsed() * 10 < genuine);
}
/// The stored hash carries the parameters it was made with, so this asserts
/// that `hasher()` uses **this module's** constants rather than any others.
///
/// **It does not kill `hasher -> Argon2::default()`.**
/// `argon2::Params::DEFAULT_M_COST/T_COST/P_COST` are 19456, 2 and 1 — the same
/// three values `M_COST_KIB`, `T_COST` and `P_COST` hold, because both are
/// OWASP's current recommendation. So the mutant is genuinely equivalent today
/// and `docs/mutants-baseline.md` records it as one.
///
/// It stops being equivalent the moment the NAS measurement (§14) raises these
/// above OWASP's floor, which is the whole reason that measurement is pending —
/// and on that day this test is what fails, rather than nothing.
#[test]
fn the_stored_hash_carries_this_modules_parameters() {
    let phc = hash_password("measure me").expect("hashes");
    assert!(
        phc.contains(&format!("m={M_COST_KIB},t={T_COST},p={P_COST}")),
        "the hash records parameters this module did not choose: {phc}"
    );
    assert!(
        phc.starts_with("$argon2id$v=19$"),
        "algorithm or version changed: {phc}"
    );
}

#[test]
fn an_empty_password_is_refused_where_every_writer_passes_through() {
    let why = hash_password("").expect_err("refuses");
    assert!(why.contains("empty password"), "{why}");
    assert!(why.contains("presses Enter"), "{why}");

    // The property the refusal exists for: without it, this is the hash an
    // account would carry, and it verifies.
    let real = hash_password("x").expect("a one-character password is still a password");
    assert!(!verifies("", &real));
}

/// The password the hashes below are made from, and one that is not it.
const RIGHT: &str = "correct horse";
const WRONG: &str = "correct horse ";

/// Any salt will do for a hash only these tests read.
const A_SALT: &[u8] = b"a salt for tests";

/// A PHC string for `password` made by `argon2` itself, at the algorithm,
/// version and parameters the caller names: a stored hash this module's
/// constants did not make.
fn hashed_by_argon2(
    algorithm: argon2::Algorithm,
    version: argon2::Version,
    params: argon2::Params,
    salt: &[u8],
    password: &str,
) -> String {
    use argon2::password_hash::{PasswordHasher as _, SaltString};

    let salt = SaltString::encode_b64(salt).expect("the salt encodes");
    argon2::Argon2::new(algorithm, version, params)
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hashes")
        .to_string()
}

/// Parameters a table of hashes can afford.
fn small(m: u32, t: u32, p: u32, output_len: Option<usize>) -> argon2::Params {
    argon2::Params::new(m, t, p, output_len).expect("the parameters are in range")
}

/// A memory cost above this module's, at one pass so that it costs less time
/// than a hash of this module's does.
fn larger() -> argon2::Params {
    argon2::Params::new(M_COST_KIB + 1024, 1, 1, None).expect("the parameters are in range")
}

/// A memory cost below this module's, at one pass.
fn smaller() -> argon2::Params {
    argon2::Params::new(M_COST_KIB - 1024, 1, 1, None).expect("the parameters are in range")
}

/// A small memory cost at as many passes as this module's cost allows: its
/// memory times its passes, exactly.
fn the_most_work() -> argon2::Params {
    let t = M_COST_KIB * T_COST / 64;
    assert_eq!(
        64 * t,
        M_COST_KIB * T_COST,
        "the cost is not a whole number of passes"
    );
    argon2::Params::new(64, t, 1, None).expect("the parameters are in range")
}

/// One pass more than [`the_most_work`]: more work than this module's cost, in
/// less memory.
fn longer() -> argon2::Params {
    argon2::Params::new(64, the_most_work().t_cost() + 1, 1, None)
        .expect("the parameters are in range")
}

/// One lane more than this module's, at a small memory cost and two passes.
fn wider() -> argon2::Params {
    in_lanes(P_COST + 1)
}

/// Two passes in `lanes`, at the least memory `argon2` allows that many lanes
/// or at 64 KiB, whichever is more.
fn in_lanes(lanes: u32) -> argon2::Params {
    small((8 * lanes).max(64), 2, lanes, None)
}

/// Where each part of a PHC string that carries all of them sits when it is
/// split at its `$`s. The string starts with one, so the first field is empty.
const ALGORITHM_FIELD: usize = 1;
const VERSION_FIELD: usize = 2;
const PARAMS_FIELD: usize = 3;
const SALT_FIELD: usize = 4;
const OUTPUT_FIELD: usize = 5;

/// `phc` with one of its fields written differently.
fn with_field(phc: &str, field: usize, value: &str) -> String {
    let mut fields: Vec<&str> = phc.split('$').collect();
    fields[field] = value;
    fields.join("$")
}

/// `phc` without one of its fields.
fn without_field(phc: &str, field: usize) -> String {
    let mut fields: Vec<&str> = phc.split('$').collect();
    fields.remove(field);
    fields.join("$")
}

/// `phc` with one bit of its output changed, in the byte `pick` chooses from
/// the output's length. Still a well-formed output of the same length.
fn with_an_output_byte_changed(phc: &str, pick: fn(usize) -> usize) -> String {
    use argon2::password_hash::{Output, PasswordHash};

    let mut parsed = PasswordHash::new(phc).expect("parses");
    let mut bytes = parsed.hash.expect("carries an output").as_bytes().to_vec();
    let at = pick(bytes.len());
    bytes[at] ^= 1;
    parsed.hash = Some(Output::new(&bytes).expect("is still an output"));
    parsed.to_string()
}

/// One password offered against one stored string, and what a verifier must
/// answer.
struct Case {
    what: &'static str,
    password: &'static str,
    phc: String,
    answer: bool,
}

/// `phc` was made from [`RIGHT`]: that verifies, and [`WRONG`] does not.
fn made_from_right(cases: &mut Vec<Case>, what: &'static str, phc: &str) {
    for (password, answer) in [(RIGHT, true), (WRONG, false)] {
        cases.push(Case {
            what,
            password,
            phc: phc.to_string(),
            answer,
        });
    }
}

/// Nothing verifies against `phc`, and `password` is what is tried.
fn refused(cases: &mut Vec<Case>, what: &'static str, password: &'static str, phc: &str) {
    cases.push(Case {
        what,
        password,
        phc: phc.to_string(),
        answer: false,
    });
}

/// **`verify_password_in` answers what `argon2`'s own verifier answers, for
/// every stored string within this module's cost and every password.**
///
/// `verify_password_in` is `argon2`'s own verifier written out again around a
/// memory the caller keeps, so it is right exactly as far as it agrees with
/// [`argon2s_own_verifier`]. Each case is put to both, and to an answer
/// written down here as well: two verifiers that both refused a hash this
/// table had built wrongly would agree and prove nothing.
///
/// **A stored hash is verified at its own cost, up to this module's.** This
/// module's parameters may be raised, and a hash made before that still has
/// to verify. So the table holds hashes this module could not have made: other
/// memory and time costs, the two other algorithms, the older version and none,
/// a key id and associated data, and other output and salt lengths. A costlier
/// hash is refused, as the tests below hold (O217).
///
/// **One memory runs the whole table, and before each case it is filled with
/// words that are not zero.** A kept memory is zeroed after every run, so in
/// use a run finds what `argon2`'s own verifier would have allocated for it.
/// The answer must not rest on that. The memory is grown to a small cost
/// before the first case, so that the first has something to be filled
/// like the rest. Each case is then put to a memory as it would be had a run
/// been left in it, and `argon2`'s own verifier is put the same case with a
/// new one. A case that reaches `argon2` runs in that memory, in the front of
/// it where the case needs less than it holds.
#[test]
fn the_two_verifiers_agree() {
    use argon2::{Algorithm, Version};

    let mut cases = Vec::new();

    made_from_right(
        &mut cases,
        "a hash this module made",
        &hash_password(RIGHT).expect("hashes"),
    );
    refused(&mut cases, "the decoy", "a guess", DECOY_HASH);

    for password in ["", "anything", LOCKED] {
        refused(&mut cases, "the locked placeholder", password, LOCKED);
    }
    refused(&mut cases, "an empty string", RIGHT, "");
    refused(&mut cases, "not a PHC string", RIGHT, "not a phc string");
    refused(
        &mut cases,
        "a PHC string of a name and one field",
        RIGHT,
        "$argon2id$nonsense",
    );

    let plain = hashed_by_argon2(
        Algorithm::Argon2id,
        Version::V0x13,
        small(64, 2, 1, None),
        A_SALT,
        RIGHT,
    );
    made_from_right(&mut cases, "a smaller m", &plain);
    refused(&mut cases, "the empty password", "", &plain);
    refused(
        &mut cases,
        "no hash part",
        RIGHT,
        &without_field(&plain, OUTPUT_FIELD),
    );
    refused(
        &mut cases,
        "neither a salt nor a hash part",
        RIGHT,
        &without_field(&without_field(&plain, OUTPUT_FIELD), SALT_FIELD),
    );
    refused(
        &mut cases,
        "something after the hash part",
        RIGHT,
        &format!("{plain}$more"),
    );
    refused(
        &mut cases,
        "an output whose last byte is not the right one",
        RIGHT,
        &with_an_output_byte_changed(&plain, |length| length - 1),
    );
    refused(
        &mut cases,
        "an output whose first byte is not the right one",
        RIGHT,
        &with_an_output_byte_changed(&plain, |_| 0),
    );
    refused(
        &mut cases,
        "an algorithm argon2 is not",
        RIGHT,
        &with_field(&plain, ALGORITHM_FIELD, "scrypt"),
    );
    refused(
        &mut cases,
        "a version argon2 never had",
        RIGHT,
        &with_field(&plain, VERSION_FIELD, "v=18"),
    );
    refused(
        &mut cases,
        "a parameter argon2 does not have",
        RIGHT,
        &with_field(&plain, PARAMS_FIELD, "m=64,t=2,p=1,x=1"),
    );
    refused(
        &mut cases,
        "a memory cost argon2 refuses",
        RIGHT,
        &with_field(&plain, PARAMS_FIELD, "m=1,t=2,p=1"),
    );
    refused(
        &mut cases,
        "a salt too short for argon2",
        RIGHT,
        &with_field(&plain, SALT_FIELD, "somesalt"),
    );
    made_from_right(
        &mut cases,
        "no version field",
        &without_field(&plain, VERSION_FIELD),
    );

    let others = [
        (
            "a different t",
            Algorithm::Argon2id,
            Version::V0x13,
            small(64, 3, 1, None),
            A_SALT,
        ),
        (
            "argon2i",
            Algorithm::Argon2i,
            Version::V0x13,
            small(64, 2, 1, None),
            A_SALT,
        ),
        (
            "argon2d",
            Algorithm::Argon2d,
            Version::V0x13,
            small(64, 2, 1, None),
            A_SALT,
        ),
        (
            "the older version",
            Algorithm::Argon2id,
            Version::V0x10,
            small(64, 2, 1, None),
            A_SALT,
        ),
        (
            "a shorter output",
            Algorithm::Argon2id,
            Version::V0x13,
            small(64, 2, 1, Some(16)),
            A_SALT,
        ),
        (
            "the longest output a PHC string holds",
            Algorithm::Argon2id,
            Version::V0x13,
            small(64, 2, 1, Some(64)),
            A_SALT,
        ),
        (
            "the shortest salt argon2 takes",
            Algorithm::Argon2id,
            Version::V0x13,
            small(64, 2, 1, None),
            &A_SALT[..8],
        ),
        (
            "the longest salt a PHC string holds",
            Algorithm::Argon2id,
            Version::V0x13,
            small(64, 2, 1, None),
            &[7u8; 48][..],
        ),
        (
            "a key id and associated data",
            Algorithm::Argon2id,
            Version::V0x13,
            argon2::ParamsBuilder::new()
                .m_cost(64)
                .t_cost(2)
                .p_cost(1)
                .keyid(argon2::KeyId::new(b"a key").expect("fits"))
                .data(argon2::AssociatedData::new(b"bound to this").expect("fits"))
                .build()
                .expect("the parameters are in range"),
            A_SALT,
        ),
    ];
    for (what, algorithm, version, params, salt) in others {
        made_from_right(
            &mut cases,
            what,
            &hashed_by_argon2(algorithm, version, params, salt, RIGHT),
        );
    }

    let mut memory = WorkingMemory::new();
    drop(memory.blocks_for(&small(64, 2, 1, None)));
    for case in &cases {
        let Case {
            what,
            password,
            phc,
            answer,
        } = case;
        assert_eq!(
            argon2s_own_verifier(password, phc),
            *answer,
            "{what}: argon2's own verifier does not answer {answer} for {password:?} \
             against {phc:?}, so this case is not the one its name says"
        );
        memory.scribble();
        assert!(
            !memory.is_all_zero(),
            "{what}: the memory held nothing to fill before this case"
        );
        assert_eq!(
            verify_password_in(&mut memory, password, phc),
            *answer,
            "{what}: the verifier that keeps its memory disagrees with argon2's own \
             about {password:?} against {phc:?}"
        );
    }
    assert_eq!(
        memory.block_count(),
        hasher().params().block_count(),
        "the costliest case did not grow this memory, so the cases were not answered \
         in it"
    );
}

/// A PHC string for [`RIGHT`] made by `argon2` at `params`, in argon2id at
/// the current version.
fn hashed_at(params: argon2::Params) -> String {
    hashed_by_argon2(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        params,
        A_SALT,
        RIGHT,
    )
}

/// **A stored hash that asks for more memory than this module's is refused,
/// and grows nothing (O217).**
///
/// `argon2`'s own verifier accepts it, so this is where the two verifiers
/// part, on purpose: a run at that cost would grow a kept memory for as long
/// as the process lasts. Here the memory already holds what a run at this
/// module's cost needs, and it holds no more after the refusal.
#[test]
fn a_stored_hash_over_this_modules_memory_cost_is_refused() {
    let costlier = hashed_at(larger());
    assert!(
        argon2s_own_verifier(RIGHT, &costlier),
        "argon2 does not verify the costlier hash, so refusing it shows nothing"
    );

    let needed = hasher().params().block_count();
    let mut memory = WorkingMemory::new();
    drop(memory.blocks_for(hasher().params()));
    assert!(
        !verify_password_in(&mut memory, RIGHT, &costlier),
        "a hash that needs more memory than this module's was verified"
    );
    assert_eq!(
        (memory.block_count(), memory.capacity(), memory.grown()),
        (needed, needed, 1),
        "refusing a costlier hash grew a memory that held this module's cost"
    );
}

/// **A stored hash that asks for more work than this module's is refused, and
/// one that asks for as much is not (O217).**
///
/// Work is memory times passes, so a hash at a small memory cost may take
/// many passes, and one pass more than this module's work is refused.
#[test]
fn a_stored_hash_over_this_modules_work_is_refused() {
    let longest = hashed_at(longer());
    assert!(
        argon2s_own_verifier(RIGHT, &longest),
        "argon2 does not verify the longer hash, so refusing it shows nothing"
    );
    assert!(
        !verifies(RIGHT, &longest),
        "a hash that asks for more work than this module's was verified"
    );
    assert!(
        verifies(RIGHT, &hashed_at(the_most_work())),
        "a hash that asks for as much work as this module's, in less memory, was \
         refused"
    );
}

/// **A stored hash in more lanes than this module's is refused, and one in as
/// many is not (O217).**
///
/// A lane adds no memory but some work of its own, so a hash within this
/// module's memory and work can still take longer than this module's verify
/// (`docs/notes.md`, O217).
#[test]
fn a_stored_hash_in_more_lanes_than_this_modules_is_refused() {
    let wider = hashed_at(wider());
    assert!(
        argon2s_own_verifier(RIGHT, &wider),
        "argon2 does not verify the hash in more lanes, so refusing it shows nothing"
    );
    assert!(
        !verifies(RIGHT, &wider),
        "a hash in more lanes than this module's was verified"
    );
    assert!(
        verifies(RIGHT, &hashed_at(in_lanes(P_COST))),
        "a hash in as many lanes as this module's, at a small cost, was refused"
    );
}

/// **A refused hash costs what a wrong password does (O217).** It is refused
/// after a verify against [`DECOY_HASH`], whose cost is this module's, so a
/// new memory is grown to that cost by the refusal and left zeroed.
#[test]
fn a_hash_over_this_modules_cost_is_refused_after_the_decoys_run() {
    let decoy = argon2::password_hash::PasswordHash::new(DECOY_HASH).expect("the decoy parses");
    assert!(
        !over_this_modules_cost(&decoy),
        "the decoy costs more than this module's cost, so a refusal costs more than a \
         wrong password"
    );
    for (what, params) in [
        ("over this module's memory cost", larger()),
        ("over this module's work", longer()),
        ("in more lanes than this module's", wider()),
    ] {
        let mut memory = WorkingMemory::new();
        assert!(!verify_password_in(&mut memory, RIGHT, &hashed_at(params)));
        assert_eq!(
            (memory.block_count(), memory.grown()),
            (hasher().params().block_count(), 1),
            "a hash {what} was refused without the decoy's run"
        );
        assert!(
            memory.is_all_zero(),
            "the decoy's run left something of itself in the memory"
        );
    }
}

/// **A stored hash that parses but whose parameters `argon2` itself rejects,
/// or that names `p` twice, is refused after the decoy's run too, and none
/// panics (O217).** This module's own hash edited, since `argon2` makes none
/// of them: lanes enough to overflow its check of the memory, more than it
/// takes, two in too little memory, more memory than a `u32` counts, each
/// answered at once without the decoy; and `p` named twice, one first, where
/// `argon2` reads the last: 2^29, and two lanes in a hash it made, which it
/// verifies. A case that panics reads `None`.
#[test]
fn a_hash_whose_parameters_argon2_rejects_is_refused_after_the_decoys_run() {
    let ours = hash_password(RIGHT).expect("hashes");
    let in_two_lanes = hashed_at(wider());
    let edited = |phc: &str, edits: &[(&str, &str)]| {
        edits.iter().fold(phc.to_string(), |phc, (from, to)| {
            assert_eq!(phc.matches(from).count(), 1, "{from} is not in {phc} once");
            phc.replacen(from, to, 1)
        })
    };
    let cases = [
        (
            "in 2^29 lanes",
            edited(&ours, &[(",p=1$", ",p=536870912$")]),
        ),
        ("in 2^24 lanes", edited(&ours, &[(",p=1$", ",p=16777216$")])),
        (
            "in two lanes of 8 KiB",
            edited(&ours, &[("$m=19456,", "$m=8,"), (",p=1$", ",p=2$")]),
        ),
        (
            "at 2^32 KiB",
            edited(&ours, &[("$m=19456,", "$m=4294967296,")]),
        ),
        (
            "in 2^29 lanes after a first p of one",
            edited(&ours, &[(",p=1$", ",p=1,p=536870912$")]),
        ),
        (
            "in two lanes after a first p of one",
            edited(&in_two_lanes, &[(",p=2$", ",p=1,p=2$")]),
        ),
    ];

    let told: Vec<_> = cases
        .iter()
        .map(|(what, phc)| {
            let mut memory = WorkingMemory::new();
            let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                verify_password_in(&mut memory, RIGHT, phc)
            }));
            (
                *what,
                answer.ok(),
                memory.block_count(),
                memory.grown(),
                memory.is_all_zero(),
            )
        })
        .collect();
    let refused_after_the_decoy: Vec<_> = cases
        .iter()
        .map(|(what, _)| (*what, Some(false), hasher().params().block_count(), 1, true))
        .collect();
    assert_eq!(
        told, refused_after_the_decoy,
        "a hash argon2 rejects was not refused after the decoy's run, or panicked"
    );
}

/// **A stored hash that parses but that `argon2` will not run is refused
/// after the decoy's run too (O217).** A hash at 64 KiB with no output, with
/// neither a salt nor an output, in an algorithm or at a version `argon2`
/// does not have, or with a salt that is not base64 or is too short for it.
/// Refused without the decoy, each would answer at once; that the memory
/// grew to the decoy's size, and no further, is what shows the decoy ran.
#[test]
fn a_hash_argon2_will_not_run_is_refused_after_the_decoys_run() {
    let cheap = hashed_at(small(64, 2, 1, None));
    let cases = [
        ("with no output", without_field(&cheap, OUTPUT_FIELD)),
        (
            "with neither a salt nor an output",
            without_field(&without_field(&cheap, OUTPUT_FIELD), SALT_FIELD),
        ),
        (
            "in an algorithm argon2 is not",
            with_field(&cheap, ALGORITHM_FIELD, "argon2q"),
        ),
        (
            "at a version argon2 never had",
            with_field(&cheap, VERSION_FIELD, "v=20"),
        ),
        (
            "with a salt that is not base64",
            with_field(&cheap, SALT_FIELD, "not.base64"),
        ),
        (
            "with a salt too short for argon2",
            with_field(&cheap, SALT_FIELD, "somesalt"),
        ),
    ];

    let told: Vec<_> = cases
        .iter()
        .map(|(what, phc)| {
            let mut memory = WorkingMemory::new();
            let answer = verify_password_in(&mut memory, RIGHT, phc);
            (*what, answer, memory.block_count(), memory.is_all_zero())
        })
        .collect();
    let refused_after_the_decoy: Vec<_> = cases
        .iter()
        .map(|(what, _)| (*what, false, hasher().params().block_count(), true))
        .collect();
    assert_eq!(
        told, refused_after_the_decoy,
        "a hash argon2 will not run was refused without the decoy's run"
    );
}

/// **A working memory is filled again and not made again.**
///
/// It holds nothing until a run needs it, and grows to what that run needs,
/// first a cheaper hash's. A later run at the same cost does not grow it, and
/// is handed the blocks it was already holding and not new ones. A hash at
/// this module's cost grows it once more, to what that hash needs and no
/// further, and it stays grown: the next verify at the cheaper cost runs in
/// the front of the bigger memory without growing it, answers right for the
/// right password and for a wrong one, and leaves all of it zeroed.
///
/// **Its size cannot show that it was not made again**, since a memory freed
/// and allocated afresh for every run is the same size each time. So the
/// memory counts the times it has grown, and that is read. And a run is
/// handed what the memory held when it began, which a new allocation would
/// not hold: the memory is filled with words that are not zero, and the
/// blocks the next run would get are read before it starts.
#[test]
fn a_working_memory_grows_to_what_a_run_needs_and_is_used_again() {
    let first = smaller().block_count();
    let cheaper = hashed_by_argon2(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        smaller(),
        A_SALT,
        RIGHT,
    );

    let mut memory = WorkingMemory::new();
    assert_eq!(
        memory.block_count(),
        0,
        "a new working memory already holds something"
    );
    assert_eq!(WorkingMemory::default().block_count(), 0);

    assert!(verify_password_in(&mut memory, RIGHT, &cheaper));
    assert_eq!(
        memory.block_count(),
        first,
        "a verify did not leave its memory holding what one run at the stored hash's \
         cost needs"
    );
    assert_eq!(
        memory.capacity(),
        first,
        "a new memory took room for more blocks than its first run needs"
    );
    assert!(!verify_password_in(&mut memory, WRONG, &cheaper));
    assert_eq!(
        memory.block_count(),
        first,
        "a second verify at the same cost changed how much the memory holds"
    );
    assert_eq!(
        memory.grown(),
        1,
        "a second verify at the same cost grew a memory that already held what it needs"
    );
    memory.scribble();
    assert!(
        memory
            .blocks_for(&smaller())
            .0
            .iter()
            .all(|block| block.as_ref().iter().all(|word| *word != 0)),
        "a run at the same cost was handed blocks that are not the ones the memory \
         was holding"
    );

    let needed = hasher().params().block_count();
    assert!(needed > first, "this module's cost is not the larger");
    let phc = hash_password(RIGHT).expect("hashes");
    assert!(
        verify_password_in(&mut memory, RIGHT, &phc),
        "a hash at this module's cost was refused in a memory a cheaper run had used"
    );
    assert_eq!(
        memory.block_count(),
        needed,
        "the memory did not grow to what a hash at this module's cost needs"
    );
    // Room for what it holds and no more. This is the growth that could have
    // gone past: a `Vec` left to grow by itself takes at least twice what it
    // had, and here that is nearly a second run's memory held for nothing.
    assert_eq!(
        memory.capacity(),
        needed,
        "a memory that grew for a costlier hash took room for more blocks than the \
         hash needs"
    );
    assert_eq!(
        memory.grown(),
        2,
        "a costlier hash did not grow the memory exactly once more"
    );

    assert!(
        verify_password_in(&mut memory, RIGHT, &cheaper),
        "the right password was refused in a memory a bigger run had used"
    );
    assert!(
        !verify_password_in(&mut memory, WRONG, &cheaper),
        "a wrong password was accepted in a memory a bigger run had used"
    );
    assert_eq!(
        memory.block_count(),
        needed,
        "the memory did not stay the size the costlier hash grew it to"
    );
    assert_eq!(
        memory.grown(),
        2,
        "a verify that needs less than the memory holds grew it"
    );
    assert!(
        memory.is_all_zero(),
        "a run in the front of a bigger memory left something of itself there"
    );

    // And from the bigger memory a run at the cheaper cost is handed the
    // blocks it needs and no more, which is what `argon2` would have allocated
    // for it. `argon2` takes the same blocks out of a longer slice today, so
    // nothing above can tell the two apart; this is what would.
    assert_eq!(
        memory.blocks_for(&smaller()).0.len(),
        first,
        "a run at the cheaper cost was not handed exactly the blocks it needs from a \
         memory that holds more"
    );
}

/// **A working memory holds nothing of a run once the run is over.**
///
/// What a run writes into its blocks is made from the password, and a kept
/// memory lasts as long as the process does. So the blocks a run used are
/// zeroed when it ends, however it ends: after a verify that answers `true`,
/// after one that answers `false`, and after one `argon2` refuses once the
/// memory is in its hands.
///
/// **The last is a salt too short for `argon2`**, which is the one way a
/// stored string gets as far as `argon2` and is turned away: its checks come
/// before it writes a block, and nothing it is given makes it fail after. So
/// that run leaves nothing of its own to find, and the memory is filled first
/// with words that are not zero, which stand for what an unfinished run
/// would have left.
#[test]
fn a_working_memory_holds_nothing_of_a_run_once_it_is_over() {
    let phc = hashed_by_argon2(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        small(64, 2, 1, None),
        A_SALT,
        RIGHT,
    );
    let mut memory = WorkingMemory::new();

    assert!(verify_password_in(&mut memory, RIGHT, &phc));
    assert_ne!(
        memory.block_count(),
        0,
        "the verify ran in no memory, so there is nothing to have been zeroed"
    );
    assert!(
        memory.is_all_zero(),
        "a verify that answered true left its run in the memory"
    );

    assert!(!verify_password_in(&mut memory, WRONG, &phc));
    assert!(
        memory.is_all_zero(),
        "a verify that answered false left its run in the memory"
    );

    memory.scribble();
    assert!(
        !memory.is_all_zero(),
        "filling the memory left it all zeros"
    );
    assert!(!verify_password_in(
        &mut memory,
        RIGHT,
        &with_field(&phc, SALT_FIELD, "somesalt")
    ));
    assert!(
        memory.is_all_zero(),
        "a run argon2 refused left the memory as it was handed over"
    );
}

/// What a run in these tests is unwound with when it is made not to finish.
///
/// It is thrown with `resume_unwind` and not `panic!`. That unwinds as a
/// panic does and drops what a panic would drop, without going through the
/// panic hook, so a test that passes prints nothing for it.
const UNFINISHED: &str = "a run that does not finish";

/// Fills the blocks a run at `params` is handed in `memory`, then unwinds
/// while still holding them: a run that panics part-way.
fn a_run_that_unwinds(memory: &mut WorkingMemory, params: &argon2::Params) -> ! {
    let blocks = memory.blocks_for(params);
    for block in blocks.0.iter_mut() {
        block.as_mut().fill(u64::MAX);
    }
    std::panic::resume_unwind(Box::new(UNFINISHED))
}

/// Asserts that `unwound` is what [`a_run_that_unwinds`] threw and not some
/// other panic the closure met on the way.
fn assert_unfinished(unwound: std::thread::Result<()>) {
    let payload = unwound.expect_err("the run finished");
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&UNFINISHED),
        "the run unwound with something other than what this test threw"
    );
}

/// **A run that unwinds leaves its memory zeroed, as a run that returns
/// does.**
///
/// The blocks are zeroed by the `Drop` of what a run is handed, so that no
/// way out of a run goes round it. A panic is the way out that a statement
/// placed after the run would miss, and the memory would then be kept with
/// the run still in it. Here a run is handed its blocks, writes to every one
/// of them and unwinds.
///
/// `AssertUnwindSafe`, because the closure borrows the memory mutably and
/// what the memory is left like by the unwind is the thing this reads.
#[test]
fn a_working_memory_holds_nothing_of_a_run_that_unwinds() {
    let params = small(64, 2, 1, None);
    let mut memory = WorkingMemory::new();

    assert_unfinished(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || a_run_that_unwinds(&mut memory, &params),
    )));

    assert_eq!(
        memory.block_count(),
        params.block_count(),
        "the run was handed no blocks, so there is nothing to have been zeroed"
    );
    assert!(
        memory.is_all_zero(),
        "a run that unwound left what it had written in the memory"
    );
}

/// **`Debug` says how much a working memory holds and prints none of it.**
///
/// Whatever holds one may derive `Debug` and be printed, and what would be
/// printed is every word of every block the memory holds.
#[test]
fn a_working_memory_does_not_print_its_blocks() {
    let phc = hashed_by_argon2(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        small(64, 2, 1, None),
        A_SALT,
        RIGHT,
    );
    let mut memory = WorkingMemory::new();
    assert!(verify_password_in(&mut memory, RIGHT, &phc));
    assert_ne!(
        memory.block_count(),
        0,
        "the memory holds nothing that could be printed"
    );

    assert_eq!(
        format!("{memory:?}"),
        format!("WorkingMemory {{ blocks: {} }}", memory.block_count())
    );
}

/// **`hash_password` stores the string `argon2`'s own hasher writes, byte for
/// byte.**
///
/// The string is what every later login is verified against, and it names
/// the algorithm, version, parameters and salt a verifier will hash with. So
/// a hash computed in kept memory has to be the hash `argon2` would have
/// computed in its own, and has to say the same about how it was made. A
/// random salt hides the comparison, so this goes through the function
/// `hash_password` hands its salt to, with salts chosen here, and puts each
/// to `PasswordHasher::hash_password` as well.
#[test]
fn a_password_hashed_in_kept_memory_is_the_string_argon2_writes() {
    use argon2::password_hash::{PasswordHasher as _, SaltString};

    let memories = KeptMemories::new();
    for (password, salt) in [
        (RIGHT, [0u8; SALT_BYTES]),
        ("another password entirely", *b"0123456789abcdef"),
    ] {
        let kept = hash_with_salt(&memories, password, &salt).expect("hashes");

        let encoded = SaltString::encode_b64(&salt).expect("the salt encodes");
        let argon2s_own = hasher()
            .hash_password(password.as_bytes(), &encoded)
            .expect("argon2 hashes")
            .to_string();

        assert_eq!(
            kept, argon2s_own,
            "hashing {password:?} in kept memory wrote a different string from argon2's \
             own hasher"
        );
    }
}

/// **The hash a run returns names the algorithm, version and parameters the
/// run was computed with, whichever they are.**
///
/// `hash_password` only ever asks for this module's, so a string that named
/// this module's whatever had been computed would read right in every hash
/// the module stores. Here none of the three is this module's, nor the output
/// length, and the string is held against the one `argon2`'s own hasher
/// writes for the same password and salt.
#[test]
fn a_hash_names_what_it_was_computed_with() {
    use argon2::password_hash::SaltString;

    let algorithm = argon2::Algorithm::Argon2i;
    let version = argon2::Version::V0x10;
    let params = small(64, 3, 2, Some(24));
    let salt = SaltString::encode_b64(A_SALT).expect("the salt encodes");

    let named = hash_in(
        &mut WorkingMemory::new(),
        algorithm,
        version,
        params.clone(),
        RIGHT.as_bytes(),
        salt.as_salt(),
    )
    .expect("hashes")
    .to_string();

    assert_eq!(
        named,
        hashed_by_argon2(algorithm, version, params, A_SALT, RIGHT)
    );
}

/// How long a test waits for a hash that should not have had to wait.
///
/// **Only a wrong implementation makes use of it.** A hash that waits for
/// nothing is done in the time one hash takes.
const LIMIT: std::time::Duration = std::time::Duration::from_secs(20);

/// **Hashes that come one after another run in one kept memory, and leave it
/// zeroed.**
///
/// A hash takes a memory out of the store it is given and puts it back, so
/// the second finds the first one's and makes none, and the store never holds
/// two. What it holds is one run at this module's cost and nothing of any
/// run.
///
/// **How many memories are kept does not say how many were made.** A store
/// that threw away the memory it found and made another for every hash would
/// keep one all the same, and allocate and free a whole working memory each
/// time, which is what keeping one is there to stop. So the store counts the
/// memories it makes for a run that finds none to take, and that is read here
/// as well.
///
/// **Nor does that count say what a run gives back.** A store that put back
/// another memory of the same size, and not the one it had lent, would make
/// none for a run and allocate one after every run. So the last run here
/// marks the memory it is lent, and the memory kept is read for the mark.
///
/// **The store is this test's own.** `hash_password` hashes in the process's,
/// and under `cargo test` every test of this crate that sets a password is in
/// this process hashing in it too, so how many memories that one holds at any
/// moment is whatever those tests make it.
#[test]
fn hashes_one_after_another_run_in_one_kept_memory() {
    let needed = hasher().params().block_count();
    let memories = KeptMemories::new();
    assert!(
        memories.each(WorkingMemory::block_count).is_empty(),
        "a store nothing has hashed in already keeps a memory"
    );

    for password in ["first", "second", "third"] {
        hash_with_salt(&memories, password, &[7; SALT_BYTES]).expect("hashes");

        assert_eq!(
            memories.each(WorkingMemory::block_count),
            [needed],
            "after {password:?} the store does not keep one memory holding what one \
             run needs"
        );
        assert_eq!(
            memories.each(WorkingMemory::is_all_zero),
            [true],
            "after {password:?} the kept memory still holds its run"
        );
        assert_eq!(
            memories.made(),
            1,
            "by {password:?} the store had not made the one memory these hashes need and no other"
        );
    }

    // The growth count is the mark: it is the one thing on a memory that a
    // test can write and that another memory of the same size would not
    // carry, however that one was come by.
    const MARK: usize = usize::MAX;
    memories.lend(|memory| memory.grown = MARK);
    assert_eq!(
        memories.each(WorkingMemory::grown),
        [MARK],
        "the store keeps some other memory than the one it lent"
    );
}

/// **A memory lent to a run that unwinds goes back to its store, zeroed, and
/// the next hash takes it.**
///
/// The memory is put back by the `Drop` of what holds it while it is out, so
/// a run that panics returns it as one that finishes does. Put back by a
/// statement after the run, it would be dropped by the unwind, and the next
/// hash would allocate a working memory the store had already had. The run
/// here writes to every block it is handed before it unwinds, and what comes
/// back holds none of that.
#[test]
fn a_kept_memory_goes_back_to_its_store_when_its_run_unwinds() {
    let needed = hasher().params().block_count();
    let memories = KeptMemories::new();
    hash_with_salt(&memories, "first", &[7; SALT_BYTES]).expect("hashes");

    assert_unfinished(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || memories.lend(|memory| a_run_that_unwinds(memory, hasher().params())),
    )));

    assert_eq!(
        memories.each(WorkingMemory::block_count),
        [needed],
        "after a run that unwound the store does not keep the one memory it had lent"
    );
    assert_eq!(
        memories.each(WorkingMemory::is_all_zero),
        [true],
        "the memory that came back from a run that unwound still holds what it wrote"
    );

    hash_with_salt(&memories, "second", &[7; SALT_BYTES]).expect("hashes");
    assert_eq!(
        memories.made(),
        1,
        "a hash that came after a run that unwound made a memory, when the store \
         should have had that run's to give it"
    );
    assert_eq!(memories.each(WorkingMemory::block_count), [needed]);
}

/// **`hash_password` hashes in the memories this process keeps.**
///
/// What a store does with a memory is held above, on a store of the test's
/// own. This holds the one thing left: that the process's store is the one
/// `hash_password` goes to. A count of the runs that store has lent a memory
/// to is something other tests hashing in the same process cannot make read
/// wrongly: it only rises, and this test's own hash has raised it. A
/// `hash_password` that hashed anywhere else would leave it where no hash had
/// been.
#[test]
fn hash_password_runs_in_the_memories_the_process_keeps() {
    hash_password("anything").expect("hashes");

    assert_ne!(
        HASH_MEMORIES.lent(),
        0,
        "a password was hashed and the process's store lent no memory for it"
    );
}

/// **A hash does not wait for another that is still running.**
///
/// The store's lock is held to take a memory out and to put one back, and not
/// across the run between. So while one run holds its memory, a second hash
/// goes through: it finds no memory to take, makes one, and finishes. Both
/// memories are then kept, and a hash that comes after them makes no third.
///
/// The first run here is a closure that waits for the second hash, standing
/// where `argon2` stands in a real one. A store that held its lock for as
/// long as a memory was out would keep that hash waiting for a run that is
/// waiting for it, and [`LIMIT`] is how long this gives it.
#[test]
fn a_hash_does_not_wait_for_a_run_that_is_holding_a_memory() {
    // `static` so that a thread can borrow it, and this test's alone.
    static MEMORIES: KeptMemories = KeptMemories::new();

    let second = MEMORIES.lend(|_first| {
        let (done, waited) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let hashed = hash_with_salt(&MEMORIES, RIGHT, &[7; SALT_BYTES]);
            let _ = done.send(hashed.is_ok());
        });
        waited.recv_timeout(LIMIT)
    });

    assert_eq!(
        second,
        Ok(true),
        "a hash did not finish while another run was holding its memory"
    );
    assert_eq!(
        MEMORIES.each(WorkingMemory::block_count).len(),
        2,
        "two runs at once did not leave the store keeping a memory for each"
    );
    // A memory for the run that was holding its own, and a memory for the
    // hash that found none to take meanwhile.
    assert_eq!(
        MEMORIES.made(),
        2,
        "two runs at once did not make a memory each and no more"
    );

    hash_with_salt(&MEMORIES, RIGHT, &[7; SALT_BYTES]).expect("hashes");
    assert_eq!(
        MEMORIES.each(WorkingMemory::block_count).len(),
        2,
        "a hash on its own left the store keeping some other number than the two \
         it found"
    );
    assert_eq!(
        MEMORIES.made(),
        2,
        "a hash on its own made a memory when two were there to take"
    );
}
