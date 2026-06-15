# Self-iteration roadmap — fast/modular update + guarded autonomous fix loop

Status: **Living plan** (created 2026-06-13 from a 5-agent survey; updated
2026-06-13 with per-item feasibility findings). Capability A is effectively
**complete for this deployment** — see "Implementation status" below.

> **This is a candidate plan, not a backlog.** A breadth-first research survey
> lists options and generic best practices; it does **not** prove each item is
> worth doing on _this_ system. Before acting on any phase, validate its
> feasibility against the concrete deployment (host ports, boot timing,
> migrations, edge networking, cache scoping). Two items below (Phase 1, Phase 2)
> looked reasonable in the survey but were **dropped after that check** — the
> reasons are recorded inline so nobody re-litigates them or treats this as a
> to-do list.

## Implementation status

| Item                                      | Status                                                                                                                                                     |
| ----------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 0 — per-service deploy + inner loop | **Shipped** (#602 `make deploy-server`/`deploy-orchestrator`; #603 `make dev-infra`/`backend-watch` + overridable `COMPOSE_PARALLEL_LIMIT`)                |
| Phase 3 — wider hot-config                | **Shipped** (#604 `PARTICIPANT_STALE_AFTER_SECS`/`PARTICIPANT_SWEEP_INTERVAL_SECS`; the server was already ~70 env flags so this was the one concrete gap) |
| Phase 1 — unify build context             | **Dropped** — ROI collapsed once Phase 0 shipped (below)                                                                                                   |
| Phase 2 — zero-downtime rolling deploy    | **Dropped** — measured cost ≫ benefit on this topology (below)                                                                                             |
| Capability B (Phases 4–6)                 | **Not started** — multi-quarter; needs its own scoping                                                                                                     |

## Verdict — split into two capabilities, do NOT conflate them

- **A) Fast / modular UPDATE mechanism — the high-ROI parts are DONE.** The hard
  build-speed parts were already wired (`cargo-chef` + BuildKit cache mounts +
  `mold` + a CI-tuned release profile), so a one-line change never recompiled
  dependencies. The real slowness was the deploy orchestration, and Phase 0/3
  fixed the part that mattered: per-service deploy (don't rebuild the whole stack
  for one service), a sub-minute non-Docker inner loop, and runtime-tunable
  liveness timers. Phase 1 and Phase 2 were investigated and **not worth doing**
  (see their entries) — capability A is effectively complete for this deployment.
- **B) Autonomous self-iteration LOOP — a multi-quarter OCEAN.** A genuinely
  useful _human-gated assistant_ (issue → agent fix → draft PR → human-confirm →
  staging deploy) is ~6–10 weeks. Unattended self-modification of production is
  explicitly out of scope — it would require relaxing the very security/human
  gates that make the platform governable, and today there is no scheduler, no
  merge primitive, and the CD workflow is a stub.

## Quick wins (cheapest high-value first moves)

1. **Per-service deploy** — `make deploy-server` instead of the whole external
   profile. Biggest single iteration-speed win; pure config (the build is already
   cargo-chef-fast). **Shipped (#602).**
2. **Overridable build parallelism** — `COMPOSE_PARALLEL_LIMIT` is now a Make
   variable (default 1 so constrained hosts don't OOM; capable hosts can raise
   it). **Shipped (#603).** Note: with per-service deploy this only matters for
   the now-rare full `make prod-ext` rebuild.
3. **Inner loop** — `make dev-infra` (Postgres/Redis/NATS only) + `make
backend-watch` (`cargo watch -x run`) for sub-minute save-on-change dev,
   skipping Docker. Frontend is already hot (`vite build` → webroot); ~70 runtime
   env flags flip with a restart, no rebuild. **Shipped (#603).**
4. ~~**`docker-rollout`** zero-downtime swap~~ — **dropped after investigation.**
   The server publishes a fixed host port `127.0.0.1:4003`, so docker-rollout's
   two-replica swap can't run (port conflict), and the measured deploy gap is
   only ~2–5s. See Phase 2.
5. **First slice of the loop** — an issue → agent → **draft PR** assistant (no
   merge, no deploy) reusing the existing task state machine, auto-spawn-on-assign,
   and the `/workspace` mount. Independently useful and the staging ground for the
   guardrails before any merge automation. (Capability B, not started.)

## Phases (each independently shippable, smallest-value-first)

### Capability A — fast/modular update

- **Phase 0 — Per-service deploy + inner-loop fast path** — **SHIPPED (#602/#603).**
  `make deploy-server`/`deploy-orchestrator` rebuild + recreate one service;
  `make dev-infra` + `make backend-watch` give a sub-minute non-Docker inner
  loop; `COMPOSE_PARALLEL_LIMIT` became overridable. CONTRIBUTING documents the
  loop and the already-hot frontend/flag paths.
- **Phase 1 — Unify server+orchestrator build context** — **DROPPED (ROI
  collapsed after Phase 0).** The double compile of the shared workspace only
  happens when **both** services are built together (`make prod-ext`). Once
  per-service deploy shipped (#602), the daily path is `make deploy-server` —
  one service, no duplicate — so Phase 1 now only speeds up the rare full-stack
  rebuild. Worse, a unified builder stage would compile **both** binaries even
  for a single-service deploy unless per-bin target logic is added. Cost (rewrite
  the Dockerfiles + `compose*.yml` + `publish-images.yml`, which is **not**
  validated by PR CI — only on push-to-main or `workflow_dispatch` — and risks
  the GHCR images OSS operators consume) stayed high while the benefit fell to
  near-zero on the common path. Revisit only if full rebuilds become frequent.
- **Phase 2 — Zero-downtime rolling deploy** — **DROPPED (cost ≫ benefit on this
  topology).** Measured: the Rust server boots in **~230 ms** (start → DB →
  migrations → `Listening on 0.0.0.0:4003`), so a `make deploy-server` recreate
  leaves a `:4003` gap of only ~2–5 s (container stop+start), not the 10–30 s the
  survey assumed; the `HEALTHCHECK` `start-period` is a status gate, not a
  traffic gate. The edge (`openresty proxy_pass http://127.0.0.1:4003`, no
  `proxy_next_upstream`) 502s during that window, but it is brief and only on an
  (now infrequent) deploy. Cost is far higher than "install a tool + a target":
  the server publishes a **fixed host port** `127.0.0.1:4003`, so
  `docker-rollout`'s two-replica swap cannot run (both replicas would bind
  `:4003`) — true zero-downtime would require removing the host port, adding a
  reverse proxy on the Docker network, and repointing openresty. Plus migrations
  run on **every** boot (`main.rs` `run_migrations`), so an overlap deploy needs
  backward-compatible migrations or a separate migrate step. Not worth it for a
  self-hosted staging with infrequent deploys; revisit only with a real
  zero-downtime SLA + high deploy frequency.
- **Phase 3 — Wider hot-config surface** — **SHIPPED (#604).** The server was
  already ~70 env flags; the one concrete gap was the hardcoded participant
  liveness timers (`DEFAULT_STALE_AFTER`/`DEFAULT_STALE_SWEEP_INTERVAL`, builders
  never called). `PARTICIPANT_STALE_AFTER_SECS` and `PARTICIPANT_SWEEP_INTERVAL_SECS`
  now tune the offline window without a rebuild (single source for both the PG
  sweep and the Phase 2 Redis presence TTL). Auth/security/migration logic is
  intentionally **not** hot-reloadable.

### Capability B — guarded autonomous loop (defer until A is done)

- **Phase 4 — Issue-triggered agent fix that opens a reviewable DRAFT PR** (L, 3–5
  weeks). The first autonomous arm: an issue deterministically spawns an agent
  that edits this repo's checkout, runs local checks, and opens a _draft_ PR — no
  merge, no deploy. The git/PR bridge, scoped credential model, and risk-tier
  circuit breaker are net-new and security-sensitive.
- **Phase 5 — Human-confirm merge gate + auto-deploy-to-STAGING on confirmed
  merge** (L, 2–4 weeks). A named human approves via the existing Review →
  Completed task transition; the system then merges and rolls the single changed
  service to staging, with rollback wiring. Staging only.
- **Phase 6 — Background triage + bounded auto-merge for the lowest-risk tier
  ONLY** (XL, multi-quarter, defer). A scheduler that triages issues and
  dispatches fix attempts, plus narrow, audited, slowly-widened auto-merge for the
  single lowest-risk tier. Production promotion stays the human-dispatched,
  soak-gated `promote-to-production.yml` workflow forever.

## Guardrails (non-negotiable)

1. **HUMAN-CONFIRM before any merge or prod deploy.** The loop opens a reviewable
   draft PR and stops. Production promotion stays human-dispatched + soak-gated
   forever; the loop never reaches prod unattended. Any future auto-merge is
   restricted to the single lowest-risk tier, all checks green, with a cooldown —
   never auth/security/migration paths.
2. **Scoped, short-lived agent credentials that cannot bypass the security
   contracts.** The git/PR bridge runs _outside_ the agent's MCP surface
   (create/prompt/status/destroy only) with a token that can push a branch and
   open a PR but cannot merge, approve, deploy, or touch prod. `security.rs`
   container validation (privileged / host-PID / host-net / forbidden caps+mounts
   / resource limits all hard-denied) is never relaxed. Per-agent NATS/HMAC
   isolation, the replay window, and `&TenantScope` org-scoping stay enforced.
3. **Every agent-authored PR passes the same gates as a human PR** — full CI
   (eslint/prettier/typecheck/unit/integration/coverage/`rustfmt`/`clippy -D
warnings`/`cargo test --workspace`/`npm audit`/dangerous-pattern + secret-leak
   scans/Trivy CRITICAL+HIGH), the Beginner UX PR-body gate, the version guard,
   the migration-manifest integrity guard, and the route DDD-boundary guard test.
   A risk-tier circuit breaker auto-routes sensitive paths (auth, `middleware.rs`,
   `mcp.rs`, `security.rs`, migrations) to CODEOWNERS. Forbid pushes to agent
   branches — open a new PR instead — to close the bot-branch injection failure
   mode.

## Risks

- **Treating a survey as a backlog.** The biggest lesson from this plan: a
  breadth-first research survey produces plausible-but-unvalidated items. Phase 1
  (ROI assumed full rebuilds were the norm — they weren't after #602) and Phase 2
  (assumed `docker-rollout` drops in — the fixed host port blocks it) both passed
  the survey and failed the feasibility check. Validate each phase against the
  concrete system before committing.
- **Conflating A and B.** Different costs, owners, risk profiles. Ship A's value
  without waiting on B.
- **Agentic refinement failure mode.** Agents merge narrow PRs well but
  ghost/abandon subjective-feedback PRs; without the creation-time circuit
  breaker + tight human gate the loop generates low-quality PRs that cost more
  review time than they save.
- **Over-promising autonomy.** The realistic deliverable is a guarded, human-gated
  assistant — not unattended self-modification of prod.
- **Migrations run on every boot** (`server` `run_migrations`). Any future deploy
  scheme with running-container overlap (the dropped Phase 2, or Capability B's
  auto-deploy) must keep migrations backward-compatible or run them as a separate
  ordered step — a non-backward-compatible migration would break the old
  container.

## Already-shipped primitives this reuses

- `cargo-chef` + BuildKit cache mounts + `mold` in every Rust Dockerfile;
  per-scope GHA cache in `publish-images.yml`.
- CLI-image auto-updater (#478–500) — an existing _self-update_ primitive
  (poller + pull + re-tag + idle-only roll, owner-gated, staging-gated).
- 70+ runtime env flags; frontend decoupled (`vite build` → webroot).
- Container/Host CLI agents + orchestration task state machine +
  auto-spawn-on-assign + `/workspace` mount.
- The break-glass merge policy, the full CI gauntlet, the Beginner UX gate, and
  the guard tests — the gates an agent-authored PR must already pass.

See also: `docs/adr/0008-orchestration-presence-liveness-scaling.md` (an example
of the flag-gated, measurement-gated rollout discipline this loop must inherit).
