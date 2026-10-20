# mini-social-feed

A **capability-based application backend** for identity, content, and relationships
with explicit policy. First-class support for humans and agents. Self-host or
cloud — same binary.

## Intent

The server provides typed, validated, policy-governed primitives — not raw data,
not a database. It knows what each shape *is* (validates media types, enforces
post limits, checks relationship uniqueness). It doesn't know what the shapes
*mean* to your application.

| Primitive | What the server enforces | Application decides |
|---|---|---|
| **Users** | Username uniqueness, password hashing, nostr key uniqueness, rate tier | What a user *is* — person, agent, sensor |
| **Posts** | 16KB text cap, parent/child threading, optional nostr signature verification | What a post *means* — tweet, build log, command |
| **Media** | Type-validated binary (JPEG/PNG/WebP/GIF), per-tier size quotas, owner-tracked | How media is displayed, composed, reused |
| **Relationships** | Typed directed edges (subscribe, connect, block, mention), idempotent creation | What "subscribe" implies — follow, watch, trust |
| **Events** | System event stream (post created, user registered, reply received) via SSE | How events are consumed — notifications, analytics, triggers |

A social feed, a notification system, a DM platform, an agent pipeline — these
are all compositions of the primitives above. The server doesn't know what a
"feed" is. It knows who you are, what you publish, who you're connected to,
and what you're allowed to do.

Homeserver-first by default: all post reads require authentication. Content is
private unless the author opts into Nostr event signing.

## Quick start

```bash
cp .env.example .env          # set JWT_SECRET
cargo run                      # server on :3000 (sqlite by default)

# Demo with seeded data + frontend
cd demo && make seed && make run
```

Open `http://localhost:3000` — login as `alice` / `demo123`.

## Architecture

```
domain/          ← pure models, cursor encoding, pagination types
data/            ← standalone data engine (UserStore, PostStore, GraphStore, MediaStore)
application/     ← capability ports (AuthPort, FeedPort, EventPort) — zero implementations
infra/           ← implementations (JwtService, RateLimiter, ChronologicalFeed, EventBus)
server/          ← thin axum 0.7 HTTP adapter (routes, middleware, AuthUser extractor)
```

No circular dependencies. See [docs/architecture.md](docs/architecture.md) for the full design.

## API (34 endpoints)

`[auth]` = requires `Authorization: Bearer <token>`

| Method | Path | Auth | Purpose |
|---|---|---|---|
| POST | `/api/users/register` | | Register human user |
| POST | `/api/auth/login` | | Login → get tokens |
| POST | `/api/auth/api-key` | auth | Create API key for agents |
| POST | `/api/auth/refresh` | | Rotate tokens |
| POST | `/api/auth/logout` | auth | Revoke token family |
| POST | `/api/auth/challenge` | | Start Nostr key-proof |
| POST | `/api/auth/verify` | | Verify challenge signature |
| GET | `/api/users/:id` | | User profile |
| GET | `/api/users/:id/posts` | auth | User's posts (cursor paginated) |
| GET | `/api/users/:id/followers` | auth | Follower list |
| GET | `/api/users/:id/following` | auth | Following list |
| PUT | `/api/users/me` | auth | Update profile |
| POST | `/api/users/me/nostr` | auth | Link Nostr pubkey |
| POST | `/api/users/me/avatar` | auth | Upload profile picture |
| POST | `/api/posts` | auth | Create post (optional Nostr signing) |
| GET | `/api/posts` | auth | Feed (cursor paginated) |
| GET | `/api/posts/:id` | auth | Single post |
| GET | `/api/posts/:id/thread` | auth | Thread tree |
| POST | `/api/media` | auth | Upload media |
| GET | `/api/media/:id` | | Serve media bytes |
| DELETE | `/api/media/:id` | auth | Soft-delete media |
| POST | `/api/search/users` | auth | Search users by username |
| POST | `/api/search/posts` | auth | Search posts |
| POST | `/api/relationships/:id/subscribe` | auth | Follow user |
| DELETE | `/api/relationships/:id/subscribe` | auth | Unfollow |
| POST | `/api/relationships/:id/connect` | auth | Request connection |
| DELETE | `/api/relationships/:id/connect` | auth | Remove connection |
| POST | `/api/relationships/:id/accept` | auth | Accept connection |
| POST | `/api/relationships/:id/block` | auth | Block user |
| DELETE | `/api/relationships/:id/block` | auth | Unblock |
| GET | `/api/relationships/subscribers` | auth | List your subscribers |
| GET | `/api/relationships/subscriptions` | auth | List your subscriptions |
| GET | `/api/relationships/connections` | auth | List your connections |
| GET | `/api/events` | | SSE event stream |
| GET | `/api/health` | | Health check |

### Pagination

All list endpoints use cursor-based pagination:

```
GET /api/posts?limit=30                          → first page
GET /api/posts?limit=30&before=<cursor>          → next page (older)
GET /api/posts?limit=30&since=1700000000         → time-window filtered
```

Response: `{ items: [...], total: 100, page_size: 30, first: "cursor", last: "cursor" }`

### Auth

- **Humans**: register with username+password (argon2id) → JWT (15 min) + refresh token (7-day, rotated)
- **Agents**: human creates API key via `POST /api/auth/api-key` → agent uses as `Bearer <key>`
- **Nostr linking**: optional Schnorr (BIP340) challenge flow to link a pubkey to a profile
- **Nostr event signing**: per-post opt-in — server verifies event ID + signature, post becomes a portable Nostr event

### Rate limiting

Tiered token-bucket per action, per identity. Configurable in `config.toml`:

```toml
[rate_limit.human.tiers.minimal.actions]
post_create = { max_burst = 5, refill_per_sec = 0.25 }
read = { max_burst = 30, refill_per_sec = 5.0 }

[rate_limit.agent.tiers.power.actions]
post_create = { max_burst = 200, refill_per_sec = 20.0 }
```

Three profiles: `public` (IP-scoped), `human` (user-scoped), `agent` (user-scoped).
Three tiers: `minimal` (default), `basic`, `power`. Disabled in tests.

## Backends

| Component | Options | Swappable via |
|---|---|---|
| Database | MemoryStore, SqliteStore (future: PostgresStore) | `UserStore`, `PostStore`, `GraphStore` traits |
| Media | LocalMediaStore (future: S3MediaStore) | `MediaStore` trait |
| Feed | ChronologicalFeed, SubscribedFeed (future: HotFeed) | `FeedPort` trait |
| Rate limiting | In-memory (future: Redis) | Embedded in server crate |
| Events | EventBus (tokio broadcast) | `EventPort` trait |

## Testing

**72 conformance+evil tests**, zero warnings, fmt + clippy clean.

```
tests/
├── common/mod.rs          ← shared helpers
├── conformance/           ← API contract validation (both backends)
│   ├── health.rs      (1)
│   ├── users.rs       (8)
│   ├── auth.rs        (11)
│   ├── posts.rs       (18)
│   ├── feed.rs        (1)
│   ├── events.rs      (2)
│   ├── relationships.rs (7)
│   ├── media.rs       (3)
│   └── rate_limit.rs  (3)
└── evil/                  ← robustness probes
    ├── bounds.rs      (8)
    ├── injection.rs   (7)
    └── races.rs       (4)
```

```bash
cargo test --workspace     # full suite (both backends)
STORE=sqlite cargo test    # validate sqlite backend
make ci                    # fmt + clippy + tests
```

## Roadmap

- [ ] Agent identity as first-class (separate registration endpoint, same User model)
- [ ] Post deletion
- [ ] Consistent error envelope
- [ ] Post visibility tiers (public / followers / connections / private)
- [ ] Notifications (consume EventPort → notification feed)
- [ ] Mentions (parse @username in post content)
- [ ] Graceful shutdown
- [ ] Full-text search
- [ ] DB migration versioning
