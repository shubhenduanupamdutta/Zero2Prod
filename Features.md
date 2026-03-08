<!-- markdownlint-disable MD024 -->
<!-- markdownlint-disable MD010 -->

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

## Feature 4: Tera-Powered HTML Email Templates

---

### Problem Statement

The two emails sent by the application — the subscription confirmation email and the "already subscribed" reminder email — are assembled by concatenating raw HTML strings directly inside `send_confirmation_email()` and `send_reminder_email()` in `src/routes/subscriptions.rs`:

```rust
// confirmation email
let html_body = format!(
    "Welcome to our newsletter!<br />Click <a href=\"{}\">here</a> to confirm your subscription.",
    confirmation_link
);

// reminder email
let html_body = "Welcome back to our newsletter!<br />You are already subscribed.";
```

This approach has several drawbacks:

1. **Ugly output** — inline HTML fragments with no structure, no styling, no branding. Every major mail client renders these as plain unstyled text on a white background.
2. **No raw-link fallback** — many corporate and mobile mail clients strip `<a>` tags or block external links behind a click-tracker. A subscriber who cannot click the button has no way to retrieve their confirmation URL.
3. **Mixing concerns** — template logic is tangled inside route handler functions. Changing the email copy requires touching the business-logic layer.
4. **No type-level rendering contract** — the HTML is an unverifiable string. Missing or misspelled variable names are silent bugs discovered only when a subscriber complains.
5. **Not extensible** — adding a third email type (e.g., "subscription cancelled") means adding more inline format strings rather than adding a new template file.

---

### Current State (What We Have)

| Component                 | Details                                                                   |
| ------------------------- | ------------------------------------------------------------------------- |
| `send_confirmation_email` | Builds HTML via `format!()`, sends via `EmailClient::send_email()`        |
| `send_reminder_email`     | Hardcoded HTML literal string, sends via `EmailClient::send_email()`      |
| `EmailClient::send_email` | Accepts `html_content: &str`; no knowledge of how the string was produced |
| Template engine           | None. No crate used.                                                      |
| Template files            | None. No `templates/` directory exists.                                   |

---

### Desired Behavior

All outbound emails are rendered from Tera templates stored in `templates/emails/`. Template rendering is handled by a dedicated `EmailTemplateEngine` struct that wraps a `tera::Tera` instance and exposes one method per email type. Route handlers call these methods to obtain a finalized HTML string, which they pass to `EmailClient::send_email()` unchanged.

#### Confirmation Email (`templates/emails/confirmation.html`)

Rendered when a new subscriber (or a pending re-subscriber) needs to confirm their address.

Template variables:

| Variable            | Type     | Description                                                             |
| ------------------- | -------- | ----------------------------------------------------------------------- |
| `name`              | `String` | The subscriber's display name                                           |
| `confirmation_link` | `String` | Full confirmation URL, e.g. `https://myapp.com/subscriptions/confirm?…` |

Required content:

- A styled header (newsletter name / branding).
- A personalized greeting: _"Hi {{ name }},"_.
- A short message: _"Thank you for signing up! Please confirm your subscription."_
- A prominent CTA button (`<a href="{{ confirmation_link }}">Confirm my subscription</a>`) styled to look like a button via inline CSS (required by most email clients).
- A **raw-link fallback** section clearly labelled (e.g., _"If the button doesn't work, copy and paste this link into your browser:"_) followed by the URL rendered as visible text and also as a clickable `<a>` tag.
- A footer: _"If you didn't sign up for this newsletter, you can safely ignore this email."_

#### Already-Subscribed Email (`templates/emails/already_subscribed.html`)

Rendered when a subscriber whose status is `confirmed` attempts to subscribe again.

Template variables:

| Variable | Type     | Description                   |
| -------- | -------- | ----------------------------- |
| `name`   | `String` | The subscriber's display name |

Required content:

- The same styled header.
- A personalized greeting: _"Hi {{ name }},"_.
- Friendly body: _"It looks like you're already subscribed to our newsletter — no action needed. You'll continue receiving our updates as usual."_
- A reassurance footer: _"If you didn't expect this email, someone may have entered your address by mistake. You can safely ignore it."_

---

### Template HTML Structure (Reference)

Both templates follow the same outer shell (inlined CSS for email-client compatibility):

```html
<!DOCTYPE html>
<html lang="en">
	<head>
		<meta charset="UTF-8" />
		<meta name="viewport" content="width=device-width, initial-scale=1.0" />
		<title><!-- email-specific title --></title>
	</head>
	<body style="margin:0;padding:0;background:#f4f4f4;font-family:Arial,sans-serif;">
		<table width="100%" cellpadding="0" cellspacing="0" style="background:#f4f4f4;padding:40px 0;">
			<tr>
				<td align="center">
					<table
						width="600"
						cellpadding="0"
						cellspacing="0"
						style="background:#ffffff;border-radius:8px;overflow:hidden;"
					>
						<!-- Header -->
						<tr>
							<td style="background:#2c3e50;padding:32px 40px;text-align:center;">
								<h1 style="color:#ffffff;margin:0;font-size:24px;">Zero2Prod Newsletter</h1>
							</td>
						</tr>

						<!-- Body -->
						<tr>
							<td style="padding:40px;">
								<!-- email-specific content injected here by Tera -->
							</td>
						</tr>

						<!-- Footer -->
						<tr>
							<td
								style="background:#ecf0f1;padding:24px 40px;text-align:center;
                        font-size:12px;color:#7f8c8d;"
							>
								<!-- email-specific footer -->
							</td>
						</tr>
					</table>
				</td>
			</tr>
		</table>
	</body>
</html>
```

The CTA button (confirmation email only) is a plain `<a>` styled with inline CSS — no JavaScript, no `:hover` pseudo-classes — because email clients do not execute scripts and many ignore `<style>` blocks:

```html
<a
	href="{{ confirmation_link }}"
	style="display:inline-block;padding:14px 28px;background:#2980b9;
          color:#ffffff;text-decoration:none;border-radius:4px;
          font-size:16px;font-weight:bold;"
>
	Confirm my subscription
</a>
```

The raw-link fallback sits below the button:

```html
<p style="margin-top:24px;font-size:13px;color:#555555;">
	If the button above doesn't work, copy and paste this link into your browser:
</p>
<p style="word-break:break-all;">
	<a href="{{ confirmation_link }}" style="color:#2980b9;">{{ confirmation_link }}</a>
</p>
```

---

### Implementation Plan

#### 1. Add `tera` to `Cargo.toml`

```toml
tera = "1"
```

No feature flags required for basic usage.

#### 2. Create `templates/emails/`

```sh
templates/
  emails/
    confirmation.html
    already_subscribed.html
```

Both files are full HTML documents following the structure above. They are stored in the repository root and shipped alongside the binary (or embedded at compile time — see Open Questions).

#### 3. Create `src/email_client/templates.rs`

Define `EmailTemplateEngine` as a newtype around `tera::Tera`:

```rust
pub struct EmailTemplateEngine(tera::Tera);
```

Provide a constructor that discovers templates from a directory glob:

```rust
impl EmailTemplateEngine {
    /// Load all `*.html` templates from `<templates_dir>/emails/`.
    /// Returns an error if the directory is unreachable or any template fails to parse.
    pub fn new(templates_dir: &str) -> Result<Self, tera::Error> {
        let glob = format!("{}/emails/**/*.html", templates_dir);
        let tera = tera::Tera::new(&glob)?;
        Ok(Self(tera))
    }
}
```

Provide one rendering method per email type. Each method accepts the exact variables its template requires, builds a `tera::Context`, and calls `self.0.render()`:

```rust
impl EmailTemplateEngine {
    pub fn render_confirmation_email(
        &self,
        name: &str,
        confirmation_link: &str,
    ) -> Result<String, tera::Error> {
        let mut ctx = tera::Context::new();
        ctx.insert("name", name);
        ctx.insert("confirmation_link", confirmation_link);
        self.0.render("confirmation.html", &ctx)
    }

    pub fn render_already_subscribed_email(
        &self,
        name: &str,
    ) -> Result<String, tera::Error> {
        let mut ctx = tera::Context::new();
        ctx.insert("name", name);
        self.0.render("already_subscribed.html", &ctx)
    }
}
```

Export the module from `src/email_client/mod.rs`:

```rust
pub mod templates;
pub use templates::EmailTemplateEngine;
```

#### 4. Add `templates_dir` to `ApplicationSettings`

In `src/configuration.rs`, add a field to `ApplicationSettings`:

```rust
pub struct ApplicationSettings {
    pub port: u16,
    pub host: String,
    pub base_url: String,
    pub templates_dir: String,   // NEW
}
```

Add the default value to `configuration/base.yaml`:

```yaml
application:
  templates_dir: "templates"
```

Override with an absolute path in production (`configuration/production.yaml`) if the binary is run from a directory that does not contain `templates/`:

```yaml
application:
  templates_dir: "/app/templates"
```

#### 5. Initialise `EmailTemplateEngine` in `startup.rs`

In `Application::build()`:

```rust
let engine = EmailTemplateEngine::new(&configuration.application.templates_dir)
    .expect("Failed to initialise email template engine");
```

In `run()`, add `engine: EmailTemplateEngine` as a parameter, wrap it in `web::Data`, and register it:

```rust
let engine = web::Data::new(engine);
// ...
.app_data(engine.clone())
```

#### 6. Modify `subscriptions.rs`

Accept `web::Data<EmailTemplateEngine>` in `subscribe()`:

```rust
pub async fn subscribe(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
    template_engine: web::Data<EmailTemplateEngine>,  // NEW
) -> HttpResponse { … }
```

Update `send_confirmation_email()` to render via the engine:

```rust
// Before
let html_body = format!("Welcome to our newsletter!<br />Click <a href=\"{}\">here</a>…", …);

// After
let html_body = match template_engine.render_confirmation_email(
    new_subscriber.name.as_ref(),
    &confirmation_link,
) {
    Ok(body) => body,
    Err(e) => {
        tracing::error!("Failed to render confirmation email template: {:?}", e);
        return Err(/* map to reqwest::Error or propagate as a new error type */);
    }
};
```

Because `send_confirmation_email` currently returns `Result<(), reqwest::Error>`, the function signature must be widened to accommodate template errors. Two approaches:

- **Option A (preferred):** Change the return type to `Result<(), anyhow::Error>` (add `anyhow` to dependencies) and map both `tera::Error` and `reqwest::Error` with `?`.
- **Option B:** Render the template in the `subscribe()` handler body before calling `send_confirmation_email()`, keeping the function signature unchanged.

Apply the same change to `send_reminder_email()`.

#### 7. Propagate Template Errors as 500

Wherever a `render_*` call returns `Err`, log the error and return `HttpResponse::InternalServerError().finish()`. Template rendering errors are server-side bugs (misconfigured template path or bad template syntax) and should never reach the client as a 500 in production — the startup-time `expect()` in `Application::build()` ensures templates parse correctly before the server accepts traffic.

---

### Error Handling Strategy

| Failure Point                      | When It Happens          | How It Is Handled                               |
| ---------------------------------- | ------------------------ | ----------------------------------------------- |
| Template directory not found       | Server startup           | `expect()` — process exits with a clear message |
| Template syntax error              | Server startup           | `expect()` — process exits with a clear message |
| Missing context variable           | Request time (rendering) | `tera` returns `Err`; handler returns 500       |
| Template found, rendered correctly | Request time             | `Ok(String)` — passed directly to `EmailClient` |

The startup-time guard (`expect()` on `EmailTemplateEngine::new()`) means that in practice a missing-variable error at request time indicates a programmer error (a new field was added to the template but not to the context). This is caught in tests before reaching production.

---

### Test Cases

#### Unit Tests — `src/email_client/templates.rs` (`#[cfg(test)]`)

These tests call `render_*` directly without spinning up the HTTP server. They require the `templates/` directory to be present relative to the workspace root (which it always is when running `cargo test`).

| #   | Test Name                                                   | Scenario                                                | Expected                                                                         |
| --- | ----------------------------------------------------------- | ------------------------------------------------------- | -------------------------------------------------------------------------------- |
| 1   | `confirmation_email_contains_subscriber_name`               | Render with `name = "Alice"`                            | Output HTML contains the string `"Alice"`                                        |
| 2   | `confirmation_email_contains_confirmation_link_as_href`     | Render with a known `confirmation_link`                 | Output HTML contains `href="<link>"` (button CTA)                                |
| 3   | `confirmation_email_contains_raw_link_as_visible_text`      | Render with a known `confirmation_link`                 | Output HTML contains the raw URL string rendered as visible text (not just href) |
| 4   | `confirmation_email_raw_link_and_button_link_are_identical` | Render with a known link                                | The link appears at least twice in the output (button + fallback text)           |
| 5   | `already_subscribed_email_contains_subscriber_name`         | Render with `name = "Bob"`                              | Output HTML contains the string `"Bob"`                                          |
| 6   | `already_subscribed_email_does_not_contain_a_link`          | Render with `name = "Bob"`                              | Output HTML contains no `href` pointing to `/subscriptions/confirm`              |
| 7   | `unknown_template_name_returns_error`                       | Call `self.0.render("nonexistent.html", &ctx)` directly | Returns `Err`                                                                    |

#### Integration Tests — `tests/api/subscriptions.rs`

| #   | Test Name                                                          | Description                                                            | Expected                                                                      |
| --- | ------------------------------------------------------------------ | ---------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| 8   | `confirmation_email_body_contains_confirmation_link`               | Subscribe, inspect the email captured by the mock server               | Email body contains the full confirmation URL                                 |
| 9   | `confirmation_email_body_contains_raw_link_text`                   | Subscribe, inspect the email captured by the mock server               | Email body contains the URL as plain visible text (raw-link fallback section) |
| 10  | `confirmation_email_body_contains_subscriber_name`                 | Subscribe as "Carol", inspect captured email                           | Email body contains "Carol"                                                   |
| 11  | `already_subscribed_email_body_contains_subscriber_name`           | Subscribe, confirm, subscribe again; inspect the second captured email | Second email body contains the subscriber's name                              |
| 12  | `already_subscribed_email_body_does_not_contain_confirmation_link` | Subscribe, confirm, subscribe again; inspect second email              | Second email body does not contain a `/subscriptions/confirm` URL             |

---

### Files to Change

| File                                       | Change                                                                                                                               |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| `Cargo.toml`                               | Add `tera = "1"`                                                                                                                     |
| `templates/emails/confirmation.html`       | **New file.** Full HTML confirmation email template with `{{ name }}` and `{{ confirmation_link }}`                                  |
| `templates/emails/already_subscribed.html` | **New file.** Full HTML already-subscribed email template with `{{ name }}`                                                          |
| `src/email_client/templates.rs`            | **New file.** `EmailTemplateEngine` newtype, `new()`, `render_confirmation_email()`, `render_already_subscribed_email()`, unit tests |
| `src/email_client/mod.rs`                  | Add `pub mod templates;` and `pub use templates::EmailTemplateEngine;`                                                               |
| `src/configuration.rs`                     | Add `templates_dir: String` to `ApplicationSettings`                                                                                 |
| `configuration/base.yaml`                  | Add `application.templates_dir: "templates"`                                                                                         |
| `configuration/production.yaml`            | Add `application.templates_dir: "/app/templates"` (or Docker-appropriate absolute path)                                              |
| `src/startup.rs`                           | Construct `EmailTemplateEngine`, pass it to `run()`, register as `app_data`                                                          |
| `src/routes/subscriptions.rs`              | Accept `web::Data<EmailTemplateEngine>`, use it in `send_confirmation_email()` and `send_reminder_email()`                           |
| `tests/api/subscriptions.rs`               | Add integration tests #8–12                                                                                                          |

---

### Open Questions

1. **Compile-time embedding vs. runtime loading?**
   Proposed default: **Runtime loading** (default `templates_dir` in config). This allows hot-editing templates during development without recompiling. For production, the `templates/` directory is copied into the Docker image alongside the binary (see `Dockerfile`). An alternative is to embed templates at compile time using `include_str!` and `Tera::default()` + `add_raw_template()`, which produces a single self-contained binary. This can be revisited when the binary is deployed to environments without a writable filesystem.

2. **Error type for `send_confirmation_email()` and `send_reminder_email()`?**
   Proposed default: **`anyhow::Error`**. Both functions already live in the application layer (not the domain). Using `anyhow::Error` avoids a bespoke error enum for what are operationally two distinct failure modes (render failure vs. HTTP send failure), and `anyhow` is already a common dependency in Zero2Prod-style projects. If the project later adopts `thiserror` for structured errors, these can be migrated then.

3. **Should a plain-text (`text/plain`) version of each email be generated and sent as a multipart message?**
   Proposed default: **No** — out of scope for this feature. The raw-link fallback in the HTML template handles the most common case (link-stripping clients). True multipart `text/html` + `text/plain` requires changes to `EmailBody` and the `EmailClient` API. It can be a separate feature.

4. **Should templates be validated for required variables at startup (not just parsed)?**
   Proposed default: **No** — Tera validates only syntax at parse time, not whether required variables are present. Variable presence is validated by the unit tests (#1–7 above). Adding a startup-time variable check would require rendering each template with dummy data, which is fragile. Tests are the right safety net here.

5. **Should the `Dockerfile` copy the `templates/` directory into the image?**
   Proposed default: **Yes** — a new `COPY templates/ /app/templates/` instruction must be added to `Dockerfile`, and `production.yaml` must set `application.templates_dir = "/app/templates"`. This is a deployment concern that is part of this feature.

---

### Feature 4 - Finished on 2026-03-08

---
