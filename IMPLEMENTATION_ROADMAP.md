# Plan Wdrożenia Systemu Licencyjnego - Aplikacja 3.0

## 🎯 Quick Start - Pierwsze Kroki

### Tydzień 1: Setup & MVP

**Dzień 1-2: Lemon Squeezy Setup**
```bash
1. Załóż konto na https://lemonsqueezy.com
2. Skonfiguruj store:
   - Nazwa: "Aplikacja 3.0"
   - Utwórz 3 produkty:
     a) Pro Monthly - $9.99/mo (subscription)
     b) Pro Yearly - $79.99/yr (subscription, -33% rabat)
     c) Pro Lifetime - $199 (one-time payment)
3. Włącz License Keys dla wszystkich produktów
4. Skonfiguruj webhooks:
   - URL: https://twoj-backend.com/webhooks/lemonsqueezy
   - Events: subscription_updated, subscription_cancelled, order_created
```

**Dzień 3-4: Implementacja Rust Backend**
```bash
1. Dodaj dependencies do Cargo.toml:
   cargo add serde serde_json tokio reqwest jsonwebtoken keyring sha2 hex chrono sysinfo machine-uid

2. Stwórz strukturę modułów:
   src/
   ├── licensing/
   │   ├── mod.rs
   │   ├── manager.rs      # ⭐ Start tutaj
   │   ├── models.rs
   │   ├── storage.rs
   │   ├── validator.rs
   │   └── fingerprint.rs
   └── features/
       ├── mod.rs
       ├── gates.rs
       └── tiers.rs

3. Skopiuj kod z LICENSING_ARCHITECTURE.md do odpowiednich plików

4. Test lokalnie:
   cargo build
   cargo test
```

**Dzień 5: Integracja z UI (React)**
```bash
1. Dodaj License Context:
   src/contexts/LicenseContext.tsx

2. Stwórz komponenty:
   src/components/
   ├── FeatureGate.tsx
   ├── ProButton.tsx
   └── UpgradeModal.tsx

3. Dodaj Settings page:
   src/pages/Settings/LicenseTab.tsx

4. Wrap App w LicenseProvider:
   // src/App.tsx
   <LicenseProvider>
     <YourApp />
   </LicenseProvider>
```

**Dzień 6-7: Testowanie**
```bash
1. Test flow aktywacji:
   - Kup testową licencję na Lemon Squeezy (test mode)
   - Aktywuj w aplikacji
   - Sprawdź czy Pro features się odblokowały

2. Test offline mode:
   - Disconnect internet
   - Restartuj app
   - Sprawdź czy działa (cached license)

3. Test device limits:
   - Aktywuj na 2 urządzeniach
   - Spróbuj na 6. urządzeniu (powinno zablokować)
```

---

## 📅 Harmonogram Pełny (4 tygodnie)

### Sprint 1: Core Licensing (Tydzień 1)
- [x] Lemon Squeezy setup
- [x] Rust licensing module
- [x] Basic validation (online)
- [x] React UI dla aktywacji

**Deliverable:** Użytkownik może kupić i aktywować Pro license

### Sprint 2: Feature Gates (Tydzień 2)
- [ ] Feature flags system
- [ ] Integracja z istniejącymi funkcjami:
  - [ ] Text expansion limit (10 vs unlimited)
  - [ ] Screenshot all monitors (Pro only)
  - [ ] Voice to text (Pro only)
- [ ] Upgrade modals
- [ ] Settings page (license management)

**Deliverable:** Free vs Pro features działają poprawnie

### Sprint 3: Offline & Security (Tydzień 3)
- [ ] OS Keychain integration
- [ ] Offline cache (7 days grace period)
- [ ] Background revalidation task
- [ ] Device fingerprinting
- [ ] Rate limiting

**Deliverable:** App działa offline, bezpieczne przechowywanie

### Sprint 4: Polish & Launch (Tydzień 4)
- [ ] Error handling & user feedback
- [ ] Analytics (track feature usage)
- [ ] Admin dashboard (Lemon Squeezy ma wbudowany)
- [ ] Documentation dla użytkowników
- [ ] Marketing page (/pricing)
- [ ] Beta testing z 10 użytkownikami

**Deliverable:** Gotowe do production launch

---

## 🔧 Konfiguracja Techniczna

### Environment Variables

```bash
# .env (dla development)
LEMONSQUEEZY_API_KEY=your_api_key_here
LEMONSQUEEZY_STORE_ID=12345
LEMONSQUEEZY_WEBHOOK_SECRET=whsec_...

# Lub dla własnego backendu:
LICENSE_SERVER_URL=https://api.aplikacja30.com/v1
DATABASE_URL=postgresql://user:pass@localhost/license_db
```

### Build Configuration

```toml
# Cargo.toml - dodaj features
[features]
default = ["licensing"]
licensing = ["reqwest", "keyring", "machine-uid"]
```

### Tauri Config

```json
// tauri.conf.json
{
  "identifier": "com.aplikacja30.app",
  "bundle": {
    "macOS": {
      "entitlements": "entitlements.plist"  // dla keychain access
    }
  }
}
```

---

## 💰 Pricing Strategy - Rekomendacje

### Dla polskiego rynku:

**Monthly:** 39 PLN/miesiąc (~$9.99)
- Dla użytkowników, którzy chcą przetestować
- Najniższy próg wejścia

**Yearly:** 299 PLN/rok (~$79.99) ⭐ RECOMMENDED
- ~25 PLN/miesiąc (-36% vs monthly)
- "Większość użytkowników wybiera yearly"
- Stabilny recurring revenue

**Lifetime:** 799 PLN (~$199)
- Dla early adopters
- Instant cashflow
- Opcja "pay once, use forever" jest bardzo atrakcyjna

### Psychologia pricing:
```
❌ Źle:
Monthly: $10
Yearly: $100
Lifetime: $200

✅ Dobrze:
Monthly: $9.99
Yearly: $79.99 (SAVE 33% - badge!)
Lifetime: $199 (BEST VALUE - dla power users)
```

---

## 🚀 Go-to-Market Strategy

### Faza 1: Early Access (Tydzień 1-2)
```
1. Ogłoś na Twitter/X:
   "🚀 Aplikacja 3.0 wchodzi w fazę Early Access!

   Pierwsze 100 osób dostaje:
   - 50% zniżki na Lifetime (399 PLN zamiast 799 PLN)
   - Unlimited updates
   - Direct support

   Link: aplikacja30.com/early-access"

2. Email do obecnych beta testerów (jeśli są)

3. Post na Reddit:
   - r/productivity
   - r/macapps
   - r/windows
```

### Faza 2: Product Hunt Launch (Tydzień 3-4)
```
1. Przygotuj Product Hunt listing:
   - Tagline: "The Swiss Army Knife for Power Users"
   - Description: Anti-bloat desktop app for screenshots, text expansion, and automation
   - Demo video (2 min)
   - Screenshots

2. Launch day:
   - Upvote od znajomych (max 10 osób)
   - Odpowiadaj na każdy komentarz
   - Special offer: "Product Hunt exclusive - 30% off for 48h"

3. Cross-post:
   - Hacker News (Show HN)
   - Twitter
   - LinkedIn
```

### Faza 3: Content Marketing (Ongoing)
```
Blog posts / YouTube:
1. "How I built a $10k/mo desktop app in Rust"
2. "Licensing system architecture for desktop apps"
3. "Screenshot tool faster than Snagit"
4. "Text expansion that doesn't slow you down"

SEO keywords:
- "best screenshot tool for mac"
- "clipboard manager alternative"
- "text expander free"
- "productivity app for developers"
```

---

## 📊 Success Metrics

### Week 1 Targets:
- 100 downloads
- 10 paid conversions
- $500 MRR

### Month 1 Targets:
- 1,000 downloads
- 50 paid users (5% conversion)
- $2,000 MRR

### Month 3 Targets:
- 5,000 downloads
- 250 paid users
- $10,000 MRR

### Conversion funnel:
```
1000 visits → 100 downloads (10%) → 5 paid (5% of downloads) = $50 MRR
```

**Goal:** Osiągnąć 5% conversion rate (industry standard: 2-5%)

---

## 🔒 Bezpieczeństwo - Checklist

Przed launch:
- [ ] License keys stored in OS keychain (nie plain text)
- [ ] HTTPS only (nie HTTP)
- [ ] Rate limiting na API (max 1 req/5min)
- [ ] Device limit enforcement (max 5 devices)
- [ ] Webhook signature verification (Lemon Squeezy HMAC)
- [ ] Error messages nie ujawniają implementation details
- [ ] Logging (ale bez sensitive data - klucze, emaile)

Nice to have:
- [ ] Code obfuscation (cargo-obfuscate)
- [ ] Certificate pinning
- [ ] Hardware binding dla enterprise

---

## 🐛 Troubleshooting - Częste Problemy

### Problem: "License validation failed"
```
Możliwe przyczyny:
1. Brak internetu → Sprawdź cached license (grace period)
2. Invalid key → Weryfikacja czy user skopiował cały klucz
3. Rate limit → Czekaj 5 minut między próbami
4. Server down → Fallback do cached license

Debug:
tracing::error!("Validation failed: {}", error_message);
```

### Problem: "Device limit exceeded"
```
Rozwiązanie:
1. User portal na Lemon Squeezy → Manage devices
2. Lub dodaj w app: Settings → Devices → Deactivate old devices
3. Zwiększ limit do 5 urządzeń (zamiast 3)
```

### Problem: "Keychain access denied" (macOS)
```
Rozwiązanie:
1. Dodaj entitlements.plist:
   <key>com.apple.security.app-sandbox</key>
   <true/>
   <key>keychain-access-groups</key>
   <array>
     <string>$(AppIdentifierPrefix)com.aplikacja30.app</string>
   </array>

2. Podpisz aplikację (codesign)
```

---

## 📞 Support Strategy

### Self-service:
- FAQ na stronie (/help)
- Video tutorials na YouTube
- Discord community (dla Pro users)

### Direct support:
- Email: support@aplikacja30.com
- Response time: 24h (12h dla Pro users)
- Discord - live chat (tylko Pro)

**Template email (Polish):**
```
Temat: Problem z aktywacją licencji

Cześć [imię],

Dzięki za zakup Aplikacji 3.0!

Aby aktywować licencję:
1. Otwórz app → Settings → License
2. Wklej klucz: XXXX-XXXX-XXXX-XXXX
3. Kliknij "Activate"

Klucz znajdziesz w emailu od Lemon Squeezy (sprawdź spam).

Jeśli dalej masz problem, odpowiedz na tego maila z:
- Wersja systemu (Windows/macOS)
- Screenshot błędu

Pozdrawiam,
[Twoje imię]
```

---

## ✅ Next Steps - Co zrobić teraz?

1. **Decyzja:** Lemon Squeezy czy własny backend?
   - **Rekomendacja:** Start with Lemon Squeezy

2. **Implementacja:** (opcje)
   - **A)** Mogę zaimplementować pełny kod licensing w Twoim projekcie (2-3h)
   - **B)** Zaimplementujesz sam używając dokumentacji w LICENSING_ARCHITECTURE.md
   - **C)** Hybrydowo - ja zrobię backend, Ty frontend

3. **Testing:** Potrzebujesz test environment?
   - Mogę stworzyć mock Lemon Squeezy server do testów

4. **Launch:** Kiedy planujesz launch?
   - Jeśli <2 tygodnie: Przyspieszyć do MVP
   - Jeśli >1 miesiąc: Możemy dodać advanced features

**Daj znać co dalej!** 🚀
