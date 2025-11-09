# Przykładowy Backend Serwera Licencyjnego (Rust + Axum)

Jeśli zdecydujesz się na własny backend zamiast Lemon Squeezy, oto kompletny przykład.

## Struktura Projektu

```
license-server/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── routes/
│   │   ├── mod.rs
│   │   └── licenses.rs
│   ├── models/
│   │   ├── mod.rs
│   │   ├── license.rs
│   │   └── device.rs
│   ├── db/
│   │   ├── mod.rs
│   │   └── schema.rs
│   └── middleware/
│       └── auth.rs
└── migrations/
```

## Cargo.toml

```toml
[package]
name = "license-server"
version = "0.1.0"
edition = "2021"

[dependencies]
# Web framework
axum = "0.7"
tokio = { version = "1", features = ["full"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }

# Database
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "chrono", "uuid"] }
uuid = { version = "1.0", features = ["v4", "serde"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Time
chrono = { version = "0.4", features = ["serde"] }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Environment
dotenvy = "0.15"

# Crypto
sha2 = "0.10"
hex = "0.4"
rand = "0.8"

# Stripe (for payments)
async-stripe = { version = "0.35", features = ["runtime-tokio-hyper"] }
```

## Database Schema (PostgreSQL)

```sql
-- migrations/001_initial.sql

CREATE TABLE licenses (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    license_key VARCHAR(255) UNIQUE NOT NULL,
    email VARCHAR(255) NOT NULL,
    tier VARCHAR(50) NOT NULL, -- 'Free', 'Pro'
    license_type VARCHAR(50) NOT NULL, -- 'Subscription', 'Lifetime'
    stripe_subscription_id VARCHAR(255), -- null dla lifetime
    expires_at TIMESTAMP WITH TIME ZONE, -- null dla lifetime
    max_devices INTEGER NOT NULL DEFAULT 5,
    status VARCHAR(50) NOT NULL DEFAULT 'active', -- 'active', 'cancelled', 'expired'
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE TABLE devices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    license_id UUID NOT NULL REFERENCES licenses(id) ON DELETE CASCADE,
    device_id VARCHAR(255) NOT NULL, -- hashed fingerprint z klienta
    device_name VARCHAR(255) NOT NULL,
    os VARCHAR(50) NOT NULL,
    last_validated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    activated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(license_id, device_id)
);

CREATE TABLE validation_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    license_id UUID NOT NULL REFERENCES licenses(id) ON DELETE CASCADE,
    device_id UUID REFERENCES devices(id) ON DELETE SET NULL,
    success BOOLEAN NOT NULL,
    error_message TEXT,
    ip_address VARCHAR(50),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_licenses_license_key ON licenses(license_key);
CREATE INDEX idx_licenses_email ON licenses(email);
CREATE INDEX idx_devices_license_id ON devices(license_id);
CREATE INDEX idx_validation_logs_license_id ON validation_logs(license_id);
CREATE INDEX idx_validation_logs_created_at ON validation_logs(created_at);
```

## Models (src/models/)

```rust
// src/models/license.rs
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum LicenseTier {
    Free,
    Pro,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum LicenseType {
    Subscription,
    Lifetime,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum LicenseStatus {
    Active,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct License {
    pub id: Uuid,
    pub license_key: String,
    pub email: String,
    pub tier: LicenseTier,
    pub license_type: LicenseType,
    pub stripe_subscription_id: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_devices: i32,
    pub status: LicenseStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl License {
    pub fn is_valid(&self) -> bool {
        if !matches!(self.status, LicenseStatus::Active) {
            return false;
        }

        match self.license_type {
            LicenseType::Lifetime => true,
            LicenseType::Subscription => {
                self.expires_at
                    .map(|exp| Utc::now() < exp)
                    .unwrap_or(false)
            }
        }
    }
}

// src/models/device.rs
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Device {
    pub id: Uuid,
    pub license_id: Uuid,
    pub device_id: String,
    pub device_name: String,
    pub os: String,
    pub last_validated_at: DateTime<Utc>,
    pub activated_at: DateTime<Utc>,
}

// Request/Response DTOs
#[derive(Debug, Deserialize)]
pub struct ValidateLicenseRequest {
    pub license_key: String,
    pub device: DeviceInfo,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub device_name: String,
    pub os: String,
}

#[derive(Debug, Serialize)]
pub struct ValidateLicenseResponse {
    pub valid: bool,
    pub license: Option<LicenseInfo>,
    pub device: Option<DeviceInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LicenseInfo {
    pub id: String,
    pub tier: LicenseTier,
    pub license_type: LicenseType,
    pub expires_at: Option<DateTime<Utc>>,
    pub email: String,
}
```

## Routes (src/routes/licenses.rs)

```rust
use axum::{
    extract::{State, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use sqlx::PgPool;
use chrono::Utc;
use uuid::Uuid;

use crate::models::{
    ValidateLicenseRequest,
    ValidateLicenseResponse,
    LicenseInfo,
    DeviceInfo,
    License,
    Device,
    LicenseStatus,
};

pub async fn validate_license(
    State(pool): State<PgPool>,
    Json(payload): Json<ValidateLicenseRequest>,
) -> Result<Json<ValidateLicenseResponse>, StatusCode> {

    // 1. Znajdź licencję
    let license = sqlx::query_as::<_, License>(
        "SELECT * FROM licenses WHERE license_key = $1"
    )
    .bind(&payload.license_key)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let Some(license) = license else {
        return Ok(Json(ValidateLicenseResponse {
            valid: false,
            license: None,
            device: None,
            error: Some("License key not found".to_string()),
        }));
    };

    // 2. Sprawdź czy licencja jest valid
    if !license.is_valid() {
        log_validation(&pool, license.id, None, false, "License not valid or expired").await;

        return Ok(Json(ValidateLicenseResponse {
            valid: false,
            license: None,
            device: None,
            error: Some("License is not active or has expired".to_string()),
        }));
    }

    // 3. Sprawdź/zarejestruj urządzenie
    let device = register_or_update_device(&pool, &license, &payload.device).await?;

    // 4. Sprawdź limit urządzeń
    let device_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM devices WHERE license_id = $1"
    )
    .bind(license.id)
    .fetch_one(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if device_count > license.max_devices as i64 {
        log_validation(&pool, license.id, Some(device.id), false, "Device limit exceeded").await;

        return Ok(Json(ValidateLicenseResponse {
            valid: false,
            license: None,
            device: None,
            error: Some("Maximum device limit reached".to_string()),
        }));
    }

    // 5. Log successful validation
    log_validation(&pool, license.id, Some(device.id), true, "Success").await;

    // 6. Return success
    Ok(Json(ValidateLicenseResponse {
        valid: true,
        license: Some(LicenseInfo {
            id: license.id.to_string(),
            tier: license.tier,
            license_type: license.license_type,
            expires_at: license.expires_at,
            email: license.email,
        }),
        device: Some(DeviceInfo {
            device_id: device.device_id,
            device_name: device.device_name,
            os: device.os,
        }),
        error: None,
    }))
}

async fn register_or_update_device(
    pool: &PgPool,
    license: &License,
    device_info: &DeviceInfo,
) -> Result<Device, StatusCode> {

    // Spróbuj znaleźć istniejące urządzenie
    let existing = sqlx::query_as::<_, Device>(
        "SELECT * FROM devices WHERE license_id = $1 AND device_id = $2"
    )
    .bind(license.id)
    .bind(&device_info.device_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(mut device) = existing {
        // Update last_validated_at
        device.last_validated_at = Utc::now();

        sqlx::query(
            "UPDATE devices SET last_validated_at = $1 WHERE id = $2"
        )
        .bind(device.last_validated_at)
        .bind(device.id)
        .execute(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(device)
    } else {
        // Utwórz nowe urządzenie
        let device = sqlx::query_as::<_, Device>(
            r#"
            INSERT INTO devices (license_id, device_id, device_name, os)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#
        )
        .bind(license.id)
        .bind(&device_info.device_id)
        .bind(&device_info.device_name)
        .bind(&device_info.os)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create device: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        Ok(device)
    }
}

async fn log_validation(
    pool: &PgPool,
    license_id: Uuid,
    device_id: Option<Uuid>,
    success: bool,
    error_message: &str,
) {
    let _ = sqlx::query(
        r#"
        INSERT INTO validation_logs (license_id, device_id, success, error_message)
        VALUES ($1, $2, $3, $4)
        "#
    )
    .bind(license_id)
    .bind(device_id)
    .bind(success)
    .bind(error_message)
    .execute(pool)
    .await;
}

// Admin endpoint: Utwórz nową licencję
pub async fn create_license(
    State(pool): State<PgPool>,
    Json(payload): Json<CreateLicenseRequest>,
) -> Result<Json<License>, StatusCode> {

    let license_key = generate_license_key();

    let license = sqlx::query_as::<_, License>(
        r#"
        INSERT INTO licenses (
            license_key, email, tier, license_type,
            stripe_subscription_id, expires_at, max_devices, status
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING *
        "#
    )
    .bind(&license_key)
    .bind(&payload.email)
    .bind(&payload.tier)
    .bind(&payload.license_type)
    .bind(&payload.stripe_subscription_id)
    .bind(&payload.expires_at)
    .bind(payload.max_devices.unwrap_or(5))
    .bind(LicenseStatus::Active)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create license: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(license))
}

#[derive(Debug, Deserialize)]
pub struct CreateLicenseRequest {
    pub email: String,
    pub tier: LicenseTier,
    pub license_type: LicenseType,
    pub stripe_subscription_id: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_devices: Option<i32>,
}

fn generate_license_key() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    let parts: Vec<String> = (0..4)
        .map(|_| {
            (0..4)
                .map(|_| {
                    let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
                    chars.chars().nth(rng.gen_range(0..chars.len())).unwrap()
                })
                .collect::<String>()
        })
        .collect();

    parts.join("-")
}

// Webhook endpoint - Stripe subscription updated
pub async fn stripe_webhook(
    State(pool): State<PgPool>,
    body: String,
) -> Result<StatusCode, StatusCode> {
    // Parse Stripe event
    // Update license based on subscription status

    tracing::info!("Received Stripe webhook");

    // Implementacja zależy od Stripe SDK
    // Przykład: jeśli subscription.cancelled -> update status to Cancelled

    Ok(StatusCode::OK)
}
```

## Main Application (src/main.rs)

```rust
use axum::{
    routing::{get, post},
    Router,
};
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use std::net::SocketAddr;

mod models;
mod routes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "license_server=debug,tower_http=debug".into()),
        )
        .init();

    // Load environment variables
    dotenvy::dotenv().ok();

    // Database connection
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    tracing::info!("✅ Connected to database");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await?;

    tracing::info!("✅ Migrations completed");

    // Build routes
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/licenses/validate", post(routes::licenses::validate_license))
        .route("/api/v1/licenses", post(routes::licenses::create_license))
        .route("/api/v1/webhooks/stripe", post(routes::licenses::stripe_webhook))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(pool);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("🚀 Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}
```

## Environment Variables (.env)

```bash
DATABASE_URL=postgresql://user:password@localhost/license_db
STRIPE_SECRET_KEY=sk_test_...
STRIPE_WEBHOOK_SECRET=whsec_...
```

## Deployment

### Fly.io

```toml
# fly.toml
app = "aplikacja30-license-server"

[build]
  builder = "paketobuildpacks/builder:base"

[env]
  PORT = "8080"

[[services]]
  http_checks = []
  internal_port = 8080
  protocol = "tcp"

  [[services.ports]]
    port = 80

  [[services.ports]]
    port = 443
```

Deploy:
```bash
fly launch
fly secrets set DATABASE_URL="postgres://..."
fly secrets set STRIPE_SECRET_KEY="sk_live_..."
fly deploy
```

## Koszty

- **Fly.io**: $5-20/miesiąc (w zależności od obciążenia)
- **PostgreSQL**: $10-30/miesiąc (Fly.io Postgres lub Supabase)
- **Total**: ~$15-50/miesiąc + Stripe fees (2.9% + $0.30)

---

## Integracja z Stripe Subscriptions

```rust
// src/stripe_integration.rs
use async_stripe::{
    Client,
    Subscription,
    EventObject,
    EventType,
};

pub async fn handle_subscription_updated(
    pool: &PgPool,
    subscription: Subscription,
) -> Result<(), Box<dyn std::error::Error>> {

    let stripe_sub_id = subscription.id.to_string();
    let status = subscription.status;

    // Update license in database
    let new_status = match status {
        SubscriptionStatus::Active => LicenseStatus::Active,
        SubscriptionStatus::Canceled => LicenseStatus::Cancelled,
        SubscriptionStatus::PastDue => LicenseStatus::Active, // grace period
        _ => LicenseStatus::Cancelled,
    };

    sqlx::query(
        "UPDATE licenses SET status = $1, updated_at = NOW() WHERE stripe_subscription_id = $2"
    )
    .bind(new_status)
    .bind(&stripe_sub_id)
    .execute(pool)
    .await?;

    tracing::info!("Updated license for subscription {}", stripe_sub_id);
    Ok(())
}
```

---

## Podsumowanie

Ten backend daje Ci:
- ✅ Pełną kontrolę nad danymi
- ✅ Custom business logic
- ✅ Device management
- ✅ Validation logging (analytics)
- ✅ Stripe integration dla subskrypcji

**Ale wymaga:**
- ⚠️ Samodzielnego zarządzania infrastrukturą
- ⚠️ Implementacji VAT compliance
- ⚠️ Monitoringu i alertów

Dla większości indie hackerów rekomendacja to **zacząć od Lemon Squeezy**, a migrować do własnego backendu gdy osiągniesz $10k+ MRR.
