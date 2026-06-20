/// Receives incremental assistant text as it streams back from a provider.
///
/// Implementations must be cheap and non-blocking: the transport calls
/// [`TokenSink::on_token`] from inside the response read loop, once per decoded
/// delta, in order. Callers that do not care about streaming can ignore this
/// trait entirely and use the non-streaming completion path.
pub trait TokenSink: Send + Sync {
    fn on_token(&self, token: &str);

    /// Receives an incremental tool-call argument fragment as it streams back
    /// from a provider, before the call is fully assembled.
    ///
    /// Transports that stream tool calls call this once per decoded tool-call
    /// delta, in order, alongside accumulating the final call. `index` is the
    /// position of the tool call within the response (a single streamed message
    /// may build several in parallel). `id` and `name` carry whatever the
    /// provider attached to *this* delta — typically present on the first
    /// fragment of a given call and `None` on subsequent argument-only
    /// fragments. `args_fragment` is the raw JSON-argument text for this delta
    /// (possibly empty when only `id`/`name` arrived).
    ///
    /// The default implementation does nothing, so existing sinks that only
    /// care about assistant text are unaffected and transports that do not
    /// stream tool calls never need to call it.
    fn on_tool_call_delta(
        &self,
        index: usize,
        id: Option<&str>,
        name: Option<&str>,
        args_fragment: &str,
    ) {
        let _ = (index, id, name, args_fragment);
    }
}

impl<F> TokenSink for F
where
    F: Fn(&str) + Send + Sync,
{
    fn on_token(&self, token: &str) {
        self(token)
    }
}
