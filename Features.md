<!-- markdownlint-disable MD024 -->

# Custom Features

---

List of features implemented by myself in this project.

---

## Feature 1: Duplicate Subscription Handling & Token Expiry

---

### Problem Statement

Currently, if a user tries to subscribe with an email that already exists in the `subscriptions` table, the `INSERT INTO subscriptions` fails due to the `UNIQUE` constraint on `email`, and the API returns a **500 Internal Server Error**. This is a poor user experience — a user who lost their first confirmation email, or simply forgot they already signed up, gets an opaque error instead of a helpful response.

Additionally, confirmation tokens currently **never expire**. A token generated weeks ago remains valid indefinitely, which is a security concern.

---

### Current State (What We Have)

| Component                       | Details                                                                                                       |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| **`subscriptions` table**       | `id` (uuid PK), `email` (text UNIQUE), `name` (text), `subscribed_at` (timestamptz), `status` (text NOT NULL) |
| **`subscription_tokens` table** | `subscription_token` (text PK), `subscriber_id` (uuid FK → subscriptions.id)                                  |
| **Status values**               | `pending_confirmation`, `confirmed`                                                                           |
| **Token generation**            | Random 25-char alphanumeric string, no expiry                                                                 |
| **Subscribe flow**              | Insert subscriber → generate token → store token → send email                                                 |
| **Confirm flow**                | Look up subscriber_id by token → update status to `confirmed`                                                 |

**Bug:** Duplicate email → SQL UNIQUE violation → 500 error.

---

### Desired Behavior

When a user submits the subscribe form with an email that already exists:

#### Case 1: Existing subscriber is `pending_confirmation`

1. **Do NOT** insert a new row into `subscriptions`.
2. Generate a **new** subscription token.
3. Store the new token in `subscription_tokens` (the old token(s) remain but will eventually expire — see token expiry below).
4. Send a **new** confirmation email with the new token link.
5. Return **200 OK** (same as a fresh subscription).

> **Rationale:** The user likely lost or missed their first email. Give them another chance.

#### Case 2: Existing subscriber is `confirmed`

1. **Do NOT** insert a new row or generate a new token.
2. Return **200 OK** (to avoid leaking subscription status to attackers).
3. Optionally, send an email to the subscriber informing them: _"You are already subscribed to our newsletter."_

> **Rationale:** Returning 200 regardless of state prevents email enumeration attacks. The informational email is a UX nicety.

#### Case 3: Email does not exist (current happy path)

No changes — behaves exactly as it does today.

---

### Token Expiry

#### Schema Change

Add a `created_at` column to `subscription_tokens`:

```sql
-- Migration: add_created_at_to_subscription_tokens
ALTER TABLE subscription_tokens ADD COLUMN created_at timestamptz NOT NULL DEFAULT now();
```

#### Expiry Duration

Tokens are valid for **24 hours** from `created_at`. This should be a **configurable** value in `configuration/base.yaml`:

```yaml
subscription:
  token_expiry_minutes: 1440 # 24 hours
```

#### Confirmation Endpoint Changes

When a user clicks a confirmation link:

1. Look up the token in `subscription_tokens`.
2. **Check expiry:** If `now() - created_at > token_expiry_minutes`, the token is expired.
   - Return **401 Unauthorized** (or a **410 Gone** with a user-friendly message).
   - Do **NOT** confirm the subscriber.
3. If the token is valid and not expired, confirm the subscriber as usual.

#### Expired Token Cleanup (Optional / Future)

A background job or periodic SQL query can delete expired tokens:

```sql
DELETE FROM subscription_tokens
WHERE created_at < now() - INTERVAL '48 hours';
```

This can be implemented later as a separate feature. Keeping expired tokens for 48h (double the expiry window) provides an audit trail.

---

### Implementation Plan

#### 1. Database Migration

```sql
-- Migration: add_created_at_to_subscription_tokens
ALTER TABLE subscription_tokens
    ADD COLUMN created_at timestamptz NOT NULL DEFAULT now();
```

#### 2. Configuration

Add to `Settings` / `configuration`:

```yaml
# base.yaml
subscription:
  token_expiry_minutes: 1440
```

Parse into a new config struct:

```rust
pub struct SubscriptionSettings {
    pub token_expiry_minutes: u64,
}
```

#### 3. Modify `subscribe()` Handler

Replace the current linear flow with a branching flow:

```sh
POST /subscriptions
  ├─ Parse & validate form data
  ├─ Query: SELECT id, status FROM subscriptions WHERE email = $1
  │
  ├─ [No row found] ──────────────────────► Current flow (insert subscriber + token + email)
  │
  ├─ [Row found, status = 'confirmed'] ──► Return 200 OK
  │                                         (optionally send "already subscribed" email)
  │
  └─ [Row found, status = 'pending'] ────► Generate new token
                                            Store token with subscriber_id
                                            Send new confirmation email
                                            Return 200 OK
```

Key functions to add/modify:

- `get_subscriber_by_email(pool, email) -> Option<(Uuid, String)>` — returns `(id, status)`
- Modify `subscribe()` to branch on the result
- `store_token()` — add `created_at` parameter (or use `Utc::now()` inside)

#### 4. Modify `confirm()` Handler

```sh
GET /subscriptions/confirm?subscription_token=...
  ├─ Query: SELECT subscriber_id, created_at FROM subscription_tokens WHERE subscription_token = $1
  │
  ├─ [No row] ───────────► 401 Unauthorized
  │
  ├─ [Row found, expired] ► 401 Unauthorized (token expired)
  │
  └─ [Row found, valid] ──► UPDATE subscriptions SET status = 'confirmed'
                             Return 200 OK
```

Key changes:

- `get_subscriber_id_from_token()` → also return `created_at`
- Add expiry check: `Utc::now() - created_at > Duration::minutes(token_expiry_minutes)`

#### 5. Email Templates

**New confirmation email (re-send):**
Same template as current, just with the new token link.

**"Already subscribed" email (optional):**

```sh
Subject: Newsletter Subscription
Body: "You are already subscribed to our newsletter. If you did not request this, please ignore this email."
```

---

### Test Cases

#### Subscription Tests

| #   | Test Name                                            | Description                                          | Expected                                         |
| --- | ---------------------------------------------------- | ---------------------------------------------------- | ------------------------------------------------ |
| 1   | `subscribe_twice_pending_sends_two_emails`           | Subscribe with same email twice without confirming   | Two confirmation emails sent, both return 200    |
| 2   | `subscribe_twice_pending_generates_different_tokens` | Subscribe twice, extract tokens from both emails     | Tokens are different                             |
| 3   | `subscribe_after_confirmed_returns_200`              | Subscribe, confirm, subscribe again                  | Second subscribe returns 200, no new token row   |
| 4   | `both_tokens_work_for_pending_subscriber`            | Subscribe twice (pending), confirm with either token | Both tokens can confirm (as long as not expired) |
| 5   | `second_subscribe_does_not_change_subscriber_id`     | Subscribe twice with same email                      | Same `subscriber_id` in DB, no duplicate rows    |

#### Token Expiry Tests

| #   | Test Name                                      | Description                                                                             | Expected                      |
| --- | ---------------------------------------------- | --------------------------------------------------------------------------------------- | ----------------------------- |
| 6   | `expired_token_returns_unauthorized`           | Create token, artificially set `created_at` to 25h ago, try to confirm                  | 401 Unauthorized              |
| 7   | `fresh_token_confirms_successfully`            | Normal subscribe + confirm within time window                                           | 200 OK, status = `confirmed`  |
| 8   | `expired_first_token_fresh_second_token_works` | Subscribe, wait (simulate expiry on 1st token), subscribe again, confirm with 2nd token | 2nd token works, 1st does not |

#### Edge Case Tests

| #   | Test Name                                            | Description                                                                             | Expected                                                      |
| --- | ---------------------------------------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| 9   | `subscribe_with_different_name_same_email_pending`   | Subscribe as "Alice" with email X, then subscribe as "Bob" with email X (still pending) | Name stays as original ("Alice"), new confirmation email sent |
| 10  | `subscribe_with_different_name_same_email_confirmed` | Confirmed subscriber re-subscribes with different name                                  | Returns 200, name unchanged                                   |

---

### Security Considerations

- **Email enumeration prevention:** Always return 200 OK for valid form data regardless of existing subscription state. An attacker cannot determine if an email is subscribed by observing HTTP responses.
- **Token entropy:** 25-char alphanumeric (62^25 ≈ 2.8 × 10^44 combinations) — sufficient for brute-force resistance.
- **Token expiry:** 24h window limits the attack surface for intercepted/leaked tokens.
- **Rate limiting (future):** Consider rate-limiting the `/subscriptions` endpoint to prevent abuse (spam confirmation emails). Out of scope for this feature.

---

### Files to Change

| File                                  | Change                                                                             |
| ------------------------------------- | ---------------------------------------------------------------------------------- |
| `migrations/`                         | New migration: `add_created_at_to_subscription_tokens.sql`                         |
| `configuration/base.yaml`             | Add `subscription.token_expiry_minutes`                                            |
| `src/configuration.rs`                | Add `SubscriptionSettings` struct, wire into `Settings`                            |
| `src/routes/subscriptions.rs`         | Add `get_subscriber_by_email()`, refactor `subscribe()` to handle duplicate emails |
| `src/routes/subscriptions_confirm.rs` | Modify `get_subscriber_id_from_token()` to check expiry                            |
| `src/startup.rs`                      | Pass `SubscriptionSettings` to app if needed                                       |
| `tests/api/subscription.rs`           | Add tests #1–5, #9–10                                                              |
| `tests/api/subscriptions_confirm.rs`  | Add tests #6–8                                                                     |

---

### Open Questions

1. **Should we update the `name` if a pending subscriber re-subscribes with a different name?**
   Proposed default: **No** — keep the original name. The name field from the second request is ignored. This avoids confusion and potential abuse.

2. **Should we invalidate old tokens when issuing a new one?**
   Proposed default: **No** — both old and new tokens remain valid until they expire naturally. Simpler implementation, and the old token still works if the user finds their first email. The expiry mechanism handles cleanup.

3. **Should the "already confirmed" case send an email?**
   Proposed default: **Yes** — send a brief "you're already subscribed" email. This helps confused users while maintaining the 200 response for security. Can be toggled via config.

---

### Feature 1 - Finished on 2026-03-07

---

## Feature 2: Idempotent Confirmation & Token Consumption

---

### Problem Statement

When a user clicks their confirmation link more than once, the system blindly re-runs `UPDATE subscriptions SET status = 'confirmed'` and returns a bare **200 OK**. While this doesn't cause errors (the UPDATE is idempotent at the SQL level), it has several drawbacks:

1. **Tokens are never invalidated** — A token remains valid in the database until it naturally expires (24h). This extends the window for token misuse if intercepted.
2. **No user feedback** — The user receives the same empty 200 response whether they just confirmed or were already confirmed. There is no indication of their actual state.
3. **Wasted DB writes** — Every click re-executes a redundant `UPDATE` against the database, even when the status is already `'confirmed'`.

---

### Current State (What We Have)

| Component                        | Details                                                                                        |
| -------------------------------- | ---------------------------------------------------------------------------------------------- |
| **Confirm flow**                 | Look up token → check expiry → `UPDATE status = 'confirmed'` → bare 200 OK                     |
| **Token lifecycle**              | Created on subscribe, never consumed or deleted, expires by time only                          |
| **Second click behavior**        | Identical to first click: silent 200 OK, redundant DB write                                    |
| **`subscription_tokens` schema** | `subscription_token` (text PK), `subscriber_id` (uuid FK), `created_at` (timestamptz NOT NULL) |
| **Response format**              | Bare `HttpResponse::Ok().finish()` — no body                                                   |

---

### Desired Behavior

When a user clicks a confirmation link:

#### Case 1: Token is valid, subscriber is `pending_confirmation` (first click — happy path)

1. Confirm the subscriber (`UPDATE status = 'confirmed'`).
2. Mark the token as consumed (`SET consumed_at = now()`).
3. Return **200 OK** with body: `{"status": "confirmed"}`.

#### Case 2: Token is consumed (already used — second+ click)

1. **Do NOT** re-execute the UPDATE on `subscriptions`.
2. Return **200 OK** with body: `{"status": "already_confirmed", "message": "You have already confirmed your subscription."}`.

> **Rationale:** The user successfully confirmed earlier. A friendly message is better than a cryptic error or silence. Returning 200 (not an error code) is correct because from the user's perspective nothing is wrong — their subscription _is_ confirmed.

#### Case 3: Token is valid and not consumed, but subscriber is already `confirmed` (confirmed via a _different_ token)

1. Mark this token as consumed (so it can't be replayed).
2. Return **200 OK** with body: `{"status": "already_confirmed", "message": "You have already confirmed your subscription."}`.

> **Rationale:** The user may have received multiple confirmation emails (Feature 1: duplicate subscription handling) and clicked links out of order. The subscription is confirmed — that's success.

#### Case 4: Token is expired (not consumed, past expiry window)

1. Return **401 Unauthorized** with body: `{"status": "expired", "message": "This confirmation link has expired. Please subscribe again to receive a new link."}`.

#### Case 5: Token does not exist

1. Return **401 Unauthorized** (bare, as today).

#### Precedence Rule: Consumed beats Expired

If a token was successfully consumed (used before expiry) and the user clicks it again _after_ the expiry window has passed, the response should be **200 "already_confirmed"**, **not** 401 expired. The token _did_ work — the user confirmed successfully. Showing "expired" would be confusing.

---

### Schema Change

Add a `consumed_at` column to `subscription_tokens`:

```sql
-- Migration: add_consumed_at_to_subscription_tokens
ALTER TABLE subscription_tokens ADD COLUMN consumed_at timestamptz;
```

The column is **nullable** — `NULL` means the token has not been used yet. A non-null value is the timestamp of when it was consumed.

---

### Implementation Plan

#### 1. Database Migration

```sql
ALTER TABLE subscription_tokens ADD COLUMN consumed_at timestamptz;
```

#### 2. Response Struct

Introduce a JSON response struct for the confirmation endpoint:

```rust
#[derive(serde::Serialize)]
struct ConfirmationResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}
```

| Scenario                     | Status Code | Body                                                                                                                    |
| ---------------------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------- |
| First confirmation (success) | 200         | `{"status": "confirmed"}`                                                                                               |
| Already confirmed (re-click) | 200         | `{"status": "already_confirmed", "message": "You have already confirmed your subscription."}`                           |
| Token expired                | 401         | `{"status": "expired", "message": "This confirmation link has expired. Please subscribe again to receive a new link."}` |
| Token not found              | 401         | (empty body)                                                                                                            |

#### 3. Modify `get_subscriber_id_from_token()`

Update the query to also return `consumed_at`:

```rust
pub async fn get_subscriber_id_from_token(
    pool: &PgPool,
    subscription_token: &str,
) -> Result<Option<(Uuid, DateTime<Utc>, Option<DateTime<Utc>>)>, sqlx::Error> {
    let result = sqlx::query!(
        r#"SELECT subscriber_id, created_at, consumed_at
           FROM subscription_tokens
           WHERE subscription_token = $1"#,
        subscription_token
    )
    .fetch_optional(pool)
    .await?;

    Ok(result.map(|r| (r.subscriber_id, r.created_at, r.consumed_at)))
}
```

#### 4. Add `mark_token_as_consumed()`

```rust
#[tracing::instrument(
    name = "Mark token as consumed",
    skip(pool, subscription_token)
)]
pub async fn mark_token_as_consumed(
    pool: &PgPool,
    subscription_token: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE subscription_tokens SET consumed_at = now() WHERE subscription_token = $1",
        subscription_token
    )
    .execute(pool)
    .await?;
    Ok(())
}
```

#### 5. Add `get_subscriber_status()`

Needed for Case 3 — checking if the subscriber was confirmed via a different token:

```rust
#[tracing::instrument(
    name = "Get subscriber status by id",
    skip(pool)
)]
pub async fn get_subscriber_status(
    pool: &PgPool,
    subscriber_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    let result = sqlx::query!(
        "SELECT status FROM subscriptions WHERE id = $1",
        subscriber_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(result.map(|r| r.status))
}
```

#### 6. Refactor `confirm()` Handler

```text
GET /subscriptions/confirm?subscription_token=...
  ├─ Query: SELECT subscriber_id, created_at, consumed_at
  │         FROM subscription_tokens WHERE subscription_token = $1
  │
  ├─ [No row] ──────────────────────► 401 Unauthorized (bare)
  │
  ├─ [Row found, consumed_at IS NOT NULL]
  │   └─► 200 OK { "status": "already_confirmed", "message": "..." }
  │
  ├─ [Row found, expired (now - created_at > expiry) AND consumed_at IS NULL]
  │   └─► 401 Unauthorized { "status": "expired", "message": "..." }
  │
  └─ [Row found, valid, not consumed]
       ├─ Query: SELECT status FROM subscriptions WHERE id = subscriber_id
       │
       ├─ [status = 'confirmed']
       │   └─► Mark token consumed → 200 OK { "status": "already_confirmed", "message": "..." }
       │
       └─ [status = 'pending_confirmation']
            └─► Confirm subscriber + mark token consumed → 200 OK { "status": "confirmed" }
```

**Key ordering:** The `consumed_at` check comes **before** the expiry check. This ensures consumed tokens always return "already_confirmed", even if they're past the expiry window (precedence rule above).

---

### Test Cases

#### Core Confirmation Tests

| #   | Test Name                                 | Description                                                      | Expected                                                                          |
| --- | ----------------------------------------- | ---------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| 1   | `confirm_twice_returns_already_confirmed` | Click confirmation link, then click it again                     | First: 200 `{"status":"confirmed"}`, Second: 200 `{"status":"already_confirmed"}` |
| 2   | `confirm_marks_token_as_consumed`         | Confirm, then check `consumed_at` in DB                          | `consumed_at` is NOT NULL and within last few seconds                             |
| 3   | `consumed_token_skips_subscriber_update`  | Confirm, click again, verify no second UPDATE on `subscriptions` | `subscriptions` row unchanged after second click                                  |

#### Multi-Token Tests

| #   | Test Name                                           | Description                                                         | Expected                                                                         |
| --- | --------------------------------------------------- | ------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| 4   | `confirm_with_second_token_after_first_consumed`    | Subscribe twice (pending), confirm with token A, then click token B | Token A: `confirmed`. Token B: `already_confirmed`. Both tokens marked consumed. |
| 5   | `unconsumed_token_for_already_confirmed_subscriber` | Subscribe twice, confirm with token A, click token B                | Token B returns `already_confirmed`, token B is now marked consumed              |

#### Edge Cases

| #   | Test Name                                              | Description                                                     | Expected                                                             |
| --- | ------------------------------------------------------ | --------------------------------------------------------------- | -------------------------------------------------------------------- |
| 6   | `consumed_token_past_expiry_returns_already_confirmed` | Confirm, artificially age `created_at` past expiry, click again | 200 `already_confirmed` (NOT 401 expired — it was used successfully) |
| 7   | `expired_unconsumed_token_returns_unauthorized`        | Don't confirm, artificially expire token, click                 | 401 `{"status": "expired", "message": "..."}`                        |
| 8   | `nonexistent_token_returns_unauthorized`               | Click with random/fake token                                    | 401 (bare)                                                           |

---

### Security Considerations

- **Reduced attack surface:** Consumed tokens cannot be replayed for any purpose. Even if intercepted after use, they are inert.
- **Audit trail:** `consumed_at` provides a timestamp of when confirmation actually occurred, useful for debugging and compliance.
- **No information leakage via status code:** Both "just confirmed" and "already confirmed" return **200 OK**. An external observer monitoring HTTP status codes cannot distinguish between a first confirmation and a replay.
- **JSON body is token-gated:** The difference in JSON body (`confirmed` vs `already_confirmed`) is only visible to someone who possesses the token — i.e., the legitimate subscriber (or someone who intercepted the email). This is acceptable because the information revealed ("you already confirmed") has minimal value to an attacker.
- **Consumed-before-expired precedence:** Prevents a confusing UX where a user who successfully confirmed gets an "expired" error when revisiting the link later. Also prevents an attacker from using expiry-based timing to probe token consumption state.

---

### Files to Change

| File                                  | Change                                                                                                                                                          |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `migrations/`                         | New migration: `add_consumed_at_to_subscription_tokens.sql`                                                                                                     |
| `src/routes/subscriptions_confirm.rs` | Refactor `confirm()`, modify `get_subscriber_id_from_token()`, add `mark_token_as_consumed()`, add `get_subscriber_status()`, add `ConfirmationResponse` struct |
| `tests/api/subscriptions_confirm.rs`  | Add tests #1–8                                                                                                                                                  |

---

### Open Questions

1. **Should consumed tokens be cleaned up on a different schedule than expired ones?**
   Proposed default: **No** — same cleanup schedule (48h after creation). Consumed tokens provide an audit trail and take negligible space. A future cleanup job can delete tokens where `created_at < now() - INTERVAL '48 hours'` regardless of consumption state.

2. **Should the response format change be applied to _all_ confirmation responses (including the existing bare 200/401)?**
   Proposed default: **Yes** — migrate from bare `HttpResponse::Ok().finish()` to JSON bodies across the entire `confirm()` endpoint for consistency. This is a minor breaking change if any client relies on an empty body, but since this is a link clicked in a browser, the risk is negligible.

3. **Should we also return the subscriber's email in the response body?**
   Proposed default: **No** — avoid leaking PII in the response. The user already knows their email. The token alone should not reveal it.

---

### Feature 2 - Finished on 2026-03-07

---

## Feature 3: `SubscriptionToken` Domain Type & Pre-Query Validation

---

### Problem Statement

The confirmation endpoint (`GET /subscriptions/confirm?subscription_token=...`) accepts `subscription_token` as a plain `String` inside the `Parameters` struct. This means **every incoming request**, no matter how obviously malformed the token is, reaches the handler body and issues a database query before being rejected.

This has several drawbacks:

1. **No type-level invariant** — the rest of the codebase has no guarantee that a token it receives is structurally valid. A 1-character token and a SQL injection attempt are equally representable as `String`.
2. **Unnecessary database load** — garbage inputs (wrong length, non-ASCII characters, special characters) cause a `SELECT` against `subscription_tokens` that will always return `None`. The database does work it never needed to do.
3. **Inconsistency with the rest of the domain** — `SubscriberName` and `SubscriberEmail` are validated newtype wrappers. Their `parse()` method enforces invariants before any handler logic runs. `subscription_token` has no equivalent protection.
4. **Reliance on sqlx as the sole defence** — sqlx parameterised queries protect against SQL injection, but defence in depth recommends rejecting obviously invalid inputs at the earliest possible layer, not the deepest one.

---

### Current State (What We Have)

| Component                                 | Details                                                                                      |
| ----------------------------------------- | -------------------------------------------------------------------------------------------- | --- | -------------------------------------------------------------------------------------------------------- |
| **`Parameters` struct**                   | `{ subscription_token: String }` — raw, unvalidated                                          |
| **Token generation** (`subscriptions.rs`) | `std::iter::repeat_with(                                                                     |     | rng.sample(Alphanumeric)).map(char::from).take(25).collect()` — exactly 25 ASCII alphanumeric characters |
| **Domain types**                          | `SubscriberName`, `SubscriberEmail` — newtype wrappers with `parse()` + custom `Deserialize` |
| **`domain/mod.rs`**                       | Exports `NewSubscriber`, `SubscriberEmail`, `SubscriberName`                                 |
| **First rejection point**                 | `get_token_row()` → `fetch_optional` → `Ok(None)` → `401 Unauthorized`                       |

The problem is that rejection only happens at step 3 of the handler, after a round-trip to the database.

---

### Desired Behavior

When a request arrives at `GET /subscriptions/confirm?subscription_token=<value>`:

#### Case 1: Token is malformed (wrong length, non-alphanumeric, unicode, whitespace, null bytes, etc.)

1. Rejected **during query-string deserialization**, before the handler body is entered.
2. actix-web returns **400 Bad Request** automatically.
3. **No database query is issued.**

#### Case 2: Token is well-formatted but does not exist in the database

1. Passes deserialization (token is structurally valid).
2. Handler queries the database, finds nothing, returns **401 Unauthorized**.
3. Behaviour unchanged from today.

#### Case 3: Token is well-formatted and exists in the database

1. Passes deserialization.
2. Handler proceeds through the existing consumed/expiry/confirmation logic unchanged.

---

### What "well-formatted" means

The token generator in `subscriptions.rs` produces exactly **25 ASCII alphanumeric characters** (`[A-Za-z0-9]`). A `SubscriptionToken` is valid if and only if:

- Its byte length is exactly **25**.
- Every character is ASCII alphanumeric (`char::is_ascii_alphanumeric()`).

These two checks reject:

- Tokens that are too short or too long (wrong length).
- Tokens containing spaces, hyphens, underscores, or punctuation.
- Tokens containing SQL meta-characters (`'`, `;`, `--`, `%`, etc.).
- Tokens containing Unicode characters, including homoglyphs (e.g., Cyrillic `а` instead of Latin `a`).
- Tokens containing null bytes or control characters.
- Empty strings.

Because `char::is_ascii_alphanumeric()` accepts only bytes `[0-9A-Za-z]`, all of the above classes are implicitly rejected by a single predicate.

---

### Implementation Plan

#### 1. Create `src/domain/subscription_token.rs`

Define a newtype struct with a private inner `String`:

```rust
pub struct SubscriptionToken(String);
```

Implement a `parse()` associated function that enforces the two invariants:

```rust
impl SubscriptionToken {
    const TOKEN_LENGTH: usize = 25;

    pub fn parse(raw: String) -> Result<SubscriptionToken, String> {
        // check 1: exact length
        // check 2: all ASCII alphanumeric
        // return Ok(Self(raw)) or Err(descriptive message)
    }
}
```

Implement `AsRef<str>` so the inner value can be passed to sqlx queries as `token.as_ref()` without breaking encapsulation:

```rust
impl AsRef<str> for SubscriptionToken {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
```

Implement a **custom `Deserialize`** (do not `#[derive(Deserialize)]`) that calls `parse()` and maps the error with `serde::de::Error::custom`. This is the same pattern used by `SubscriberName` and `SubscriberEmail`. When actix-web's `web::Query<Parameters>` fails to deserialize the query string, it returns 400 automatically — no handler code required:

```rust
impl<'de> Deserialize<'de> for SubscriptionToken {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        SubscriptionToken::parse(raw).map_err(serde::de::Error::custom)
    }
}
```

#### 2. Export from `src/domain/mod.rs`

Add a module declaration and re-export alongside the existing domain types:

```rust
mod subscription_token;
pub use subscription_token::SubscriptionToken;
```

#### 3. Use `SubscriptionToken` in `subscriptions_confirm.rs`

Change the `Parameters` struct from:

```rust
#[derive(Deserialize)]
pub struct Parameters {
    subscription_token: String,
}
```

to:

```rust
#[derive(serde::Deserialize)]
pub struct Parameters {
    subscription_token: SubscriptionToken,
}
```

Add the import:

```rust
use crate::domain::SubscriptionToken;
```

Everywhere `parameters.subscription_token` is passed to a function expecting `&str`, use `.as_ref()`:

```rust
// before
get_token_row(&pool, &parameters.subscription_token)
mark_token_as_consumed(&pool, &parameters.subscription_token)

// after
get_token_row(&pool, parameters.subscription_token.as_ref())
mark_token_as_consumed(&pool, parameters.subscription_token.as_ref())
```

No other logic in the handler needs to change.

---

### How the Validation Layer Fits Into the Request Lifecycle

```sh
GET /subscriptions/confirm?subscription_token=<value>
        │
        ▼
actix-web query string extractor
web::Query<Parameters>::from_query(...)
        │
        ├─ Calls serde::Deserialize for each field
        │
        ├─ SubscriptionToken::deserialize(...)
        │       └─ SubscriptionToken::parse(raw)
        │               ├─ length != 25         ──► Err → serde::de::Error::custom
        │               └─ non-ASCII-alnum char ──► Err → serde::de::Error::custom
        │
        ├─ [Deserialisation failed] ──────────────────► 400 Bad Request (actix-web automatic)
        │                                                (no handler code runs, no DB query)
        │
        └─ [Deserialisation succeeded]
                │
                ▼
            confirm() handler body
                │
                ├─ get_token_row() → None  ──► 401 Unauthorized
                └─ get_token_row() → Some  ──► consumed / expiry / confirmation logic
```

---

### Test Cases

All tests belong in `src/domain/subscription_token.rs` under `#[cfg(test)]`, following the same pattern as `SubscriberName` tests.

| #   | Test Name                                          | Input                                    | Expected |
| --- | -------------------------------------------------- | ---------------------------------------- | -------- |
| 1   | `a_valid_25_char_alphanumeric_token_is_accepted`   | `"abcdefghijklmnopqrstuvwxy"` (25 chars) | `Ok`     |
| 2   | `a_token_with_24_chars_is_rejected`                | 24 char string                           | `Err`    |
| 3   | `a_token_with_26_chars_is_rejected`                | 26 char string                           | `Err`    |
| 4   | `an_empty_token_is_rejected`                       | `""`                                     | `Err`    |
| 5   | `a_token_with_spaces_is_rejected`                  | 25 chars including a space               | `Err`    |
| 6   | `a_token_with_a_hyphen_is_rejected`                | 25 chars including `-`                   | `Err`    |
| 7   | `a_token_with_special_characters_is_rejected`      | SQL injection attempt string             | `Err`    |
| 8   | `a_token_with_unicode_chars_is_rejected`           | 25 chars including a Cyrillic homoglyph  | `Err`    |
| 9   | `a_token_with_a_null_byte_is_rejected`             | 25 chars including `\x00`                | `Err`    |
| 10  | `uppercase_and_lowercase_and_digits_are_all_valid` | Mix of `A-Z`, `a-z`, `0-9`, exactly 25   | `Ok`     |

Use the `claims` crate (`assert_ok!`, `assert_err!`) for assertions, consistent with existing domain tests.

---

### Security Considerations

- **Defence in depth:** sqlx parameterised queries already prevent SQL injection. This type adds a second, earlier rejection layer that is independent of the database driver. Neither layer should be removed in favour of the other.
- **Reduced attack surface for database probing:** Without this type, an attacker can send arbitrarily large or complex strings and cause a database round-trip for each one. With this type, only structurally valid tokens ever reach the database.
- **No information leakage:** A 400 on malformed tokens does not reveal whether a valid token exists. A structurally valid-but-nonexistent token still receives a 401, same as before. An attacker learns nothing new from the 400 — they only learn the expected format, which is already implied by a 25-character link in an email.
- **Consistent domain model:** Making token validation explicit in a domain type ensures future code paths that accept a token (e.g., an admin revocation endpoint) receive the same invariant guarantees without needing to re-implement the checks.

---

### Files to Change

| File                                  | Change                                                                                                    |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `src/domain/subscription_token.rs`    | **New file.** `SubscriptionToken` newtype, `parse()`, `AsRef<str>`, custom `Deserialize`, tests           |
| `src/domain/mod.rs`                   | Add `mod subscription_token;` and `pub use subscription_token::SubscriptionToken;`                        |
| `src/routes/subscriptions_confirm.rs` | Import `SubscriptionToken`, change `Parameters.subscription_token` field type, add `.as_ref()` call sites |

No migrations, no configuration changes, no changes to `subscriptions.rs`.

---

### Open Questions

1. **Should `SubscriptionToken` also derive `Serialize`?**
   Proposed default: **No** — tokens are never serialised into a response body in this codebase. Deriving `Serialize` would be dead code and could accidentally expose the raw token in a future response if a struct containing it were serialised carelessly.

2. **Should the constant `TOKEN_LENGTH = 25` be shared with `generate_subscription_token()` in `subscriptions.rs`?**
   Proposed default: **Yes, eventually.** Currently both the generator (25 characters) and the validator (`TOKEN_LENGTH = 25`) hard-code the same value independently. A future refactor could define a single constant in the domain type and import it in the generator, making a length change automatically update both sides. For now, keeping them co-located with their respective concerns is acceptable.

3. **Should the 400 response include an error body explaining the format requirement?**
   Proposed default: **No.** actix-web returns a bare 400 when `web::Query` extraction fails. Customising the error response would require an extractor wrapper or a custom error handler. The added complexity is not justified — the token comes from a link in a confirmation email, not from a user-facing form. A human who sees a 400 here has most likely tampered with the URL.

---

### Feature 3 - Finished on 2026-03-08

---
