# Buzzing Goose Architecture

## Purpose

Buzzing Goose is a hybrid agent platform. Buzz remains the durable workspace, workflow, identity, audit, event, mobile/desktop, and orchestration foundation. Goose remains a separately installed execution runtime. The `buzz-goose` adapter invokes documented Goose commands and records policy and evidence around them.

```text
User/API/UI -> Universal Operator envelope -> Buzz planner/router/workflow
           -> selected agent genome -> buzz-goose adapter -> Goose runtime
           -> provider + MCP/tools
           -> independent verification -> evidence/artifact -> Buzz durable memory
```

## Reused unchanged

The integration does not modify Goose provider/model abstraction, agent loop, MCP implementation, extension system, recipes, CLI/session format, tool execution, built-in permissions, or tests. It calls Goose only through documented CLI or ACP boundaries.

Buzz retains its existing Postgres event/workflow persistence, auth, audit hash chain, ACP harness, native agent, MCP developer server, relay, and desktop/mobile clients.

## First vertical slice

The initial `buzz-goose` adapter admits a read-only repository-health task through an explicit execution envelope. It performs Goose version health detection; bounds task turns and runtime; captures real output; verifies the Goose process result; compares Git worktree status before and after; and creates a SHA-256 evidence receipt. A missing or unhealthy Goose runtime produces `BLOCKED: Goose runtime unavailable`, never simulated success.

The first adapter's filesystem artifact is real worker-plane persistence, but not a replacement for Buzz Postgres. The next phase adds Postgres execution/evidence projections and durable queue/checkpoint/cancellation state.

## Security boundary

An execution envelope contains tenant, actor, goal, plan, step, workspace, agent identity/version/lineage, capabilities, denied permissions, secret references, time/cost limits, revocation state, and verification requirements. No child agent inherits authority automatically. The initial repository inspection grants only repository read and explicitly denies write, network, and external publishing. Production sandboxing and egress enforcement are a subsequent worker-plane milestone; a prompt is never treated as access control.

## Phases

1. Real Goose CLI health and bounded read-only inspection with evidence.
2. Buzz Postgres execution/evidence indexes, queue, checkpoint, resume and cancellation.
3. Central policy/approval/revocation/quarantine and sandbox worker planes.
4. ACP/Goose Serve streaming, registered MCP tools and multi-runtime routing.
5. Quarantined agent import, SBOM/scans/canary/benchmark evidence and learning from verified outcomes.
