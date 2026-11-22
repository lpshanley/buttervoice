#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Environment,
    Saved,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey {
    pub key: String,
    pub source: KeySource,
}

/// Leaves the key itself out, so it cannot end up in a log or a panic.
impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKey")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

/// The API key to use, if there is one. A key in the environment wins, and
/// `saved` is then never called: looking up the saved key can raise a
/// Keychain prompt, which a development run should not trigger.
pub fn choose(
    from_environment: Option<String>,
    saved: impl FnOnce() -> Option<String>,
) -> Option<ApiKey> {
    present(from_environment, KeySource::Environment).or_else(|| present(saved(), KeySource::Saved))
}

fn present(key: Option<String>, source: KeySource) -> Option<ApiKey> {
    key.map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .map(|key| ApiKey { key, source })
}
