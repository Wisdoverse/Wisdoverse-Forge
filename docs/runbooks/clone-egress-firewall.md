# Clone egress firewall — REQUIRED operator step (project-git-clone)

> **Action required.** If your deployment lets users create projects from a git
> URL, you **MUST** apply an egress policy to the clone network described below.
> Without it, a hostile repository URL can reach your host's private network or
> the cloud metadata endpoint. This is the deploy-layer half of the clone SSRF
> control; the application already does its half.

## Who needs this

You need this runbook if **project create-from-git-URL is enabled** and your
host can reach an internal network or a cloud metadata service (almost every
cloud VM and most on-prem hosts). A laptop with no internal services to protect
can skip it, but applying the policy anyway costs nothing.

## What the platform already does (and what it cannot do)

When a user creates a project from a git URL, the backend launches a short-lived,
locked-down `agentforge-clone` container that clones exactly one repository and
exits. The application provides three layers of SSRF defense on its own:

- **URL gate (in-app).** Only `https://` URLs with a real host are accepted;
  embedded credentials (`user:token@host`) are rejected.
- **Network separation (in-app).** The clone container is attached to a
  dedicated Docker bridge network, `agentforge-clone-egress`, that the
  internal-service containers (PostgreSQL, NATS, the API, the orchestrator, …)
  are **not** on. On reuse the backend inspects the existing network and refuses
  to clone if it is not the managed, isolated egress bridge.
- **Host pre-resolve (in-container, best-effort).** Before `git clone` runs, the
  entrypoint resolves the URL's host and refuses (exit 2) if it resolves to
  loopback, RFC1918 (`10/8`, `172.16/12`, `192.168/16`), CGNAT (`100.64/10`),
  link-local / the metadata address `169.254.169.254`, or a `.local` mDNS name.

**What none of these can do:** Docker's stock bridge driver still routes packets
from the clone container to the host's other subnets, RFC1918 ranges, and the
metadata endpoint. The in-container pre-resolve is also best-effort against
**DNS rebinding** — a host that resolves to a public IP at check time can
re-resolve to a private IP at git's connect time. **Only a packet-level egress
firewall on the clone subnet closes these.** That is this runbook.

## Apply the egress policy

### Prerequisites

- The `agentforge-clone-egress` Docker network exists. The backend creates it on
  first clone; you can also create it ahead of time:

  ```bash
  docker network create \
    --driver bridge \
    --label agentforge.managed=clone-egress \
    agentforge-clone-egress
  ```

- You can run `iptables`/`nftables` on the Docker host (root) **or** you front
  egress with a proxy you control.

### Find the clone subnet

```bash
docker network inspect agentforge-clone-egress \
  --format '{{range .IPAM.Config}}{{.Subnet}}{{end}}'
# example output: 172.20.0.0/16
```

Use that subnet as `CLONE_SUBNET` below.

### Option A — nftables / iptables DOCKER-USER (recommended)

Docker honors the `DOCKER-USER` chain before its own rules, so policy there
survives container restarts. Drop traffic **from the clone subnet to private
ranges and the metadata IP**, while allowing the public internet:

```bash
CLONE_SUBNET=172.20.0.0/16   # from the inspect above

# Block the cloud metadata endpoint outright.
iptables -I DOCKER-USER -s "$CLONE_SUBNET" -d 169.254.169.254/32 -j DROP

# Block RFC1918 + link-local + CGNAT destinations.
for cidr in 10.0.0.0/8 172.16.0.0/12 192.168.0.0/16 169.254.0.0/16 100.64.0.0/10; do
  iptables -I DOCKER-USER -s "$CLONE_SUBNET" -d "$cidr" -j DROP
done

# Everything else (the public internet) is allowed by the default DOCKER-USER
# RETURN at the end of the chain.
```

Persist these with your distro's iptables-persistent / nftables service so they
survive a reboot. On an IPv6-enabled host, mirror the rules with `ip6tables`
(block `::1/128`, `fe80::/10`, `fc00::/7`).

### Option B — egress proxy

If you already route outbound traffic through a proxy, point the clone network at
a proxy that **denies private/metadata destinations** and allows only the git
hosts you support (e.g. `github.com`, `gitlab.com`). This is stricter (an
allow-list) and also defeats DNS rebinding, at the cost of maintaining the host
list.

## Verify the policy

A clone of a public repo must still succeed, and a URL pointing at a private
address must fail closed. From the host:

```bash
# Should be DROPPED (no route). If this times out / fails, the policy works.
docker run --rm --network agentforge-clone-egress curlimages/curl:latest \
  -s --max-time 5 http://169.254.169.254/ ; echo "exit=$?"

# Should SUCCEED (public internet reachable).
docker run --rm --network agentforge-clone-egress curlimages/curl:latest \
  -s --max-time 10 -o /dev/null -w '%{http_code}\n' https://github.com/
```

Then create a project from a public git URL through the product UI and confirm it
reaches **Ready**.

## Tracking / tests

- The in-app and in-container halves of this control ship with the
  project-git-clone feature (M3/M4).
- The **fails-closed SSRF integration test** is part of **milestone M8**:
  `rust/crates/api/tests/project_clone_security.rs::`
  `ssrf_internal_address_urls_are_rejected_at_create` proves the in-app deny-list
  rejects loopback / RFC1918 / link-local-metadata / `.local` / port-only repo
  URLs at create time, and `ssrf_normal_https_github_url_is_accepted` is the
  positive control. The in-app deny-list is the layer the API can enforce in
  process; the packet-level egress firewall in this runbook is the deploy-layer
  layer that closes DNS rebinding and stock-bridge routing, verified by the
  `Verify the policy` steps above (no in-process test can exercise the host's
  network rules).
