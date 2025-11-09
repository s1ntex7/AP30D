# Przewodnik Integracji Systemu Licencyjnego z Istniejącym Kodem

## Jak zintegrować licensing z obecnymi funkcjami aplikacji

### 1. Definicja Tier Features dla Aplikacji 3.0

```rust
// src/features/app_tiers.rs

use crate::licensing::models::LicenseTier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppFeature {
    // FREE TIER (Basic)
    BasicScreenshot,          // F10 - screenshot active monitor
    BasicTextExpansion,       // Do 10 skrótów
    BasicHotkeys,             // F9, F10, F11

    // PRO TIER
    AdvancedScreenshot,       // F11 - all monitors + annotation tools
    UnlimitedTextExpansion,   // Unlimited shortcuts
    VoiceToText,              // Home key - voice to text
    CloudSync,                // Sync shortcuts między urządzeniami
    ScreenshotOCR,            // Text extraction from screenshots
    ScreenshotAnnotation,     // Edit screenshots before saving
    CustomHotkeys,            // User-defined hotkeys
    AdvancedAutomation,       // Workflows, chains
}

impl AppFeature {
    pub fn required_tier(&self) -> LicenseTier {
        match self {
            AppFeature::BasicScreenshot
            | AppFeature::BasicTextExpansion
            | AppFeature::BasicHotkeys => LicenseTier::Free,

            _ => LicenseTier::Pro,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            AppFeature::BasicScreenshot => "Basic Screenshot",
            AppFeature::AdvancedScreenshot => "Advanced Screenshot (All Monitors)",
            AppFeature::BasicTextExpansion => "Text Expansion (10 shortcuts)",
            AppFeature::UnlimitedTextExpansion => "Unlimited Text Expansion",
            AppFeature::VoiceToText => "Voice to Text",
            AppFeature::CloudSync => "Cloud Sync",
            AppFeature::ScreenshotOCR => "Screenshot OCR",
            AppFeature::ScreenshotAnnotation => "Screenshot Annotation",
            AppFeature::CustomHotkeys => "Custom Hotkeys",
            AppFeature::AdvancedAutomation => "Advanced Automation",
            _ => "Unknown Feature",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            AppFeature::VoiceToText => "Convert speech to text using AI",
            AppFeature::AdvancedScreenshot => "Capture all monitors at once",
            AppFeature::UnlimitedTextExpansion => "Create unlimited text shortcuts",
            AppFeature::ScreenshotOCR => "Extract text from screenshots",
            AppFeature::CloudSync => "Sync your settings across devices",
            _ => "",
        }
    }
}
```

### 2. Modyfikacja Istniejącego Kodu

#### A. Text Expansion - Limit dla Free Tier

```rust
// src/simple_expansion.rs

// Dodaj do SimpleExpansionState:
use crate::features::AppFeature;
use crate::licensing::LicenseManager;

pub const FREE_TIER_SHORTCUT_LIMIT: usize = 10;

#[tauri::command]
pub async fn add_shortcut(
    trigger: String,
    expansion: String,
    state: tauri::State<'_, SimpleExpansionState>,
    app_state: tauri::State<'_, AppState>,  // Nowy parametr
) -> Result<(), String> {

    // Sprawdź czy user może dodać więcej skrótów
    let is_pro = app_state.license_manager.is_pro().await;

    if !is_pro {
        let current_count = state.shortcuts.read().unwrap().len();

        if current_count >= FREE_TIER_SHORTCUT_LIMIT {
            return Err(format!(
                "Free tier allows maximum {} shortcuts. Upgrade to Pro for unlimited shortcuts.",
                FREE_TIER_SHORTCUT_LIMIT
            ));
        }
    }

    // Existing logic...
    let mut map = state.shortcuts.write().unwrap();
    map.insert(trigger.clone(), expansion.clone());

    Ok(())
}

// Dodaj command do sprawdzania limitu
#[tauri::command]
pub async fn get_shortcut_limit(
    app_state: tauri::State<'_, AppState>,
) -> Result<ShortcutLimitInfo, String> {
    let is_pro = app_state.license_manager.is_pro().await;

    Ok(ShortcutLimitInfo {
        max_shortcuts: if is_pro { None } else { Some(FREE_TIER_SHORTCUT_LIMIT) },
        is_unlimited: is_pro,
    })
}

#[derive(serde::Serialize)]
pub struct ShortcutLimitInfo {
    pub max_shortcuts: Option<usize>,
    pub is_unlimited: bool,
}
```

#### B. Screenshot - Blokada All Monitors dla Free

```rust
// src/screenshot_new.rs

use crate::features::AppFeature;

#[tauri::command]
pub async fn launch_screenshot_overlay_all_monitors(
    app: tauri::AppHandle,
    app_state: tauri::State<'_, AppState>,
) -> Result<(), String> {

    // Sprawdź dostęp do Pro feature
    if !app_state.license_manager.is_pro().await {
        return Err(
            "Screenshot All Monitors is a Pro feature. Upgrade to unlock.".to_string()
        );
    }

    // Existing implementation...
    tracing::info!("🖼️ Launching All Monitors Screenshot (Pro)");

    // ... rest of code
    Ok(())
}
```

#### C. Voice to Text - Pro Only

```rust
// src/voice_to_text.rs

#[tauri::command]
pub async fn set_recording_state(
    recording: bool,
    app_state: tauri::State<'_, AppState>,
) -> Result<(), String> {

    // Check Pro access
    if !app_state.license_manager.is_pro().await {
        return Err(
            "Voice to Text is a Pro feature. Upgrade to unlock.".to_string()
        );
    }

    // Existing implementation...
    Ok(())
}
```

### 3. Frontend - React Integration

#### A. License Context Provider

```typescript
// src/contexts/LicenseContext.tsx
import React, { createContext, useContext, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export type LicenseTier = 'Free' | 'Pro';

interface LicenseContextType {
  tier: LicenseTier;
  isPro: boolean;
  isLoading: boolean;
  activateLicense: (key: string) => Promise<void>;
  checkFeatureAccess: (feature: string) => Promise<boolean>;
  refreshLicense: () => Promise<void>;
}

const LicenseContext = createContext<LicenseContextType | undefined>(undefined);

export function LicenseProvider({ children }: { children: React.ReactNode }) {
  const [tier, setTier] = useState<LicenseTier>('Free');
  const [isLoading, setIsLoading] = useState(true);

  const refreshLicense = async () => {
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
  };

  useEffect(() => {
    refreshLicense();
  }, []);

  const activateLicense = async (key: string) => {
    try {
      await invoke('activate_license', { licenseKey: key });
      await refreshLicense();
    } catch (error) {
      throw new Error(`Failed to activate license: ${error}`);
    }
  };

  const checkFeatureAccess = async (feature: string): Promise<boolean> => {
    try {
      return await invoke<boolean>('check_feature_access', {
        featureName: feature
      });
    } catch {
      return false;
    }
  };

  return (
    <LicenseContext.Provider
      value={{
        tier,
        isPro: tier === 'Pro',
        isLoading,
        activateLicense,
        checkFeatureAccess,
        refreshLicense,
      }}
    >
      {children}
    </LicenseContext.Provider>
  );
}

export function useLicense() {
  const context = useContext(LicenseContext);
  if (!context) {
    throw new Error('useLicense must be used within LicenseProvider');
  }
  return context;
}
```

#### B. Feature-Gated Button Component

```typescript
// src/components/ProButton.tsx
import React from 'react';
import { useLicense } from '../contexts/LicenseContext';

interface ProButtonProps {
  feature: string;
  onClick: () => void;
  children: React.ReactNode;
  className?: string;
}

export function ProButton({ feature, onClick, children, className }: ProButtonProps) {
  const { isPro } = useLicense();
  const [hasAccess, setHasAccess] = React.useState(false);

  React.useEffect(() => {
    async function checkAccess() {
      const license = useLicense();
      const access = await license.checkFeatureAccess(feature);
      setHasAccess(access);
    }
    checkAccess();
  }, [feature, isPro]);

  const handleClick = () => {
    if (hasAccess) {
      onClick();
    } else {
      // Show upgrade modal
      window.showUpgradeModal?.(feature);
    }
  };

  return (
    <button
      onClick={handleClick}
      className={`${className} ${!hasAccess ? 'pro-locked' : ''}`}
      title={!hasAccess ? 'Pro Feature - Click to Upgrade' : ''}
    >
      {children}
      {!hasAccess && <span className="pro-badge">PRO</span>}
    </button>
  );
}
```

#### C. Text Expansion UI z limitem

```typescript
// src/components/TextExpansion/ShortcutList.tsx
import { useLicense } from '../../contexts/LicenseContext';
import { invoke } from '@tauri-apps/api/core';

interface ShortcutLimitInfo {
  max_shortcuts: number | null;
  is_unlimited: boolean;
}

export function ShortcutList() {
  const { isPro } = useLicense();
  const [shortcuts, setShortcuts] = useState([]);
  const [limitInfo, setLimitInfo] = useState<ShortcutLimitInfo | null>(null);

  useEffect(() => {
    async function loadLimitInfo() {
      const info = await invoke<ShortcutLimitInfo>('get_shortcut_limit');
      setLimitInfo(info);
    }
    loadLimitInfo();
  }, [isPro]);

  const canAddMore = () => {
    if (!limitInfo) return true;
    if (limitInfo.is_unlimited) return true;
    return shortcuts.length < (limitInfo.max_shortcuts || 0);
  };

  return (
    <div className="shortcut-list">
      <div className="header">
        <h2>Text Expansion Shortcuts</h2>
        {!isPro && limitInfo && (
          <div className="limit-badge">
            {shortcuts.length} / {limitInfo.max_shortcuts} shortcuts
          </div>
        )}
      </div>

      {/* Shortcuts list */}
      {shortcuts.map(s => <ShortcutItem key={s.id} {...s} />)}

      {/* Add button */}
      {canAddMore() ? (
        <button onClick={addShortcut}>Add Shortcut</button>
      ) : (
        <div className="upgrade-prompt">
          <p>You've reached the Free tier limit ({limitInfo?.max_shortcuts} shortcuts)</p>
          <button onClick={showUpgradeModal}>Upgrade to Pro for Unlimited</button>
        </div>
      )}
    </div>
  );
}
```

#### D. Screenshot Toolbar

```typescript
// src/components/Screenshot/Toolbar.tsx
import { ProButton } from '../ProButton';
import { invoke } from '@tauri-apps/api/core';

export function ScreenshotToolbar() {
  const captureActiveMonitor = async () => {
    await invoke('launch_screenshot_overlay_active_monitor');
  };

  const captureAllMonitors = async () => {
    try {
      await invoke('launch_screenshot_overlay_all_monitors');
    } catch (error) {
      // Pokazuje modal upgrade jeśli nie ma dostępu
      console.error(error);
    }
  };

  return (
    <div className="screenshot-toolbar">
      {/* Free feature */}
      <button onClick={captureActiveMonitor}>
        📸 Capture Active Monitor
        <kbd>F10</kbd>
      </button>

      {/* Pro feature */}
      <ProButton
        feature="advanced_screenshot"
        onClick={captureAllMonitors}
      >
        📸 Capture All Monitors
        <kbd>F11</kbd>
      </ProButton>
    </div>
  );
}
```

### 4. Upgrade Modal

```typescript
// src/components/UpgradeModal.tsx
import { useLicense } from '../contexts/LicenseContext';
import { open } from '@tauri-apps/plugin-shell';

interface UpgradeModalProps {
  isOpen: boolean;
  onClose: () => void;
  feature?: string;
}

export function UpgradeModal({ isOpen, onClose, feature }: UpgradeModalProps) {
  if (!isOpen) return null;

  const handleUpgrade = async () => {
    await open('https://aplikacja30.com/pricing');
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={e => e.stopPropagation()}>
        <button className="close-btn" onClick={onClose}>×</button>

        <div className="icon">⭐</div>
        <h2>Upgrade to Pro</h2>

        {feature && (
          <p className="feature-highlight">
            <strong>{feature}</strong> is a Pro feature
          </p>
        )}

        <div className="benefits">
          <h3>Pro Benefits:</h3>
          <ul>
            <li>✅ Unlimited text expansion shortcuts</li>
            <li>✅ Voice to Text (AI-powered)</li>
            <li>✅ Advanced screenshot tools (All monitors, OCR, Annotation)</li>
            <li>✅ Cloud sync across devices</li>
            <li>✅ Custom hotkeys</li>
            <li>✅ Advanced automation workflows</li>
            <li>✅ Priority support</li>
          </ul>
        </div>

        <div className="pricing">
          <div className="plan">
            <h4>Monthly</h4>
            <div className="price">$9.99/mo</div>
            <button onClick={handleUpgrade}>Subscribe Monthly</button>
          </div>

          <div className="plan featured">
            <div className="badge">Best Value</div>
            <h4>Yearly</h4>
            <div className="price">$79.99/yr</div>
            <div className="savings">Save 33%</div>
            <button onClick={handleUpgrade}>Subscribe Yearly</button>
          </div>

          <div className="plan">
            <h4>Lifetime</h4>
            <div className="price">$199</div>
            <div className="onetime">One-time payment</div>
            <button onClick={handleUpgrade}>Buy Lifetime</button>
          </div>
        </div>
      </div>
    </div>
  );
}

// Global function to show modal from anywhere
declare global {
  interface Window {
    showUpgradeModal?: (feature?: string) => void;
  }
}
```

### 5. Settings Page - License Management

```typescript
// src/pages/Settings/LicenseTab.tsx
import { useState } from 'react';
import { useLicense } from '../../contexts/LicenseContext';

export function LicenseTab() {
  const { tier, isPro, activateLicense } = useLicense();
  const [licenseKey, setLicenseKey] = useState('');
  const [isActivating, setIsActivating] = useState(false);
  const [error, setError] = useState('');

  const handleActivate = async () => {
    setIsActivating(true);
    setError('');

    try {
      await activateLicense(licenseKey);
      setLicenseKey('');
      alert('License activated successfully! 🎉');
    } catch (err) {
      setError(err.message);
    } finally {
      setIsActivating(false);
    }
  };

  return (
    <div className="license-tab">
      <h2>License</h2>

      {/* Current Status */}
      <div className={`status-card ${isPro ? 'pro' : 'free'}`}>
        <div className="badge">{tier}</div>
        <h3>{isPro ? 'Pro License Active' : 'Free Version'}</h3>
        <p>
          {isPro
            ? 'You have access to all Pro features'
            : 'Upgrade to Pro to unlock advanced features'}
        </p>
      </div>

      {/* Activation Form */}
      {!isPro && (
        <div className="activation-form">
          <h3>Activate License</h3>
          <p>Enter your license key to unlock Pro features</p>

          <input
            type="text"
            placeholder="XXXX-XXXX-XXXX-XXXX"
            value={licenseKey}
            onChange={e => setLicenseKey(e.target.value.toUpperCase())}
            maxLength={19}
          />

          {error && <div className="error">{error}</div>}

          <button
            onClick={handleActivate}
            disabled={!licenseKey || isActivating}
          >
            {isActivating ? 'Activating...' : 'Activate License'}
          </button>

          <a href="https://aplikacja30.com/pricing" className="buy-link">
            Don't have a license? Buy now →
          </a>
        </div>
      )}

      {/* Features Comparison */}
      <div className="features-comparison">
        <h3>Feature Comparison</h3>
        <table>
          <thead>
            <tr>
              <th>Feature</th>
              <th>Free</th>
              <th>Pro</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>Basic Screenshot (Active Monitor)</td>
              <td>✅</td>
              <td>✅</td>
            </tr>
            <tr>
              <td>Advanced Screenshot (All Monitors)</td>
              <td>❌</td>
              <td>✅</td>
            </tr>
            <tr>
              <td>Text Expansion</td>
              <td>10 shortcuts</td>
              <td>Unlimited</td>
            </tr>
            <tr>
              <td>Voice to Text</td>
              <td>❌</td>
              <td>✅</td>
            </tr>
            <tr>
              <td>Screenshot OCR</td>
              <td>❌</td>
              <td>✅</td>
            </tr>
            <tr>
              <td>Cloud Sync</td>
              <td>❌</td>
              <td>✅</td>
            </tr>
            <tr>
              <td>Custom Hotkeys</td>
              <td>❌</td>
              <td>✅</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  );
}
```

### 6. Modyfikacja main.rs - Finalna Integracja

```rust
// src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod screenshot_new;
mod simple_expansion;
mod voice_to_text;
mod hotkeys;
mod keyboard;
mod licensing;  // NOWY
mod features;   // NOWY

use std::sync::{Arc, RwLock, Once};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use simple_expansion::SimpleExpansionState;
use licensing::LicenseManager;  // NOWY

#[derive(Clone)]
pub struct HotkeysState {
    vtt: Arc<RwLock<Shortcut>>,
}

#[derive(Clone)]
pub struct AppState {
    license_manager: LicenseManager,  // NOWY
}

fn default_vtt() -> Shortcut {
    Shortcut::new(Some(Modifiers::empty()), Code::F9)
}

static EXPANSION_LISTENER_ONCE: Once = Once::new();

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let expansion_state = SimpleExpansionState::default();
    let license_manager = LicenseManager::new();  // NOWY

    tauri::Builder::default()
        .manage(expansion_state.clone())
        .manage(HotkeysState { vtt: Arc::new(RwLock::new(default_vtt())) })
        .manage(AppState {                        // NOWY
            license_manager: license_manager.clone(),
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
            tracing::info!("🔧 setup() start");

            // Initialize license manager          // NOWY
            let lm = license_manager.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = lm.initialize().await {
                    tracing::error!("Failed to initialize license: {}", e);
                }
            });

            // Auto-load shortcuts
            let loaded = expansion_state.load_from_file(None).unwrap_or(0);
            tracing::info!("[TEXP] Auto-loaded {} shortcuts", loaded);

            let gs = app.global_shortcut();

            // ... existing hotkey registrations ...

            tracing::info!("✅ setup() done");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Text expansion
            simple_expansion::add_shortcut,
            simple_expansion::update_shortcut,
            simple_expansion::remove_shortcut,
            simple_expansion::list_shortcuts,
            simple_expansion::save_shortcuts,
            simple_expansion::load_shortcuts,
            simple_expansion::get_storage_path,
            simple_expansion::export_shortcuts,
            simple_expansion::import_shortcuts,
            simple_expansion::get_shortcut_limit,  // NOWY

            // Voice to text
            voice_to_text::paste_text,
            voice_to_text::set_recording_state,

            // Hotkeys
            hotkeys::get_vtt_hotkey,

            // Screenshots
            screenshot_new::launch_screenshot_overlay,
            screenshot_new::launch_screenshot_overlay_active_monitor,
            screenshot_new::launch_screenshot_overlay_all_monitors,

            // Licensing - NOWE
            licensing::activate_license,
            licensing::check_license_status,
            licensing::is_pro_user,
            licensing::deactivate_license,
            features::check_feature_access,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### 7. CSS dla Pro Badge

```css
/* src/styles/pro-badge.css */

.pro-badge {
  display: inline-block;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  font-size: 10px;
  font-weight: bold;
  padding: 2px 6px;
  border-radius: 4px;
  margin-left: 8px;
  text-transform: uppercase;
}

.pro-locked {
  position: relative;
  opacity: 0.6;
}

.pro-locked::after {
  content: '🔒';
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  font-size: 24px;
  pointer-events: none;
}

.upgrade-prompt {
  border: 2px dashed #667eea;
  border-radius: 8px;
  padding: 20px;
  text-align: center;
  background: rgba(102, 126, 234, 0.05);
}

.upgrade-prompt button {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border: none;
  padding: 12px 24px;
  border-radius: 6px;
  font-weight: bold;
  cursor: pointer;
  margin-top: 12px;
}

.status-card.pro {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
}

.status-card.free {
  background: #f5f5f5;
  border: 2px solid #e0e0e0;
}
```

---

## Podsumowanie Integracji

Po implementacji powyższego kodu:

1. **Free users otrzymują:**
   - Basic screenshot (F10 - active monitor)
   - 10 text expansion shortcuts
   - Podstawowe hotkeys

2. **Pro users otrzymują:**
   - Advanced screenshot (F11 - all monitors)
   - Unlimited text expansion
   - Voice to text (Home key)
   - Wszystkie przyszłe Pro features

3. **User experience:**
   - Przyciski Pro features są widoczne z badge "PRO"
   - Kliknięcie pokazuje upgrade modal
   - Po zakupie i aktywacji - natychmiastowe odblokowanie
   - Offline grace period - 7 dni bez internetu

4. **Bezpieczeństwo:**
   - Walidacja po stronie Rust (nie można obejść przez console.log)
   - Secure storage w OS keychain
   - Periodic revalidation (co 6h)

**Gotowe do wdrożenia!** 🚀
