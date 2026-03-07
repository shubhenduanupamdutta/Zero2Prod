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
