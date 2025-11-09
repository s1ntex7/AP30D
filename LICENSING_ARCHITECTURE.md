# Architektura Systemu Licencyjnego dla Aplikacji 3.0

## Spis Treści
1. [Rekomendowana Architektura](#1-rekomendowana-architektura)
2. [Zewnętrzne Usługi vs Własne Rozwiązanie](#2-zewnętrzne-usługi-vs-własne-rozwiązanie)
3. [Implementacja w Rust/Tauri](#3-implementacja-w-rusttauri)
4. [Feature Flags i Dynamiczne Moduły](#4-feature-flags-i-dynamiczne-moduły)
5. [Bezpieczeństwo](#5-bezpieczeństwo)
6. [Przykładowa Implementacja](#6-przykładowa-implementacja)

---

## 1. Rekomendowana Architektura

### 1.1 Model Hybrydowy (Client-Server Validation)

**REKOMENDACJA: Hybrydowy model walidacji z offline fallback**

```
┌─────────────────────────────────────────────────────────┐
│                    APLIKACJA KLIENCKA                    │
│  ┌────────────────────────────────────────────────────┐ │
│  │  Rust Backend (Tauri)                              │ │
│  │  ┌──────────────────────────────────────────────┐ │ │
│  │  │  License Manager                             │ │ │
│  │  │  - Local cache (encrypted JWT)               │ │ │
│  │  │  - Periodic validation (co 6-24h)            │ │ │
│  │  │  - Offline grace period (7 dni)              │ │ │
│  │  │  - Feature flags resolver                    │ │ │
│  │  └──────────────────────────────────────────────┘ │ │
│  │                                                    │ │
│  │  ┌──────────────────────────────────────────────┐ │ │
│  │  │  Secure Storage                              │ │ │
│  │  │  - OS Keychain (macOS/Windows Credential)    │ │ │
│  │  │  - Encrypted local cache                     │ │ │
│  │  └──────────────────────────────────────────────┘ │ │
│  └────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────┐ │
│  │  React Frontend                                    │ │
│  │  - Feature gates (useLicense hook)                │ │
│  │  - Conditional rendering                          │ │
│  │  - Upgrade prompts                                │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
                         ▲ │
                         │ │ HTTPS (TLS 1.3)
                         │ ▼
┌─────────────────────────────────────────────────────────┐
│              SERWER LICENCYJNY / API                     │
│  ┌────────────────────────────────────────────────────┐ │
│  │  Validation Endpoint                               │ │
│  │  POST /api/v1/licenses/validate                    │ │
│  │  - Weryfikacja klucza/tokenu                       │ │
│  │  - Sprawdzenie statusu subskrypcji                 │ │
│  │  - Rate limiting (max 1 req/5min per device)      │ │
│  │  - Device fingerprinting                           │ │
│  └────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────┐ │
│  │  Database                                          │ │
│  │  - Licenses table                                  │ │
│  │  - Subscriptions table                             │ │
│  │  - Devices table (max 3-5 urządzeń per license)   │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
                         ▲ │
                         │ │ Webhooks
                         │ ▼
┌─────────────────────────────────────────────────────────┐
│         PAYMENT PROVIDER (Lemon Squeezy/Paddle)          │
│  - Subskrypcje                                           │
│  - Lifetime purchases                                    │
│  - Webhooks (subscription updated, canceled, etc.)       │
└─────────────────────────────────────────────────────────┘
```

### 1.2 Dlaczego Model Hybrydowy?

**✅ Zalety:**
- **Bezpieczeństwo**: Walidacja po stronie serwera zapobiega łatwemu obejściu licencji
- **Elastyczność**: Możesz natychmiastowo zdalnie dezaktywować licencje (np. przy chargebacku)
- **Tracking**: Wiesz, ile urządzeń używa danej licencji
- **Offline capability**: Aplikacja działa przez 7 dni bez internetu (cached token)
- **User experience**: Nie wymaga ciągłego połączenia z internetem

**❌ Wady:**
- Wymaga serwera backend (koszt, maintenance)
- Początkowa aktywacja wymaga internetu

**⚠️ Czego unikać:**
- **Tylko client-side**: Za łatwe do złamania przez prostą modyfikację kodu
- **Zawsze online**: Frustrujące dla użytkowników (np. w samolocie, w miejscach bez internetu)

---

## 2. Zewnętrzne Usługi vs Własne Rozwiązanie

### 2.1 Porównanie Platform

| Cecha | Lemon Squeezy | Paddle | Gumroad | Własny Backend |
|-------|--------------|--------|---------|----------------|
| **API do walidacji** | ✅ Świetne | ✅ Świetne | ⚠️ Podstawowe | ✅ Pełna kontrola |
| **Obsługa VAT/Taxes** | ✅ Automatyczna | ✅ Automatyczna | ❌ Musisz sam | ❌ Musisz sam |
| **Subskrypcje** | ✅ Pełne wsparcie | ✅ Pełne wsparcie | ✅ Tak | Musisz sam (Stripe) |
| **Webhooks** | ✅ Tak | ✅ Tak | ✅ Tak | ✅ Tak (custom) |
| **Koszt** | 5% + fees | 5% + fees | 10% | Serwer + Stripe (2.9%) |
| **Czas wdrożenia** | 1-2 dni | 2-3 dni | 1 dzień | 2-4 tygodnie |
| **License Keys** | ✅ Built-in | ✅ Built-in | ⚠️ Ręczne | Własna implementacja |
| **Device Limits** | ✅ API support | ✅ API support | ❌ Nie | ✅ Pełna kontrola |

### 2.2 REKOMENDACJA: Lemon Squeezy

**Dlaczego Lemon Squeezy?**
1. **Merchant of Record** - oni zajmują się VAT, podatkami, compliance
2. **Świetne API** - dedykowane endpointy do walidacji licencji
3. **License Keys** - wbudowany system generowania i walidacji kluczy
4. **Webhooks** - real-time powiadomienia o zmianach subskrypcji
5. **Polski rynek** - obsługują wszystkie wymagane podatki dla EU
6. **Sprawdzone** - używane przez setki indie hackerów (CalDotCom, Raycast, etc.)

**Przykładowe API Lemon Squeezy:**
```bash
# Walidacja license key
POST https://api.lemonsqueezy.com/v1/licenses/validate
{
  "license_key": "XXXX-XXXX-XXXX-XXXX",
  "instance_id": "device-fingerprint-uuid"
}

# Odpowiedź
{
  "valid": true,
  "license_key": {
    "id": 123,
    "status": "active",
    "activation_limit": 5,
    "activation_usage": 2,
    "expires_at": null  // null = lifetime
  },
  "instance": {
    "id": "uuid",
    "name": "MacBook Pro - John"
  },
  "meta": {
    "variant_name": "Pro License",
    "store_id": 456
  }
}
```

### 2.3 Alternatywnie: Własny Backend (dla pełnej kontroli)

**Stack rekomendowany:**
- **Backend**: Rust (Axum) lub Node.js (Fastify) + PostgreSQL
- **Payments**: Stripe for subscriptions
- **Hosting**: Fly.io, Railway.app, lub Cloudflare Workers

**Kiedy wybrać własny backend?**
- Chcesz pełną kontrolę nad danymi użytkowników
- Planujesz bardzo zaawansowane feature flags
- Masz już backend dla innych funkcji aplikacji
- Chcesz uniknąć 5% opłaty

---

## 3. Implementacja w Rust/Tauri

### 3.1 Struktura Modułów

```
src/
├── main.rs
├── licensing/
│   ├── mod.rs              # Public API
│   ├── manager.rs          # License Manager (główna logika)
│   ├── storage.rs          # Secure storage (keychain)
│   ├── validator.rs        # Server communication
│   ├── models.rs           # Data structures
│   └── fingerprint.rs      # Device fingerprinting
├── features/
│   ├── mod.rs
│   ├── gates.rs            # Feature gates/flags
│   └── tiers.rs            # Tier definitions (Free/Pro)
```

### 3.2 Dependencies (Cargo.toml)

```toml
[dependencies]
# Existing...
tauri = "2.x"

# Licensing
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
jsonwebtoken = "9"  # For JWT tokens

# Secure storage
keyring = "3"  # OS keychain integration

# Crypto
sha2 = "0.10"
hex = "0.4"

# Time
chrono = { version = "0.4", features = ["serde"] }

# Device fingerprinting
sysinfo = "0.32"
machine-uid = "0.5"
```

### 3.3 Modele Danych (models.rs)

```rust
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LicenseTier {
    Free,
    Pro,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LicenseType {
    Subscription { expires_at: DateTime<Utc> },
    Lifetime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    pub tier: LicenseTier,
    pub license_type: LicenseType,
    pub license_key: String,
    pub email: Option<String>,
    pub last_validated: DateTime<Utc>,
    pub cached_until: DateTime<Utc>,  // offline grace period
}

impl License {
    pub fn is_pro(&self) -> bool {
        matches!(self.tier, LicenseTier::Pro)
    }

    pub fn is_valid(&self) -> bool {
        match &self.license_type {
            LicenseType::Lifetime => true,
            LicenseType::Subscription { expires_at } => {
                Utc::now() < *expires_at
            }
        }
    }

    pub fn needs_validation(&self) -> bool {
        Utc::now() > self.cached_until
    }
}

// Response from license server
#[derive(Debug, Deserialize)]
pub struct ValidationResponse {
    pub valid: bool,
    pub tier: LicenseTier,
    pub license_type: LicenseType,
    pub email: Option<String>,
    pub error: Option<String>,
}

// Device fingerprint for limiting activations
#[derive(Debug, Serialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub device_name: String,
    pub os: String,
}
```

### 3.4 Secure Storage (storage.rs)

```rust
use keyring::Entry;
use anyhow::{Result, Context};
use super::models::License;

const SERVICE_NAME: &str = "com.aplikacja30";
const LICENSE_KEY: &str = "license_data";

pub struct LicenseStorage;

impl LicenseStorage {
    /// Zapisuje licencję w OS keychain (szyfrowane przez system)
    pub fn save(license: &License) -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, LICENSE_KEY)
            .context("Failed to create keyring entry")?;

        let json = serde_json::to_string(license)
            .context("Failed to serialize license")?;

        entry.set_password(&json)
            .context("Failed to save to keychain")?;

        tracing::info!("✅ License saved to secure storage");
        Ok(())
    }

    /// Odczytuje licencję z OS keychain
    pub fn load() -> Result<License> {
        let entry = Entry::new(SERVICE_NAME, LICENSE_KEY)
            .context("Failed to create keyring entry")?;

        let json = entry.get_password()
            .context("No license found in keychain")?;

        let license: License = serde_json::from_str(&json)
            .context("Failed to deserialize license")?;

        Ok(license)
    }

    /// Usuwa licencję (np. przy logout)
    pub fn delete() -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, LICENSE_KEY)?;
        entry.delete_credential()?;
        tracing::info!("🗑️ License removed from storage");
        Ok(())
    }
}
```

### 3.5 Device Fingerprinting (fingerprint.rs)

```rust
use anyhow::Result;
use sha2::{Sha256, Digest};
use sysinfo::System;

/// Generuje unikalny fingerprint urządzenia
/// Bazuje na machine_id + hostname (stabilne między uruchomieniami)
pub fn get_device_id() -> Result<String> {
    // Pobierz machine UUID (unikalny per instalacja OS)
    let machine_id = machine_uid::get()
        .unwrap_or_else(|| "fallback_id".to_string());

    // Dodaj hostname dla dodatkowej entropii
    let mut sys = System::new_all();
    sys.refresh_all();
    let hostname = System::host_name()
        .unwrap_or_else(|| "unknown".to_string());

    // Hash dla prywatności (nie wysyłamy raw machine ID)
    let mut hasher = Sha256::new();
    hasher.update(format!("{}-{}", machine_id, hostname));
    let result = hasher.finalize();

    Ok(format!("{:x}", result)[..32].to_string())
}

pub fn get_device_name() -> String {
    System::host_name().unwrap_or_else(|| "Unknown Device".to_string())
}

pub fn get_os() -> String {
    std::env::consts::OS.to_string()
}
```

### 3.6 License Validator - komunikacja z serwerem (validator.rs)

```rust
use anyhow::{Result, anyhow};
use reqwest::Client;
use super::models::{ValidationResponse, DeviceInfo};
use super::fingerprint;

const LICENSE_SERVER_URL: &str = "https://api.yourdomain.com/v1";
// Lub Lemon Squeezy:
// const LICENSE_SERVER_URL: &str = "https://api.lemonsqueezy.com/v1";

pub struct LicenseValidator {
    client: Client,
}

impl LicenseValidator {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent("Aplikacja30/1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// Walidacja z Lemon Squeezy
    pub async fn validate_lemon_squeezy(&self, license_key: &str) -> Result<ValidationResponse> {
        let device_id = fingerprint::get_device_id()?;

        let response = self.client
            .post("https://api.lemonsqueezy.com/v1/licenses/validate")
            .json(&serde_json::json!({
                "license_key": license_key,
                "instance_id": device_id
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("Validation failed: {}", response.status()));
        }

        let data: serde_json::Value = response.json().await?;

        // Parse Lemon Squeezy response
        let valid = data["valid"].as_bool().unwrap_or(false);
        let status = data["license_key"]["status"].as_str().unwrap_or("inactive");

        if !valid || status != "active" {
            return Ok(ValidationResponse {
                valid: false,
                tier: super::models::LicenseTier::Free,
                license_type: super::models::LicenseType::Lifetime,
                email: None,
                error: Some("License is not active".to_string()),
            });
        }

        // Sprawdź czy lifetime czy subscription
        let expires_at = data["license_key"]["expires_at"].as_str();
        let license_type = if expires_at.is_none() {
            super::models::LicenseType::Lifetime
        } else {
            let dt = chrono::DateTime::parse_from_rfc3339(expires_at.unwrap())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            super::models::LicenseType::Subscription { expires_at: dt }
        };

        Ok(ValidationResponse {
            valid: true,
            tier: super::models::LicenseTier::Pro,
            license_type,
            email: data["meta"]["customer_email"].as_str().map(|s| s.to_string()),
            error: None,
        })
    }

    /// Walidacja z własnym serwerem
    pub async fn validate_custom(&self, license_key: &str) -> Result<ValidationResponse> {
        let device_id = fingerprint::get_device_id()?;
        let device_info = DeviceInfo {
            device_id: device_id.clone(),
            device_name: fingerprint::get_device_name(),
            os: fingerprint::get_os(),
        };

        let response = self.client
            .post(format!("{}/licenses/validate", LICENSE_SERVER_URL))
            .json(&serde_json::json!({
                "license_key": license_key,
                "device": device_info
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("Validation failed: {}", response.status()));
        }

        let validation: ValidationResponse = response.json().await?;
        Ok(validation)
    }
}
```

### 3.7 License Manager - główna logika (manager.rs)

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Result, anyhow};
use chrono::{Utc, Duration};
use super::{
    models::{License, LicenseTier, ValidationResponse},
    storage::LicenseStorage,
    validator::LicenseValidator,
};

#[derive(Clone)]
pub struct LicenseManager {
    current_license: Arc<RwLock<Option<License>>>,
    validator: Arc<LicenseValidator>,
}

impl LicenseManager {
    pub fn new() -> Self {
        Self {
            current_license: Arc::new(RwLock::new(None)),
            validator: Arc::new(LicenseValidator::new()),
        }
    }

    /// Inicjalizacja przy starcie aplikacji
    pub async fn initialize(&self) -> Result<()> {
        tracing::info!("🔐 Initializing License Manager...");

        // Spróbuj załadować z secure storage
        match LicenseStorage::load() {
            Ok(license) => {
                tracing::info!("📦 Loaded cached license: {:?}", license.tier);

                // Sprawdź czy needs validation
                if license.needs_validation() {
                    tracing::info!("🔄 License needs revalidation...");
                    match self.revalidate_license(&license.license_key).await {
                        Ok(new_license) => {
                            *self.current_license.write().await = Some(new_license);
                        }
                        Err(e) => {
                            tracing::warn!("⚠️ Revalidation failed: {}. Using cached (grace period)", e);
                            // Sprawdź grace period
                            if Utc::now() < license.cached_until {
                                *self.current_license.write().await = Some(license);
                            } else {
                                tracing::error!("❌ Grace period expired. Reverting to Free tier.");
                                *self.current_license.write().await = None;
                            }
                        }
                    }
                } else {
                    // License still valid in cache
                    *self.current_license.write().await = Some(license);
                }
            }
            Err(_) => {
                tracing::info!("📭 No cached license found. Running in Free mode.");
                *self.current_license.write().await = None;
            }
        }

        Ok(())
    }

    /// Aktywacja nowego klucza licencyjnego
    pub async fn activate(&self, license_key: &str) -> Result<License> {
        tracing::info!("🔑 Activating license key...");

        // Walidacja przez serwer
        let validation = self.validator
            .validate_lemon_squeezy(license_key)
            .await?;

        if !validation.valid {
            return Err(anyhow!("Invalid license key: {:?}", validation.error));
        }

        // Utwórz license object
        let license = License {
            tier: validation.tier,
            license_type: validation.license_type,
            license_key: license_key.to_string(),
            email: validation.email,
            last_validated: Utc::now(),
            cached_until: Utc::now() + Duration::days(7), // 7 days grace
        };

        // Zapisz do secure storage
        LicenseStorage::save(&license)?;

        // Update in-memory state
        *self.current_license.write().await = Some(license.clone());

        tracing::info!("✅ License activated successfully: {:?}", license.tier);
        Ok(license)
    }

    /// Rewalidacja istniejącej licencji
    async fn revalidate_license(&self, license_key: &str) -> Result<License> {
        let validation = self.validator
            .validate_lemon_squeezy(license_key)
            .await?;

        if !validation.valid {
            return Err(anyhow!("License no longer valid"));
        }

        let license = License {
            tier: validation.tier,
            license_type: validation.license_type,
            license_key: license_key.to_string(),
            email: validation.email,
            last_validated: Utc::now(),
            cached_until: Utc::now() + Duration::days(7),
        };

        LicenseStorage::save(&license)?;
        Ok(license)
    }

    /// Sprawdź czy użytkownik ma dostęp do Pro tier
    pub async fn is_pro(&self) -> bool {
        let license = self.current_license.read().await;
        license.as_ref()
            .map(|l| l.is_pro() && l.is_valid())
            .unwrap_or(false)
    }

    /// Pobierz aktualny tier
    pub async fn get_tier(&self) -> LicenseTier {
        let license = self.current_license.read().await;
        license.as_ref()
            .filter(|l| l.is_valid())
            .map(|l| l.tier.clone())
            .unwrap_or(LicenseTier::Free)
    }

    /// Deaktywacja (logout)
    pub async fn deactivate(&self) -> Result<()> {
        LicenseStorage::delete()?;
        *self.current_license.write().await = None;
        tracing::info!("🔓 License deactivated");
        Ok(())
    }
}
```

### 3.8 Integracja z Tauri (main.rs)

```rust
// W main.rs dodaj:
mod licensing;
use licensing::LicenseManager;

#[derive(Clone)]
struct AppState {
    license_manager: LicenseManager,
}

#[tauri::command]
async fn activate_license(
    license_key: String,
    state: tauri::State<'_, AppState>
) -> Result<String, String> {
    state.license_manager
        .activate(&license_key)
        .await
        .map(|l| serde_json::to_string(&l).unwrap())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn check_license_status(
    state: tauri::State<'_, AppState>
) -> Result<String, String> {
    let tier = state.license_manager.get_tier().await;
    Ok(serde_json::to_string(&tier).unwrap())
}

#[tauri::command]
async fn is_pro_user(
    state: tauri::State<'_, AppState>
) -> Result<bool, String> {
    Ok(state.license_manager.is_pro().await)
}

fn main() {
    let license_manager = LicenseManager::new();

    tauri::Builder::default()
        .manage(AppState {
            license_manager: license_manager.clone(),
        })
        .setup(move |app| {
            // Initialize license at startup
            let lm = license_manager.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = lm.initialize().await {
                    tracing::error!("Failed to initialize license: {}", e);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            activate_license,
            check_license_status,
            is_pro_user,
            // ... existing handlers
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

---

## 4. Feature Flags i Dynamiczne Moduły

### 4.1 Definicja Feature Tiers (features/tiers.rs)

```rust
use super::super::licensing::models::LicenseTier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    // Free tier features
    BasicScreenshot,
    BasicTextExpansion,

    // Pro tier features
    AdvancedScreenshot,    // OCR, editing tools
    UnlimitedTextExpansion,
    VoiceToText,
    FileConverter,
    CloudSync,
    CustomPlugins,
    AIIntegration,
}

impl Feature {
    pub fn required_tier(&self) -> LicenseTier {
        match self {
            Feature::BasicScreenshot
            | Feature::BasicTextExpansion => LicenseTier::Free,

            Feature::AdvancedScreenshot
            | Feature::UnlimitedTextExpansion
            | Feature::VoiceToText
            | Feature::FileConverter
            | Feature::CloudSync
            | Feature::CustomPlugins
            | Feature::AIIntegration => LicenseTier::Pro,
        }
    }

    pub fn is_available_for(&self, tier: &LicenseTier) -> bool {
        match (self.required_tier(), tier) {
            (LicenseTier::Free, _) => true,
            (LicenseTier::Pro, LicenseTier::Pro) => true,
            _ => false,
        }
    }
}
```

### 4.2 Feature Gates (features/gates.rs)

```rust
use super::super::licensing::LicenseManager;
use super::tiers::Feature;
use anyhow::{Result, anyhow};

pub struct FeatureGate {
    license_manager: LicenseManager,
}

impl FeatureGate {
    pub fn new(license_manager: LicenseManager) -> Self {
        Self { license_manager }
    }

    /// Sprawdź czy feature jest dostępny
    pub async fn is_available(&self, feature: Feature) -> bool {
        let tier = self.license_manager.get_tier().await;
        feature.is_available_for(&tier)
    }

    /// Wymusz dostęp - rzuć błąd jeśli niedostępny
    pub async fn require(&self, feature: Feature) -> Result<()> {
        if self.is_available(feature).await {
            Ok(())
        } else {
            Err(anyhow!("Feature {:?} requires Pro license", feature))
        }
    }
}

// Tauri commands
#[tauri::command]
pub async fn check_feature_access(
    feature_name: String,
    state: tauri::State<'_, AppState>
) -> Result<bool, String> {
    // Parse feature name
    let feature = match feature_name.as_str() {
        "advanced_screenshot" => Feature::AdvancedScreenshot,
        "voice_to_text" => Feature::VoiceToText,
        "unlimited_text_expansion" => Feature::UnlimitedTextExpansion,
        _ => return Err("Unknown feature".to_string()),
    };

    Ok(state.feature_gate.is_available(feature).await)
}
```

### 4.3 React - useLicense Hook

```typescript
// src/hooks/useLicense.ts
import { invoke } from '@tauri-apps/api/core';
import { useState, useEffect } from 'react';

export type LicenseTier = 'Free' | 'Pro';

export interface LicenseInfo {
  tier: LicenseTier;
  isPro: boolean;
  isLoading: boolean;
}

export function useLicense(): LicenseInfo {
  const [tier, setTier] = useState<LicenseTier>('Free');
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    checkLicenseStatus();
  }, []);

  async function checkLicenseStatus() {
    try {
      const tierJson = await invoke<string>('check_license_status');
      const parsedTier = JSON.parse(tierJson);
      setTier(parsedTier);
    } catch (error) {
      console.error('Failed to check license:', error);
      setTier('Free');
    } finally {
      setIsLoading(false);
    }
  }

  return {
    tier,
    isPro: tier === 'Pro',
    isLoading,
  };
}
```

### 4.4 React - Feature Gate Component

```typescript
// src/components/FeatureGate.tsx
import React from 'react';
import { useLicense } from '../hooks/useLicense';

interface FeatureGateProps {
  feature: string;
  fallback?: React.ReactNode;
  children: React.ReactNode;
}

export function FeatureGate({ feature, fallback, children }: FeatureGateProps) {
  const { isPro, isLoading } = useLicense();
  const [hasAccess, setHasAccess] = React.useState(false);

  React.useEffect(() => {
    async function checkAccess() {
      try {
        const access = await invoke<boolean>('check_feature_access', {
          featureName: feature
        });
        setHasAccess(access);
      } catch (error) {
        console.error('Failed to check feature access:', error);
        setHasAccess(false);
      }
    }
    checkAccess();
  }, [feature, isPro]);

  if (isLoading) {
    return null; // lub loading spinner
  }

  if (!hasAccess) {
    return fallback ? <>{fallback}</> : null;
  }

  return <>{children}</>;
}

// Przykład użycia:
export function ScreenshotPage() {
  return (
    <div>
      <h1>Screenshot Tool</h1>

      {/* Podstawowy screenshot - dostępny dla wszystkich */}
      <BasicScreenshotButton />

      {/* Zaawansowane funkcje - tylko Pro */}
      <FeatureGate
        feature="advanced_screenshot"
        fallback={<UpgradePrompt feature="Advanced Screenshot Tools" />}
      >
        <OCRButton />
        <ImageEditorButton />
      </FeatureGate>
    </div>
  );
}
```

### 4.5 Upgrade Prompt Component

```typescript
// src/components/UpgradePrompt.tsx
import { open } from '@tauri-apps/plugin-shell';

interface UpgradePromptProps {
  feature: string;
}

export function UpgradePrompt({ feature }: UpgradePromptProps) {
  const handleUpgrade = async () => {
    await open('https://aplikacja30.com/pricing');
  };

  return (
    <div className="upgrade-prompt">
      <div className="icon">🔒</div>
      <h3>{feature} is a Pro feature</h3>
      <p>Upgrade to Pro to unlock this feature and many more.</p>
      <button onClick={handleUpgrade}>
        Upgrade to Pro →
      </button>
    </div>
  );
}
```

---

## 5. Bezpieczeństwo

### 5.1 Checklist Zabezpieczeń

**✅ Bezpieczne przechowywanie:**
- Używamy OS Keychain (macOS Keychain, Windows Credential Manager)
- NIGDY nie przechowuj license key w plain text w plikach
- Używaj encrypted cache z TTL

**✅ Komunikacja:**
- Tylko HTTPS (TLS 1.3)
- Certificate pinning (opcjonalnie dla extra security)
- Rate limiting na serwerze (max 1 request/5min per device)

**✅ Device Fingerprinting:**
- Hashuj machine ID przed wysłaniem (prywatność)
- Limituj liczbę urządzeń (np. 3-5 per license)
- Pozwól użytkownikowi zarządzać urządzeniami przez web portal

**✅ Offline Grace Period:**
- 7 dni offline capability
- Po tym czasie wymuszaj revalidation
- Nie blokuj natychmiast - user experience first

**✅ Obfuskacja (opcjonalnie):**
- Dla extra protection możesz użyć code obfuscation
- Narzędzie: `cargo-obfuscate` lub commercial solutions
- UWAGA: To nie zastąpi dobrej architektury server-side

### 5.2 Czego NIE robić

❌ **Nie hardcode'uj secrets w kodzie**
```rust
// ŹLE!
const API_KEY: &str = "sk_live_abc123...";
```

❌ **Nie ufaj tylko client-side validation**
```rust
// ŹLE!
if license_key.starts_with("PRO-") {
    enable_pro_features();
}
```

❌ **Nie blokuj całej aplikacji przy braku internetu**
- Używaj offline cache z grace period

---

## 6. Przykładowa Implementacja - Pełny Flow

### 6.1 Flow Aktywacji Licencji

```
1. Użytkownik kupuje licencję na Lemon Squeezy
2. Otrzymuje email z license key: XXXX-XXXX-XXXX-XXXX
3. W aplikacji: Settings → License → "Activate License"
4. Wpisuje klucz
5. Aplikacja:
   - Generuje device fingerprint
   - Wysyła POST do Lemon Squeezy API
   - Otrzymuje potwierdzenie + metadata
   - Zapisuje do keychain
   - Odświeża UI (pokazuje Pro features)
```

### 6.2 Flow Walidacji Okresowej

```
1. Aplikacja startuje
2. LicenseManager.initialize():
   - Ładuje cached license z keychain
   - Sprawdza cached_until
   - Jeśli expired: async revalidation
   - Jeśli revalidation fail: sprawdź grace period
3. Co 6h (background task):
   - Revalidate license
   - Update cache
4. Jeśli subscription canceled:
   - Lemon Squeezy webhook → Twój backend
   - Backend oznacza license jako inactive
   - Przy następnej walidacji: downgrade do Free
```

### 6.3 Przykładowy JSON Response (własny backend)

```json
{
  "valid": true,
  "license": {
    "id": "lic_abc123",
    "tier": "Pro",
    "type": "subscription",
    "expires_at": "2025-12-09T00:00:00Z",
    "email": "user@example.com"
  },
  "device": {
    "id": "dev_xyz789",
    "name": "MacBook Pro - John",
    "activated_at": "2025-11-01T10:30:00Z"
  },
  "limits": {
    "max_devices": 5,
    "current_devices": 2
  },
  "features": {
    "advanced_screenshot": true,
    "voice_to_text": true,
    "cloud_sync": true,
    "ai_integration": true
  }
}
```

---

## 7. Podsumowanie - Rekomendowana Strategia

### Faza 1 - MVP (1-2 tygodnie)
1. **Integracja z Lemon Squeezy**
   - Skonfiguruj store
   - Utwórz produkty (Subscription + Lifetime)
   - Dodaj webhooks
2. **Podstawowa walidacja**
   - Implementuj `LicenseManager` + `LicenseValidator`
   - Tylko Lemon Squeezy API (bez własnego backendu)
3. **Feature flags**
   - Zdefiniuj Free vs Pro features
   - Zaimplementuj `FeatureGate`
   - Warunkowe renderowanie w React

### Faza 2 - Polish (2-3 tygodnie)
1. **Secure storage**
   - OS Keychain integration
   - Encrypted cache
2. **Offline capability**
   - Grace period (7 dni)
   - Background revalidation task
3. **Device management**
   - Fingerprinting
   - Web portal do zarządzania urządzeniami

### Faza 3 - Advanced (opcjonalnie)
1. **Własny backend** (jeśli potrzebny)
   - Axum/Fastify + PostgreSQL
   - Custom license logic
   - Advanced analytics
2. **Code obfuscation**
3. **Hardware binding** (dla enterprise)

---

## 8. Przykładowe Koszty

### Opcja A: Lemon Squeezy
- **Setup**: $0
- **Prowizja**: 5% + payment processing fees
- **Hosting**: $0 (wszystko w chmurze Lemon Squeezy)
- **Czas wdrożenia**: 1-2 tygodnie
- **Total dla $10k MRR**: ~$500/miesiąc w fees

### Opcja B: Własny Backend
- **Setup**: Czas developera (2-4 tygodnie)
- **Stripe**: 2.9% + $0.30 per transaction
- **Hosting**: $20-50/miesiąc (Fly.io/Railway)
- **VAT compliance**: Trzeba samemu załatwić (skomplikowane)
- **Total dla $10k MRR**: ~$310/miesiąc + developer time

**REKOMENDACJA dla startu: Lemon Squeezy**
- Szybsze wdrożenie
- Mniej compliance headache
- Możesz później migrować do własnego backendu

---

## Pytania? Następne Kroki?

Gotowy kod skeleton jest w tym dokumencie. Jeśli chcesz, mogę:
1. Stworzyć pełną strukturę modułów w Twoim projekcie
2. Zintegrować z istniejącym kodem (screenshot_new.rs, etc.)
3. Zaimplementować konkretne feature gates dla Twoich modułów
4. Przygotować przykładowy backend w Rust (Axum)

Daj znać!
