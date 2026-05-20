use clap::Subcommand;
use colored::Colorize;
use sqlx::sqlite::SqlitePool;

#[derive(Subcommand)]
pub enum KeyAction {
    /// Add a new exergy key
    Add {
        /// Key ID (if not provided, one will be generated)
        #[arg(short, long)]
        key: Option<String>,
        /// Owner identifier (e.g. username)
        #[arg(short, long)]
        owner: String,
        /// Tier (SOVEREIGN, COMMERCIAL, DEVELOPER)
        #[arg(short, long, default_value = "SOVEREIGN")]
        tier: String,
    },
    /// List all exergy keys
    List,
    /// Revoke an exergy key
    Revoke {
        /// Key ID to revoke
        key: String,
    },
}

pub async fn cmd_keys(action: KeyAction) {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:anvil.db".to_string());
    let pool = SqlitePool::connect(&db_url)
        .await
        .expect("Failed to connect to database");

    match action {
        KeyAction::Add { key, owner, tier } => {
            let key_id = key.unwrap_or_else(|| {
                format!("anvil-{}-{}", owner, &uuid::Uuid::new_v4().to_string()[..8])
            });
            match sqlx::query("INSERT INTO exergy_keys (key_id, owner_id, tier) VALUES (?, ?, ?)")
                .bind(&key_id)
                .bind(&owner)
                .bind(&tier)
                .execute(&pool)
                .await
            {
                Ok(_) => println!(
                    "  {} Added key: {} (Owner: {}, Tier: {})",
                    "✓".bright_green(),
                    key_id,
                    owner,
                    tier
                ),
                Err(e) => eprintln!("  {} Failed to add key: {}", "✗".bright_red(), e),
            }
        }
        KeyAction::List => {
            let rows =
                sqlx::query_as::<_, (Option<String>, String, Option<String>, Option<String>)>(
                    "SELECT key_id, owner_id, tier, status FROM exergy_keys",
                )
                .fetch_all(&pool)
                .await
                .expect("Failed to fetch keys");

            println!(
                "  {:<30} {:<15} {:<15} {:<10}",
                "KEY ID", "OWNER", "TIER", "STATUS"
            );
            println!("  {}", "-".repeat(80));
            for (key_id, owner_id, tier, status) in rows {
                println!(
                    "  {:<30} {:<15} {:<15} {:<10}",
                    key_id.as_deref().unwrap_or("UNKNOWN"),
                    owner_id,
                    tier.as_deref().unwrap_or("SOVEREIGN"),
                    status.as_deref().unwrap_or("ACTIVE")
                );
            }
        }
        KeyAction::Revoke { key } => {
            match sqlx::query("UPDATE exergy_keys SET status = 'REVOKED' WHERE key_id = ?")
                .bind(&key)
                .execute(&pool)
                .await
            {
                Ok(_) => println!("  {} Revoked key: {}", "✓".bright_green(), key),
                Err(e) => eprintln!("  {} Failed to revoke key: {}", "✗".bright_red(), e),
            }
        }
    }
}
