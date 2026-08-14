use super::*;

#[test]
fn a_directory_is_usable_and_has_nothing_to_say() {
    assert_eq!(unusable("/anywhere", true), None);
}

/// The message has to name the *cause*, which is never "unset" — `corpus`
/// only reaches this with a value in hand. The cause is almost always a
/// relative path meeting cargo's working directory.
#[test]
fn a_value_that_is_not_a_directory_names_the_working_directory_trap() {
    let message = unusable("corpus", false).expect("a non-directory is unusable");
    assert!(
        message.contains("TAIYAKU_CORPUS") && message.contains("corpus"),
        "the message must name the variable and the value it was given: {message}"
    );
    assert!(
        !message.contains("unset"),
        "the value is set; saying `unset` names the wrong cause: {message}"
    );
    assert!(
        message.contains("working directory") && message.contains("relative"),
        "the message must explain why a relative path misses: {message}"
    );
}

/// An absolute path that is simply not there gets the same treatment. It is
/// still a value that cannot be used, and still not an absence.
#[test]
fn an_absolute_path_that_is_not_there_is_unusable_too() {
    assert!(unusable("/no/such/corpus", false).is_some());
}
