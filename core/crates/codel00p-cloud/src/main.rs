use std::env;

use codel00p_cloud::{
    AppState, ClerkDirectory, JwtVerifier, app, clerk_frontend_api_from_publishable_key,
    storage_from_env,
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let frontend_api = resolve_frontend_api()?;
    let issuer = format!("https://{frontend_api}");
    let jwks_url = format!("https://{frontend_api}/.well-known/jwks.json");

    println!("codel00p-cloud: loading Clerk signing keys from {jwks_url}");
    let verifier = JwtVerifier::from_clerk_jwks(&jwks_url, Some(issuer)).await?;

    // Connect off the async runtime: the blocking Postgres driver drives its own
    // runtime internally and panics if called from within one.
    let mut state = match tokio::task::spawn_blocking(storage_from_env).await?? {
        Some(storage) => {
            println!("codel00p-cloud: using DATABASE_URL-backed storage");
            AppState::with_storage(storage, verifier)
        }
        None => {
            println!("codel00p-cloud: in-memory storage (set DATABASE_URL for durability)");
            AppState::new(verifier)
        }
    };

    match ClerkDirectory::from_env() {
        Some(directory) => {
            println!("codel00p-cloud: Clerk organization directory enabled (GET /org/members)");
            state = state.with_directory(directory);
        }
        None => println!(
            "codel00p-cloud: no CLERK_SECRET_KEY - /org/members will report the directory \
             as unconfigured"
        ),
    }

    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8787);
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    println!("codel00p-cloud: listening on http://0.0.0.0:{port}");

    axum::serve(listener, app(state)).await?;
    Ok(())
}

/// Resolves the Clerk frontend API host from `CLERK_FRONTEND_API`, or derives it
/// from `CLERK_PUBLISHABLE_KEY`.
fn resolve_frontend_api() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    if let Ok(host) = env::var("CLERK_FRONTEND_API")
        && !host.trim().is_empty()
    {
        return Ok(host.trim().to_string());
    }

    let publishable_key = env::var("CLERK_PUBLISHABLE_KEY").map_err(|_| {
        "set CLERK_FRONTEND_API or CLERK_PUBLISHABLE_KEY so the service can verify Clerk tokens"
    })?;
    Ok(clerk_frontend_api_from_publishable_key(&publishable_key)?)
}
