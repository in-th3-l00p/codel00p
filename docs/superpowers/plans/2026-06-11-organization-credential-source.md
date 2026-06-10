# Organization Credential Source Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use test-driven-development before implementation. Keep this as metadata plumbing only; do not add a live secret manager.

**Goal:** Let provider routes report when a credential came from an organization-managed source, without exposing secret values or changing transport behavior.

**Architecture:** Add an additive `InferenceClientBuilder::organization_credential(...)` helper that stores a credential with a safe `organization:<ref>` source label. Existing route resolution already emits `credential_source`, so transports and response shapes do not need to change.

**Tech Stack:** Rust, `codel00p-providers`, runtime resolution tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Runtime Test

**Files:**
- Modify: `core/crates/codel00p-providers/tests/runtime_resolution.rs`

- [x] **Step 1: Write the test**

Add a test that registers an organization credential for a provider alias, resolves the canonical provider, and asserts `route.credential_source == Some("organization:<ref>")`.

- [x] **Step 2: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test runtime_resolution client_builder_preserves_organization_credential_source
```

Expected: compile failure because the builder helper does not exist yet.

### Task 2: Implement Organization Credential Source

**Files:**
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [x] **Step 1: Add the builder helper**

Implement `organization_credential(provider, credential, organization_ref)` using source label `organization:<organization_ref>`.

- [x] **Step 2: Preserve existing canonicalization**

Rely on the existing `build()` canonicalization path so aliases work the same as `credential(...)`.

### Task 3: Document The Source Label

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document organization-managed credential injection and the safe route metadata label.

- [x] **Step 2: Update backlog status**

Mark route audit metadata as including organization credential source labels.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test runtime_resolution client_builder_preserves_organization_credential_source && cargo test -p codel00p-providers --test runtime_resolution && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add organization credential source` and push to `origin/main`.
