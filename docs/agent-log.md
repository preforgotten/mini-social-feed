# Agent coordination log
# Each agent appends its status here after completing work.
# Format: [AGENT_NAME] DONE: what was done | TODO: what remains
# Read this file between agent batches to verify state.

=== SESSION START ===

[DATA-CRATE] DONE: created with UserStore, PostStore, GraphStore, MediaStore + MemoryStore + SqliteStore + LocalMediaStore + Config + MediaPolicy
[DATA-CRATE] VERIFY: cargo check ✅ | cargo test ✅ (15 tests)
[DATA-CRATE] TODO: application ports.rs needs old traits removed; infra needs JwtService rewrite; server routes need updating

[APP-PORTS] DONE: removed old traits, AuthPort has register/login/create_api_key/verify_api_key, kept FeedPort+EventPort
[APP-PORTS] VERIFY: cargo check ✅
[APP-PORTS] TODO: infra JwtService needs to implement new AuthPort methods; server routes need updating

[INFRA] DONE: JwtService implements new AuthPort methods (register/login/create_api_key/verify_api_key), old modules removed (config/media/media_policy/memory/sqlite), feed uses PostStore, subscribed_feed uses GraphStore+PostStore
[INFRA] VERIFY: cargo check ✅ | cargo test ✅ (6 tests)
[INFRA] TODO: server routes + demo + main + tests need updating

[DUAL-MEDIA-1] DONE: config split into [media.local] and [media.s3], AppState has media_local + media_cloud
[DUAL-MEDIA-1] VERIFY: cargo check ✅ | cargo test ✅ (77 tests)
[DUAL-MEDIA-1] TODO: route handler needs ?backend= param; per-user quotas

[DUAL-MEDIA-2] DONE: ?backend= routing, UserStore.get_space_used, MediaPolicy per-user limits, quota check in upload handler
[DUAL-MEDIA-2] VERIFY: cargo check ✅ | cargo test ✅ (77 tests)

[AVATAR-1] DONE: User.avatar_media_id, UserResponse.avatar_url, UserStore.set_avatar, MediaPolicy split into post_image + avatar with square constraint
[AVATAR-1] VERIFY: cargo check ✅ | cargo test ✅ (77 tests)
[AVATAR-1] TODO: POST /api/users/me/avatar route handler; frontend updates
[AVATAR-2] DONE: POST /api/users/me/avatar handler with policy validation, frontend avatar display + upload, avatar_url in responses
[AVATAR-2] VERIFY: cargo check ✅ | cargo test ✅ (77 tests) | frontend build ✅

[SOLID-UI] DONE: SolidJS rewrite — 6 pages, session context, modern CSS, full feature coverage
[SOLID-UI] VERIFY: npm run build ✅ | demo health check ✅

[FRONTEND-V3] DONE: complete rewrite — proper @solidjs/router v0.16, component architecture, original devtool command-center design
[FRONTEND-V3] DONE: session.js module-level signal, Sidebar/PostCard/Composer components, 6 pages with auth guards
[FRONTEND-V3] DONE: flat dark theme (#0a0a10/#14141a), amber accent (#f0a030), square avatars (4px radius), monospace touches
[FRONTEND-V3] VERIFY: npm build ✅ (0 errors) | cargo test ✅ (77 tests)

[POST-RESPONSE-FIX] DONE: media and author_avatar_url populated in all PostResponse call sites
[POST-RESPONSE-FIX] VERIFY: cargo check ✅ | cargo test ✅
