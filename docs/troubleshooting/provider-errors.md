# Provider & model errors

When a chat turn fails because of the inference provider or model, codel00p
prints a short, coded message (e.g. `error[CL0001]`) with a link back to the
matching section here. Each section explains what happened and how to fix it.

Quick reference:

| Code | Meaning |
| ---- | ------- |
| [CL0001](#cl0001) | The model doesn't support tool use |
| [CL0002](#cl0002) | The conversation exceeds the model's context window |
| [CL0003](#cl0003) | The provider is rate-limiting the model (HTTP 429) |
| [CL0004](#cl0004) | The provider rejected your credentials (HTTP 401) |
| [CL0005](#cl0005) | The provider doesn't recognise the model id (HTTP 404) |

Configure the provider and model with `codel00p config providers` — see
[Inference Providers](../providers.md).

---

<a id="cl0001"></a>

## CL0001 — the model doesn't support tool use

**What happened.** codel00p's agent works by calling tools (reading files,
running commands, editing, searching). It therefore needs a *chat* model that
exposes a tool-capable endpoint. Rerank, embedding, and some vision-only models
can't call tools, so the provider rejects the request — on OpenRouter this comes
back as `404 No endpoints found that support tool use`.

**How to fix it.**

- Switch to a tool-capable model:

  ```sh
  codel00p config providers use <provider> --model <model>
  ```

- Or switch the model for later turns from inside chat:

  ```
  /model <model>
  ```

- Pick a model that advertises tool/function calling. On
  [OpenRouter](https://openrouter.ai/models), filter the catalog by
  **Tools**; good free options at time of writing include
  `openai/gpt-oss-120b:free` and `qwen/qwen3-coder:free`.

---

<a id="cl0002"></a>

## CL0002 — the conversation is larger than the context window

**What happened.** The request exceeded the model's maximum context length.
This is usually a long conversation, or a single large tool output (for example
reading a very big file).

**How to fix it.**

- Start a fresh chat. A bare `codel00p` always begins a **new** session, so just
  relaunch — or inside chat run `/reset`.
- Resume a specific past conversation deliberately with
  `codel00p agent chat --session-id <id>` (list them with `/sessions`).
- Use a model with a larger context window:

  ```sh
  codel00p config providers use <provider> --model <model>
  ```

---

<a id="cl0003"></a>

## CL0003 — the provider is rate-limiting the model

**What happened.** The provider temporarily refused the request with HTTP 429.
Free and shared models are rate-limited and hit this when busy.

**How to fix it.**

- Wait a few seconds and send the message again.
- Add your own provider API key for higher, dedicated limits:

  ```sh
  codel00p config providers set-key <provider>
  ```

- Switch to a less-busy (often non-`:free`) model:

  ```sh
  codel00p config providers use <provider> --model <model>
  ```

---

<a id="cl0004"></a>

## CL0004 — the provider rejected your credentials

**What happened.** The provider returned HTTP 401: the API key is missing,
incorrect, or doesn't have access to the requested model.

**How to fix it.**

- Set or update the key (stored in `~/.codel00p/.env`, never in `config.toml`):

  ```sh
  codel00p config providers set-key <provider>
  ```

- Check which environment variable / key is being used:

  ```sh
  codel00p config providers show <provider>
  ```

---

<a id="cl0005"></a>

## CL0005 — the model id was not found

**What happened.** The provider returned HTTP 404 'model not found': the model
id is misspelled, has been retired, or isn't available on your account.

**How to fix it.**

- Inspect the currently configured model:

  ```sh
  codel00p config providers show <provider>
  ```

- Set a valid model id:

  ```sh
  codel00p config providers use <provider> --model <model>
  ```
