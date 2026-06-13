# Self-iteration roadmap — fast/modular update + guarded autonomous fix loop

Status: **Plan / proposal** (2026-06-13). Researched by a 5-agent survey of the
build/deploy pipeline, existing self-update primitives, the agent/orchestration

- review/CI stack, and external best practices.

## Verdict — split into two capabilities, do NOT conflate them

- **A) Fast / modular UPDATE mechanism — a boilable LAKE (~1–2 weeks).** The hard
  parts are already done: `cargo-chef` + BuildKit registry/git/target cache
  mounts + `mold` + a CI-tuned release profile are wired in every Rust Dockerfile.
  A one-line change does **not** recompile dependencies. The slowness is the
  _deploy orchestration_, not the build: `make prod-ext` rebuilds the whole
  external stack serially (`COMPOSE_PARALLEL_LIMIT=1`) even for a one-service
  change, and server + orchestrator compile the shared workspace from two
  separate cache scopes (a double compile). These are config/Dockerfile fixes.
- **B) Autonomous self-iteration LOOP — a multi-quarter OCEAN.** A genuinely
  useful _human-gated assistant_ (issue → agent fix → draft PR → human-confirm →
  staging deploy) is ~6–10 weeks. Unattended self-modification of production is
  explicitly out of scope — it would require relaxing the very security/human
  gates that make the platform governable, and today there is no scheduler, no
  merge primitive, and the CD workflow is a stub.

**Recommendation: ship A in full first.** It pays for itself immediately and is
the precondition for fast iteration whether or not B ever ships.

## Quick wins (cheapest high-value first moves)

1. **Per-service deploy** — `make deploy-server` = `docker compose up -d --build
agentforge-server` instead of the whole external profile. Biggest single
   iteration-speed win; pure config, no architecture change (the build is already
   cargo-chef-fast). _(This is the path already used by hand this session.)_
2. **Parallelise multi-service builds** — drop/raise `COMPOSE_PARALLEL_LIMIT=1`;
   images already have independent GHA cache scopes.
3. **Document the inner loop** — `npm run server` (`cargo run --bin
agentforge-server`) + `cargo-watch`/`bacon` for sub-minute save-on-change dev,
   skipping Docker entirely. Frontend is already hot (`vite build` → webroot, no
   backend rebuild); 70+ runtime env flags flip with a restart, no rebuild.
4. **`docker-rollout`** (wowu/docker-rollout) as the drop-in zero-downtime swap
   for one service once Docker healthchecks exist.
5. **First slice of the loop** — an issue → agent → **draft PR** assistant (no
   merge, no deploy) reusing the existing task state machine, auto-spawn-on-assign,
   and the `/workspace` mount. Independently useful and the staging ground for the
   guardrails before any merge automation.

## Phases (each independently shippable, smallest-value-first)

### Capability A — fast/modular update

- **Phase 0 — Per-service deploy + inner-loop fast path** (S, 2–4 days). `make
deploy-server`/`deploy-orchestrator`; document the `cargo run` + watch inner
  loop and the already-hot frontend/flag paths. No new product surface.
- **Phase 1 — Unify server+orchestrator build context** (M, 3–6 days). Build both
  from one context/Dockerfile so the shared workspace compiles once, halving cold
  build time. Re-verify all four images **functionally**, not by "Image built"
  (the binary lives only in the target cache mount and must be `cp`'d out before
  the `RUN` ends).
- **Phase 2 — Zero-downtime per-service rolling deploy** (M, 4–7 days). Docker
  healthchecks + `docker-rollout`/proxy cutover so a redeploy is gap-free.
- **Phase 3 — Wider hot-config surface** (S–M, 3–5 days). Push more _safe_
  behavior behind runtime flags so more changes need only a restart. (Do **not**
  hot-reload auth/security/migration logic.)

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

- **Conflating A and B.** Different costs, owners, risk profiles. Ship A's value
  without waiting on B.
- **sccache over-reach.** On ephemeral CI runners the target cache mount is lost
  between builds; sccache needs a persisted/cloud backend or buys nothing. Treat
  as a _measured_ Phase-1 add-on only if cold builds remain the bottleneck.
- **Build-context unification regressing an image.** Re-verify all four images
  functionally; the binary must be `cp`'d out of the cache mount before `RUN` ends.
- **Blue/green doesn't cover DB migrations.** A non-backward-compatible migration
  breaks rolling deploy; gate any migration-bearing PR on a human and run
  migrations as a separate ordered step.
- **Agentic refinement failure mode.** Agents merge narrow PRs well but
  ghost/abandon subjective-feedback PRs; without the creation-time circuit
  breaker + tight human gate the loop generates low-quality PRs that cost more
  review time than they save.
- **Over-promising autonomy.** The realistic deliverable is a guarded, human-gated
  assistant — not unattended self-modification of prod.

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
