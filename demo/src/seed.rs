use reqwest::Client;
use serde_json::{json, Value};

/// Seed the demo instance with realistic data via API calls.
/// Idempotent — skips if users already exist.
pub async fn run(base: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();

    // Check if already seeded by looking for alice
    let resp = client
        .post(format!("{}/api/auth/login", base))
        .json(&json!({"username": "alice", "password": "demo123"}))
        .send()
        .await?;
    if resp.status().is_success() {
        tracing::info!("Already seeded, skipping.");
        return Ok(());
    }

    tracing::info!("Seeding demo data...");

    // ── Register users ──────────────────────────────────────────────
    let users = vec![
        (
            "alice",
            Some("Rust enthusiast. Building distributed systems."),
        ),
        ("bob", Some("Full-stack dev. Coffee-powered.")),
        ("carol", Some("ML researcher. Neural nets and poetry.")),
        ("dave", Some("SRE by day, drummer by night.")),
        ("eve", Some("Security auditor. Zero trust.")),
        ("frank", Some("Open source maintainer. ASCII art lover.")),
        ("grace", Some("Backend engineer. Loves Postgres.")),
        ("hank", Some("Frontend magician. CSS is my canvas.")),
    ];

    let mut tokens: Vec<(String, String, String)> = Vec::new(); // (token, user_id, username)
    for (name, bio) in &users {
        let resp: Value = client
            .post(format!("{}/api/users/register", base))
            .json(&json!({"username": name, "password": "demo123", "bio": bio}))
            .send()
            .await?
            .json()
            .await?;
        let token = resp["access_token"].as_str().unwrap().to_string();
        let uid = resp["user"]["id"].as_str().unwrap().to_string();
        tokens.push((token, uid, name.to_string()));
        tracing::info!("  registered @{name}");
    }

    let tk = |name: &str| tokens.iter().find(|t| t.2 == name).unwrap().0.clone();
    let uid = |name: &str| tokens.iter().find(|t| t.2 == name).unwrap().1.clone();

    // ── Upload test images ─────────────────────────────────────────────
    let test_media_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/test-media");
    let image_files = vec!["red.png", "green.png", "blue.png", "amber.png"];
    let mut media_ids: Vec<String> = Vec::new();

    for fname in &image_files {
        let path = format!("{}/{}", test_media_dir, fname);
        let data = std::fs::read(&path)?;
        let part = reqwest::multipart::Part::bytes(data)
            .file_name(fname.to_string())
            .mime_str("image/png")?;
        let form = reqwest::multipart::Form::new().part("file", part);

        let resp: Value = client
            .post(format!("{}/api/media", base))
            .header("Authorization", format!("Bearer {}", tk("alice")))
            .multipart(form)
            .send()
            .await?
            .json()
            .await?;
        let mid = resp["id"].as_str().unwrap().to_string();
        media_ids.push(mid);
    }
    tracing::info!("  uploaded {} test images", image_files.len());

    // ── Create posts ────────────────────────────────────────────────
    let posts_data: Vec<(&str, &str, Option<&str>)> = vec![
        ("alice", "Just shipped a new distributed consensus library in Rust. The write path is 3x faster with zero-copy serialization. Blog post coming soon.", None),
        ("bob", "Hot take: Tailwind is good actually. But I still miss writing raw CSS sometimes.", None),
        ("carol", "Trained a small transformer on my chat logs. It now generates my commit messages with alarming accuracy.", None),
        ("dave", "Our pager went off at 3am because someone pushed a config with a trailing comma. JSON5 was a mistake.", None),
        ("eve", "Reviewed 12 smart contracts this week. The most common bug is still integer overflow. Use SafeMath. Please.", None),
        ("frank", "Released v3.0 of my CLI tool. Breaking changes:\n- --json is now --format json\n- removed 4 deprecated flags\n- improved error messages by 400%", None),
        ("grace", "PostgreSQL tip: use GENERATED ALWAYS AS IDENTITY instead of SERIAL. It's safer, more standard, and prevents accidental overwrites.", None),
        ("alice", "Unpopular opinion: async Rust is actually fine once you understand Pin. The learning curve is steep but the payoff is real.", None),
        ("hank", "Built an entire dashboard with just CSS Grid and a single div. No JavaScript. The div contains a 2000-line grid-template-areas string.", None),
        ("bob", "The best code is the code you delete. Just removed 200 lines from our auth middleware and nothing broke. Chef's kiss.", None),
        ("carol", "Experiment: replaced all our Python data pipeline with Polars. Went from 45 minutes to 8 seconds. The team is speechless.", None),
        ("eve", "If your threat model includes nation-states, you need hardware security modules. Software-only crypto is not enough. Thread.", None),
        ("frank", "ASCII art for your terminal prompt:\n  (\\__/)\n  (='.'=)\n  (\")_(\")\nYou're welcome.", None),
        ("grace", "Just discovered that PostgreSQL can do graph queries with recursive CTEs. My mind is blown. No need for Neo4j for simple cases.", None),
        ("dave", "Monitoring tip: alert on symptoms, not causes. Alert on 'user signup rate dropped' not 'disk is 80% full'. The user doesn't care about your disk.", None),
    ];

    let mut post_ids: Vec<(String, String)> = Vec::new();
    for (idx, (author, content, _parent)) in posts_data.iter().enumerate() {
        let auth_token = tk(author);
        let mut body = json!({"content": content});

        // Attach test images to select posts
        if idx == 0 && media_ids.len() > 0 {
            body["media_ids"] = json!([media_ids[0].clone()]);
        } else if idx == 1 && media_ids.len() > 1 {
            body["media_ids"] = json!([media_ids[1].clone()]);
        } else if idx == 12 && media_ids.len() > 2 {
            body["media_ids"] = json!([media_ids[2].clone(), media_ids[3].clone()]);
        }

        let resp: Value = client
            .post(format!("{}/api/posts", base))
            .header("Authorization", format!("Bearer {}", auth_token))
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        let pid = resp["id"].as_str().unwrap().to_string();
        post_ids.push((pid, author.to_string()));
    }
    tracing::info!("  created {} posts", post_ids.len());

    // ── Create replies (threads) ────────────────────────────────────
    // Reply to alice's first post (about distributed consensus)
    let alice_root = &post_ids[0].0;
    let auth_bob = tk("bob");
    client
        .post(format!("{}/api/posts", base))
        .header("Authorization", format!("Bearer {}", auth_bob))
        .json(&json!({"content": "Zero-copy is great but how do you handle alignment on ARM? We had issues with unaligned SIMD loads.", "parent_id": alice_root}))
        .send().await?;
    let reply1: Value = client
        .post(format!("{}/api/posts", base))
        .header("Authorization", format!("Bearer {}", tk("alice")))
        .json(&json!({"content": "We use a custom allocator that guarantees 16-byte alignment. It adds ~2% overhead but completely eliminates the unaligned load penalty.", "parent_id": alice_root}))
        .send().await?.json().await?;

    // Nested reply to alice's reply
    client
        .post(format!("{}/api/posts", base))
        .header("Authorization", format!("Bearer {}", tk("carol")))
        .json(&json!({"content": "Have you considered using std::simd? It's experimental but handles alignment automatically.", "parent_id": reply1["id"].as_str().unwrap()}))
        .send().await?;

    // Reply to bob's Tailwind post
    let bob_root = &post_ids[1].0;
    client
        .post(format!("{}/api/posts", base))
        .header("Authorization", format!("Bearer {}", tk("hank")))
        .json(&json!({"content": "I feel seen. Raw CSS is an art form. Utility classes feel like painting by numbers.", "parent_id": bob_root}))
        .send().await?;

    // Reply to dave's pager post
    let dave_root = &post_ids[3].0;
    client
        .post(format!("{}/api/posts", base))
        .header("Authorization", format!("Bearer {}", tk("grace")))
        .json(&json!({"content": "This is why we use YAML for configs. Wait no, YAML has its own demons. Do we just use TOML for everything now?", "parent_id": dave_root}))
        .send().await?;

    tracing::info!("  created 5 replies across 3 threads");

    // ── Create subscriptions ────────────────────────────────────────
    let subs = vec![
        ("alice", "bob"),
        ("alice", "carol"),
        ("alice", "frank"),
        ("bob", "alice"),
        ("bob", "grace"),
        ("carol", "eve"),
        ("carol", "alice"),
        ("eve", "carol"),
        ("eve", "grace"),
        ("frank", "alice"),
        ("frank", "bob"),
        ("grace", "dave"),
        ("hank", "bob"),
    ];
    for (subscriber, target) in &subs {
        let tid = uid(target);
        client
            .post(format!("{}/api/relationships/{}/subscribe", base, tid))
            .header("Authorization", format!("Bearer {}", tk(subscriber)))
            .send()
            .await?;
    }
    tracing::info!("  created {} subscriptions", subs.len());

    // ── Create connections ──────────────────────────────────────────
    let conns = vec![("alice", "bob"), ("carol", "eve"), ("frank", "grace")];
    for (a, b) in &conns {
        let bid = uid(b);
        let aid = uid(a);
        // A sends request to B
        client
            .post(format!("{}/api/relationships/{}/connect", base, bid))
            .header("Authorization", format!("Bearer {}", tk(a)))
            .send()
            .await?;
        // B accepts
        client
            .post(format!("{}/api/relationships/{}/accept", base, aid))
            .header("Authorization", format!("Bearer {}", tk(b)))
            .send()
            .await?;
    }
    tracing::info!("  created {} mutual connections", conns.len());

    tracing::info!(
        "Seed complete: {} users, {} posts, {} replies, {} subs, {} connections",
        users.len(),
        posts_data.len(),
        5,
        subs.len(),
        conns.len()
    );
    Ok(())
}
