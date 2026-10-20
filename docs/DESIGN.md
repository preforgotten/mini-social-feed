# Design Reference

> **Single source of truth for architecture, decisions, and conventions.**
> Sub-agents: read this before making changes. Update it after significant
> design shifts.

---

## Positioning

This is a **capability-based application backend** — not a database, not a
raw data server. It provides typed, validated, policy-governed primitives that
applications compose. The server knows what each shape *is* (validates media
types, enforces post limits, checks relationship uniqueness). It does not know
what the shapes *mean* to your application.

| Primitive | Trait(s) | What the server enforces | Application defines |
|---|---|---|---|
| **Users** | `UserStore`, `AuthPort` | Username uniqueness, password hashing, nostr key uniqueness, rate tier | What a user *is* — person, agent, sensor |
| **Posts** | `PostStore` | 16KB text cap, parent/child threading, optional nostr signature verification | What a post *means* — tweet, build log, command |
| **Media** | `MediaStore` | Type-validated binary (JPEG/PNG/WebP/GIF), per-tier size quotas, owner-tracked | How media is displayed, composed |
| **Relationships** | `GraphStore` | Typed directed edges (subscribe, connect, block, mention), idempotent | What "subscribe" implies — follow, watch, trust |
| **Events** | `EventPort` | System event stream (post created, user registered, reply received) via SSE | How events are consumed — notifications, analytics |
| **Policy** | `RateLimitConfig`, `MediaPolicy` | Per-identity, per-action rate limits; per-tier media size/count quotas | How limits map to user expectations |

Homeserver-first by default: all post reads require authentication. Content is
private unless the author opts into Nostr event signing. The same binary runs as
a single-process homeserver or as multiple instances behind a load balancer.

## Port Architecture (capability-based)

Every capability is a trait. Data ports in `data/src/ports.rs`, application ports in
`application/src/ports.rs`. `AppState` holds `Arc<dyn Trait>` for each. Implementations
live in `data/` (memory, sqlite) and `infra/` (JWT, feeds, event bus).

```
AppState {
    users: Arc<dyn UserStore>,        // user profiles, nostr linking, user search
    posts: Arc<dyn PostStore>,         // post CRUD, cursor pagination, thread queries, post search
    graph: Arc<dyn GraphStore>,        // relationships (reply, subscribe, connect, block, mention)
    media_local: Option<Arc<dyn MediaStore>>,  // local disk media
    media_cloud: Option<Arc<dyn MediaStore>>,  // cloud media (future S3)
    media_local_policy: Option<MediaPolicy>,   // upload size/count quotas
    auth: Arc<dyn AuthPort>,           // JWT, refresh tokens, Schnorr challenges, API keys
    feed: Arc<dyn FeedPort>,           // chronological, subscribed (swappable for hot/trending)
    event: Arc<dyn EventPort>,         // pub/sub event stream (SSE endpoint at /api/events)
}
```

### Trait summary

| Trait | Layer | Purpose | Implementors |
|---|---|---|---|
| `UserStore` | data | register, get_profile, find_by_username, link_nostr_pubkey, update_profile, search_users, space_used | MemoryStore, SqliteStore |
| `PostStore` | data | create_post, get_post, get_posts (cursor-based), get_user_posts, get_thread_posts, search_posts | MemoryStore, SqliteStore |
| `GraphStore` | data | relate, unrelate, related, check_relation | MemoryStore, SqliteStore |
| `MediaStore` | data | upload, get_file, delete, get_url, resolve_batch | LocalMediaStore, (S3 future) |
| `AuthPort` | application | register, login, create_api_key, verify_api_key, create_token, verify_token, refresh_token flow, challenge flow | JwtService |
| `FeedPort` | application | get_feed(viewer, &PageParams) → Page<Post> | ChronologicalFeed, SubscribedFeed |
| `EventPort` | application | emit, subscribe | EventBus |

### Why Arc<dyn Trait> instead of generics?

Avoids parameterising AppState, Router, and every handler on concrete types.
MemoryStore and SqliteStore are single structs that impl all data ports — cloning
the Arc gives you a different trait object pointing to the same data. Zero
virtual-call overhead in practice for network-bound code.

## Relationship model

All entity connections flow through `GraphPort`. No ad-hoc `parent_id` maps
or `followers` lists in the store.

| Relation | From → To | Meaning |
|---|---|---|
| `Reply` | Post → Post | Thread parent-child |
| `Subscribe` | User → User | Unidirectional, no consent |
| `ConnectRequest` | User → User | Bidirectional request, pending |
| `Connected` | User → User | Bidirectional, accepted |
| `Block` | User → User | Explicit block |
| `Mention` | Post → User | Tag mention |

`post.parent_id` is retained as a denormalized cache for display and search
filtering. The authoritative relationship is in the graph.

## Communication modes (capability composition)

Applications are built by composing capabilities. The server doesn't know what
a "feed" or "notification" is — the capabilities are the primitives:

| Mode | Composes | Example |
|---|---|---|
| Broadcast (feed) | Graph + Content + Identity + Policy | User subscribes to authors → Content filtered by subscriptions → Identity for profiles → Policy for rate limits |
| Pub/Sub (notifications) | Events + Graph + Content + Identity | New reply event → Graph finds who's subscribed → Content fetches the post → Identity for the sender |
| Direct (messaging) | Graph + Content + Identity + Policy | Connected users exchange directed posts → Policy enforces visibility |
| Agent pipeline | Identity + Content + Events | Agent registers → publishes results → downstream subscribers react to events |

Each mode is a composition on top of the capabilities — new modes don't need
new traits, just new orchestrators using the existing ones.

## Visibility model (future)

Posts carry an optional `visibility` field:

| Tier | Who can see |
|---|---|
| `public` | Any authenticated user |
| `subscribers` | Users who subscribed to the author |
| `connections` | Mutual connections of the author |
| `private` | Only the author |

Default is `public`. Higher tiers compose with the relationship model.

## Crate dependency graph

```
domain/          — EntityId, UserId, PostId, User, Post, Relation, Direction, DomainError
                   Deps: serde, uuid, thiserror. Zero knowledge of HTTP/crypto/persistence.

application/     — IdentityPort, Store, GraphPort, EventPort, AuthPort, FeedPort, AppError
                   Deps: domain. Zero knowledge of implementations.

infra/           — MemoryStore, SqliteStore, JwtService, ChronologicalFeed, crypto
                   Deps: domain, application. All impls live here.

server/          — Axum 0.7 routes, AppState, AuthUser extractor, HttpError mapping
                   Deps: all three. Thin HTTP adapter.

main.rs          — Wiring only.
```

No circular dependencies. Domain is pure. Application is pure ports.

## Configuration

**`config.toml`** — committed to git, contains structure only. No secrets.

```toml
[server]
host = "0.0.0.0"
port = 3000

[database]
backend = "sqlite"          # "memory" | "sqlite"
sqlite_path = "data/app.db"

[media]
backend = "local"           # "local" | "s3" (future)
policy = "standard"         # "minimal" | "standard" | "generous"
local_path = "uploads"
base_url = "http://localhost:3000"
```

**Secrets** — NEVER in config.toml. Set via environment variables at startup:

| Variable | Purpose | Required? |
|---|---|---|
| `JWT_SECRET` | HS256 signing key for access tokens | ✅ Required — server crashes if unset |
| `S3_ACCESS_KEY` | S3 access key (future) | Only when `media.backend = "s3"` |
| `S3_SECRET_KEY` | S3 secret key (future) | Only when `media.backend = "s3"` |

**Local dev:** copy `.env.example` to `.env` (gitignored) and fill in values.
**Production:** set via systemd Environment=, Docker -e, or k8s secrets.

**Test-only exception:** `STORE=sqlite` env var in conformance tests — this is
not production config, it's a test harness switch.

## Auth summary

- **Password auth**: argon2id hashing, `POST /api/auth/login` (username + password → tokens)
- **API keys**: SHA-256 hashed, `POST /api/auth/api-key` (authenticated → raw key)
- **Access tokens**: 15-min HS256 JWT, stateless
- **Refresh tokens**: opaque UUID, SHA-256 hashed before storage, 7-day TTL
- **Token rotation**: one-time-use refresh, rotated on each use, stale-token detection revokes family
- **Logout**: revokes entire token family
- **Nostr linking**: Schnorr challenge flow (BIP340), 5-min TTL, one-time-use
- **Agent auth**: API keys used as Bearer tokens directly (no JWT needed)
- **AuthUser extractor**: tries API key first (via IdentityPort), falls back to JWT (via AuthPort)

## Test conventions

| Layer | Location | Count | Command |
|---|---|---|---|
| Unit (domain) | `crates/domain/src/` | 2 | `cargo test -p mini-social-feed-domain` |
| Unit (data) | `crates/data/src/*.rs` `#[cfg(test)]` | 15 | `cargo test -p mini-social-feed-data` |
| Unit (infra) | `crates/infra/src/*.rs` `#[cfg(test)]` | 6 | `cargo test -p mini-social-feed-infra` |
| Unit (server) | `crates/server/src/routes/posts.rs` | 2 | `cargo test -p mini-social-feed-server` |
| Conformance | `tests/conformance/` | 51 | `cargo test --test conformance` |
| Evil | `tests/evil/` | 18 | `cargo test --test evil` |
| **Total** | | **94** | `cargo test --workspace` |

Shared helpers: `tests/common/mod.rs` (spawn_server, register_user, auth_header).
Both conformance and evil import it via `#[path = "../common/mod.rs"] mod common;`.

Dual-backend: Set `STORE=sqlite` env var; default is MemoryStore.

## Code conventions

- `axum 0.7` uses `:param` syntax in routes, NOT `{param}` (that's 0.8).
- `FromRequestParts` in axum 0.7 requires manual `Pin<Box<dyn Future>>` return type.
- Schnorr verification: accept both 32-byte (x-only) and 33-byte (compressed) pubkeys.
- `sqlx 0.8` with `runtime-tokio` and `sqlite` features.
- All post reads require auth (`AuthUser` extractor). Only registration, profile
  lookup, challenge, verify, refresh, login, media serve, and health are public.

## Current limitations

- Feed is chronological only (no ranking) — `FeedPort` is swappable.
- No rate limiting — important before cloud deployment.
- No post visibility tiers — all posts are effectively server-wide.
- No post deletion endpoint — posts are immutable once created.
- Search does not support cursor pagination (returns flat arrays with offset/limit).
- SQLite uses `LIKE '%' || ? || '%'` for search — works but no full-text index.
- No verification that a `nostr_event_id` posted is a real Nostr event (only internal hashing check).

## Design decisions log

| Date | Decision | Rationale |
|---|---|---|
| 2024-07 | UUIDv7 for all ids | Time-sortable, globally unique, no hash collisions |
| 2024-07 | Username identity, not pubkey | Simpler onboarding; Nostr linking is optional |
| 2024-07 | All post reads auth-gated | Homeserver-first, private by default |
| 2024-07 | Schnorr-only for Nostr | Single scheme, matches Nostr (BIP340) |
| 2024-07 | Capability-based ports | Stable lower layers, features are compositions |
| 2024-07 | GraphPort for relationships | Subscribe/connect/block model requires it |
| 2024-07 | IdentityPort split from Store | Profiles and content are different capabilities |
| 2024-07 | Refresh tokens as bearer credentials | Standard OAuth2 model, not bound to access token |
| 2024-07 | 16KB content cap, 64-char username, 512-char bio | Reasonable defaults for text-first platform |
| 2024-07 | No tags/economy yet | Observe real usage before building tokenomics |
| 2024-07 | Cursor-based pagination (not offset) | Stable under concurrent writes, efficient SQL compound index use |
| 2024-07 | Data engine as separate crate | Persistence is a standalone capability, swappable backends (memory/sqlite/S3) |
| 2024-07 | Tiered rate limiting (profile × action) | Humans and agents need different throughput; action types should not share buckets |
| 2024-07 | In-memory rate limiter (no trait yet) | Single-instance is correct for sqlite. Extract trait when Redis is needed for multi-instance. |
| 2024-07 | Project repositioned as capability-based application backend | Not a database — typed primitives (Users, Posts, Media, Relationships) with explicit policy. Apps define meaning. |
