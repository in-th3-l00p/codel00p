/// Receives incremental assistant text as it streams back from a provider.
///
/// Implementations must be cheap and non-blocking: the transport calls
/// [`TokenSink::on_token`] from inside the response read loop, once per decoded
/// delta, in order. Callers that do not care about streaming can ignore this
/// trait entirely and use the non-streaming completion path.
pub trait TokenSink: Send + Sync {
    fn on_token(&self, token: &str);
}

impl<F> TokenSink for F
where
    F: Fn(&str) + Send + Sync,
{
    fn on_token(&self, token: &str) {
        self(token)
    }
}
