# Initiative 7: Execution Backends & Sandboxing

## Goal

Let codel00p run agent commands somewhere other than the bare local workspace —
in a container, on a remote host, or in an ephemeral sandbox — so untrusted or
unattended work is isolated and remote/cloud execution becomes possible.

## Why (Hermes reference)

Hermes abstracts execution behind **six terminal backends**: local, Docker, SSH
remote, Daytona (serverless), Singularity (containers), and Modal (serverless).
The agent core is unchanged; only the terminal backend swaps. It also layers
sandboxing via command approval, authorization, and container isolation.

## Current codel00p state

- The `run_command` tool executes programs locally with timeout and output
  limits, structured argv (not a shell string), and all paths resolved inside
  the workspace boundary.
- `PermissionScope::Shell` + `PermissionMode` gate execution, and decisions can
  be remembered.
- No container/VM/remote isolation; "sandboxing" today means the workspace path
  boundary + permission prompts.
- Sandboxing and security review are **already on the roadmap** (Milestone 8 /
  Stage 8); this initiative is the concrete design and broadens it to remote
  backends.

## Design

Introduce a `TerminalBackend` trait in `codel00p-harness` (or a new
`codel00p-exec` crate) that the `run_command` and editing tools route through.
The agent loop and tool contracts stay identical across backends.

### Backend trait
- `TerminalBackend`: `spawn(argv, env, cwd, limits) -> handle`, streamed stdout/
  stderr, exit status, file read/write within the backend's workspace, and
  teardown. Mirrors what tools already need from the local executor.
- Backends register as plugins ([#1](plugins-and-hooks.md)); config selects the
  active backend per project/session, layered like everything else.

### Backends (priority order for codel00p)
1. **Local** (existing) — refactored behind the trait.
2. **Docker** — run commands in a container with the workspace mounted; the
   highest-value isolation step and the natural home for unattended
   ([#5](scheduling-cron.md)) and sub-agent ([#4](subagents-delegation.md)) runs.
3. **SSH remote** — execute against a remote dev host/VM.
4. **Ephemeral cloud sandbox** — a Daytona/Modal-style provider for throwaway
   environments; pluggable so we are not locked to one vendor.

### Sandboxing layers
- Command **approval/authorization** already exists (permission system); keep it.
- **Container isolation** as defense-in-depth: even with shell allowed, a Docker
  backend limits blast radius — essential for scheduled/unattended and
  gateway-driven runs.
- Resource limits (cpu/mem/time/network) per backend.

### Governance fit
- Org policy can **require** an isolating backend for certain scopes (e.g.
  "unattended or gateway-initiated shell must run in Docker"), enforced through
  the same policy mechanism as provider presets.
- Backend selection and every command are audited in the event stream.

## Scope

### Phase 1 — Backend seam
- [ ] `TerminalBackend` trait; refactor local executor behind it (no behavior
      change; golden tests).
- [ ] Per-project/session backend selection in layered config.

### Phase 2 — Docker
- [x] Docker backend (`DockerBackend`) with workspace bind-mount, streamed I/O,
      resource limits (`--memory`/`--cpus`), `--network` mode, and host uid:gid
      mapping so workspace files stay host-owned. Each command runs in its own
      ephemeral `docker run --rm` container; timeout/kill run `docker kill` so no
      container is orphaned. Selected via `agent.execution_backend = "docker"`
      and configured under `[agent.docker]`.
- [ ] Org policy: require isolation for unattended/gateway shell scopes.
- [ ] Warm/long-lived session container to amortize per-command spin-up (today
      each command starts a fresh container — see the Performance risk below).

### Phase 3 — Remote & ephemeral
- [ ] SSH remote backend.
- [ ] Pluggable ephemeral cloud-sandbox backend (Daytona/Modal-style).

## Risks & open questions

- **Workspace fidelity**: file tools assume a local path; remote/container
  backends need a consistent virtual workspace abstraction so `read_file` /
  `apply_patch` work identically.
- **Performance**: container spin-up per command is too slow; keep a warm
  session per agent run.
- **Secret/credential reach**: a sandbox must not silently inherit host
  credentials; explicit, scoped passthrough only.
- **Cross-platform**: Docker availability varies; local stays the default.

## Dependencies

- [#1 Plugins & Hooks](plugins-and-hooks.md) (backends as plugins).
- Strong pairing with [#4 Sub-Agents](subagents-delegation.md) and
  [#5 Scheduling](scheduling-cron.md) (isolation for parallel/unattended work).
- Aligns with roadmap Milestone 8 (security review + sandboxing).

## Exit criteria

- The same agent and tools run unchanged against local, Docker, and at least one
  remote backend; org policy can require an isolating backend for risky scopes;
  and backend choice plus every command are audited.
