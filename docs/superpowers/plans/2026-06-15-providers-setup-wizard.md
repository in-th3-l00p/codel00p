# Slice: Guided `providers setup` wizard

Date: 2026-06-15
Roadmap: Milestone 1 (Local Agent Foundation) — first-run UX.
Backlog/Initiative: Desktop/Cloud/CLI interfaces — onboarding polish.

## Goal

Turn provider configuration into a friendly, step-by-step walk-through —
`codel00p config providers setup` — so a new user can go from nothing to a
working agent in one guided flow, instead of stitching together `providers use`
+ `set-key` by hand. Modelled on a polished setup-wizard UX.

## Why this slice

Onboarding is the first thing a new user hits. The pieces already existed
(provider catalog with rich metadata, the dotenv credential store, live
`list_models`, policy presets, layered settings) — this slice composes them into
one guided experience and is self-contained in `codel00p-cli`.

## Design

`providers_setup` in `core/crates/codel00p-cli/src/providers.rs` orchestrates:

1. **Provider** — a described menu (`display_name`, id, `[x]` for providers that
   already have a key, current default), chosen by number or id.
2. **Credential** — for `AuthType::ApiKey` providers, prompt for the key (stored
   in `~/.codel00p/.env`); the just-entered key is also pushed into the process
   env so the live model fetch below can use it. Non-key auth types are
   explained and skipped.
3. **Base URL** — optional override (blank keeps the provider default).
4. **Model** — when a key is available and the provider has a catalog, offer to
   fetch the live model list and pick by number; otherwise fall back to
   free-text entry, hinting the provider's known aux model.
5. **Policy preset** — optional, opt-in menu of `ProviderPolicy::presets()`.
6. **Scope** — save to user config (default) or the project config, then write
   `agent.provider/model/base_url/provider_policy_preset` and print a summary.

`codel00p config setup` now delegates to the same wizard. The pure decision
helpers (`render_provider_menu`, `resolve_provider_id`, `resolve_model_choice`,
`render_preset_menu`, `resolve_preset_id`, `render_setup_summary`) are unit
tested; the live fetch runs on a private current-thread runtime so the sync CLI
can call the async client, and degrades gracefully on any error.

## Tests (test-first)

Unit tests for the pure helpers: index/id/unknown provider resolution, model
index/text/blank/out-of-range, preset index/id/blank, menu marking
(configured + default), and summary field inclusion. Plus a manual piped smoke
test of the full flow against a throwaway `CODEL00P_HOME`.

## Out of scope

A ratatui full-screen wizard (this is line-based prompts), validating the key
with a live test call, and an interactive arrow-key selector.
