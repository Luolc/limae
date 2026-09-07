use super::{EngineState, FailureReason, Signs};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn signs_separate_rejected_credentials_from_an_unreachable_service() -> TestResult {
    let signs = Signs::new()?;
    assert_eq!(
        signs.classify("acme-cli: HTTP 401 Unauthorized while calling the gateway"),
        EngineState::Unauthorized
    );
    assert_eq!(
        signs.classify("acme-cli: getaddrinfo ENOTFOUND gateway.invalid"),
        EngineState::Unreachable
    );
    assert_eq!(
        signs.classify("acme-cli: the reactor ran out of synthetic widgets"),
        EngineState::Failed
    );
    Ok(())
}

#[test]
fn signs_are_case_insensitive_and_read_rejection_before_the_network() -> TestResult {
    let signs = Signs::new()?;
    for output in [
        "FORBIDDEN",
        "Invalid API key",
        "invalid_api_key",
        "you are Not Logged In",
        "please re-authenticate",
    ] {
        assert_eq!(
            signs.classify(output),
            EngineState::Unauthorized,
            "{output}"
        );
    }
    for output in [
        "ECONNREFUSED",
        "Network Is Unreachable",
        "request timed out",
    ] {
        assert_eq!(signs.classify(output), EngineState::Unreachable, "{output}");
    }
    assert_eq!(
        signs.classify("403 forbidden: the session timed out"),
        EngineState::Unauthorized
    );
    Ok(())
}

#[test]
fn states_carry_the_reference_wording_and_next_steps() {
    assert_eq!(EngineState::Ok.as_str(), "ok");
    assert_eq!(EngineState::Missing.as_str(), "not installed");
    assert_eq!(
        EngineState::Unauthorized.as_str(),
        "credentials rejected (401 / 403)"
    );
    assert_eq!(EngineState::NoCredentials.as_str(), "no credentials found");
    assert_eq!(EngineState::Unreachable.as_str(), "network unreachable");
    assert_eq!(EngineState::Failed.as_str(), "ran but failed");

    assert_eq!(EngineState::Ok.next_step(), None);
    assert_eq!(
        EngineState::Missing.next_step(),
        Some("install the CLI, or name another engine with --engine")
    );
    assert_eq!(
        EngineState::Unauthorized.next_step(),
        Some("log in to that CLI again")
    );
    assert_eq!(
        EngineState::NoCredentials.next_step(),
        Some("log in to that CLI once, or configure engine = 'custom'")
    );
    assert_eq!(
        EngineState::Unreachable.next_step(),
        Some("check the network, then retry")
    );
    assert_eq!(
        EngineState::Failed.next_step(),
        Some("run the CLI by hand once to see what it says")
    );

    assert_eq!(
        EngineState::Unreachable.describe(),
        "network unreachable — check the network, then retry"
    );
    assert_eq!(EngineState::Ok.describe(), "ok");
}

#[test]
fn reasons_carry_the_reference_tokens() {
    assert_eq!(
        [
            FailureReason::NoEngine,
            FailureReason::NotInstalled,
            FailureReason::TimedOut,
            FailureReason::Unreachable,
            FailureReason::Rejected,
            FailureReason::NonzeroExit,
            FailureReason::EmptyAnswer,
            FailureReason::UnreadableAnswer,
            FailureReason::Other,
        ]
        .map(FailureReason::as_str),
        [
            "no-engine",
            "not-installed",
            "timeout",
            "unreachable",
            "unauthorized",
            "exit",
            "empty",
            "unreadable",
            "other",
        ]
    );
    assert_eq!(FailureReason::TimedOut.to_string(), "timeout");
}
