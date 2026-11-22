/// The API key to use, if there is one. A key in the environment wins, and
/// `saved` is then never called: looking up the saved key can raise a
/// Keychain prompt, which a development run should not trigger.
pub fn choose(
    from_environment: Option<String>,
    saved: impl FnOnce() -> Option<String>,
) -> Option<String> {
    present(from_environment).or_else(|| present(saved()))
}

fn present(key: Option<String>) -> Option<String> {
    key.map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}
