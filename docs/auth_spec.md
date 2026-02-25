# Authentication Specification

In Amber Clad authentication for all client types: TUI, web, Android, LLM agents,
and third-party clients.

## Design Principles

1. **One protocol, all clients.** The WebSocket JSON protocol is the only interface.
   There is no separate REST API, no web form, no special endpoint. Every client --
   ours or third-party -- registers and logs in the same way.

2. **Token-based, not password-based.** Tokens are generated server-side, returned
   once at registration, and used for all subsequent logins. No passwords, no
   interactive prompts, no client-side hashing. This works natively for both human
   clients (store in a file) and LLM agents (store in config/env).

3. **Server stores hashes, not tokens.** The raw token is shown exactly once (at
   registration). The server stores only `hash(token)`. A database leak does not
   compromise accounts.

4. **Token loss = account loss.** There is no recovery mechanism. Register a new
   name and start over. Space is unforgiving. (See "Future: Multi-Device and Token
   Recovery" for planned evolution.)


## Protocol

### Registration

Client opens a WebSocket connection and sends:

```json
{"auth": {"player_name": "<name>", "action": "register"}}
```

Server validates the request and responds:

```json
{"auth_result": {"success": true, "player_id": 42, "token": "a3f8c9..."}}
```

The token is a 256-bit (32-byte) cryptographically random value, hex-encoded (64
characters). This is the **only time** the token is transmitted. The client must
store it securely.

On failure:

```json
{"auth_result": {"success": false, "message": "<reason>"}}
```

The server closes the connection after a failed registration attempt.

### Login

Client opens a WebSocket connection and sends:

```json
{"auth": {"player_name": "<name>", "action": "login", "token": "a3f8c9..."}}
```

Server validates the request and responds:

```json
{"auth_result": {"success": true, "player_id": 42}}
```

No token is returned on login -- the client already has it.

On failure, the server returns a generic error (does not reveal whether the name
exists or the token was wrong) and closes the connection:

```json
{"auth_result": {"success": false, "message": "invalid credentials"}}
```

### Token Verification

Server-side verification flow:

1. Look up player by name.
2. If player not found, return generic failure (do not reveal name existence).
3. Hash the provided token with the same algorithm used at registration.
4. Constant-time compare against stored `token_hash`.
5. On match, bind session to player and send game state.
6. On mismatch, return generic failure and close connection.

Constant-time comparison prevents timing side-channel attacks.


## Hashing

Tokens are hashed with SHA-256 before storage. The input is already a 256-bit
cryptographically random value, so salting is unnecessary -- preimage attacks and
rainbow tables are not viable against high-entropy random inputs.

```
stored_hash = SHA-256(raw_token)
```

If token generation ever changes to accept lower-entropy inputs (e.g., user-chosen
secrets), this must be upgraded to a proper KDF (Argon2, bcrypt) with per-account
salts.


## Name Validation

Player names must satisfy:

- Length: 3 to 24 characters (inclusive).
- Character set: `[a-zA-Z0-9_-]` (ASCII alphanumeric, underscores, hyphens).
- No leading or trailing hyphens/underscores.
- Case-sensitive: "Admiral" and "admiral" are distinct accounts.
- Uniqueness enforced by the database (UNIQUE constraint on `players.name`).

Reserved names that cannot be registered:

```
admin, administrator, server, system, moderator, mod, npc, mlm, gm, gamemaster
```

The reserved list is checked case-insensitively (e.g., "Admin", "ADMIN" are all
blocked).


## Anti-Griefing Layers

### Layer 1: Player Cap

Hard limit on total registered accounts. Once reached, registration returns an error;
existing players can still log in.

- Default: 200 (configurable at server start).
- Checked before any other registration logic.

### Layer 2: Per-IP Registration Rate Limit

Tracks registration attempts per source IP address. In-memory table, not persisted.

- Default: 2 registrations per IP per rolling 1-hour window.
- Applies to successful registrations only (failed attempts due to name collision
  etc. do not consume quota).
- Window tracked as server tick ranges.

### Layer 3: Per-IP Connection Rate Limit

Limits how fast any single IP can open new WebSocket connections (registration or
login). Protects against both registration spam and brute-force login attempts.

- Default: 10 connections per IP per rolling 1-minute window.
- Enforced at the WebSocket accept stage, before any JSON processing.
- Rejected connections receive WebSocket close code 1008 (Policy Violation) with
  reason "rate limited".

### Layer 4: Auth Failure Lockout

After repeated failed login attempts from the same IP, impose escalating cooldowns.

- 5 failures in 5 minutes: 30-second cooldown before next attempt accepted.
- 10 failures in 15 minutes: 5-minute cooldown.
- 20 failures in 1 hour: 1-hour cooldown.
- Tracked in-memory per IP. Resets on server restart.

### Summary

```
Layer  | What                        | Default Limit        | Scope
-------|-----------------------------|----------------------|--------
1      | Player cap                  | 200 accounts         | Global
2      | Registration rate limit     | 2/hour               | Per IP
3      | Connection rate limit       | 10/minute            | Per IP
4      | Auth failure lockout        | Escalating cooldowns | Per IP
```


## Database Schema Changes

Add columns to the `players` table:

```sql
ALTER TABLE players ADD COLUMN token_hash BLOB;
ALTER TABLE players ADD COLUMN created_at INTEGER;
ALTER TABLE players ADD COLUMN last_login_at INTEGER;
```

- `token_hash`: 32-byte SHA-256 hash of the registration token.
- `created_at`: Unix timestamp (seconds) of account creation.
- `last_login_at`: Unix timestamp of most recent successful login. Updated each
  login. Used for inactive account reaping.

Rate limiting is in-memory only -- no schema needed.


## Protocol Changes

### AuthRequest (Client -> Server)

```zig
pub const AuthRequest = struct {
    player_name: []const u8,
    action: enum { register, login },
    token: ?[]const u8 = null,      // required for login, ignored for register
    client_type: ?ClientType = null, // optional self-report
};
```

The `action` field replaces the current implicit behavior where `token == null`
means registration. Explicit actions are clearer for third-party implementors.

The `client_type` field is an optional self-report. The server does not enforce it
or use it for security decisions -- it exists for analytics, matchmaking, and UI
purposes.

### AuthResult (Server -> Client)

No structural changes. The existing `AuthResult` already has all needed fields:

```zig
pub const AuthResult = struct {
    success: bool,
    player_id: ?u64 = null,
    token: ?[]const u8 = null,   // populated only on successful registration
    message: ?[]const u8 = null, // error reason on failure
};
```

### New Error Codes

```zig
pub const ErrorCode = enum(u16) {
    // ... existing codes ...
    auth_failed = 2000,
    already_authenticated = 2001,
    registration_closed = 2002,    // player cap reached
    rate_limited = 2003,           // IP rate limit hit
    invalid_player_name = 2004,    // name validation failed
    name_taken = 2005,             // registration with existing name
};
```


## Client Token Storage

How each client type should store the token after registration:

| Client              | Storage                                          |
|---------------------|--------------------------------------------------|
| TUI (our client)    | `~/.iac/credentials` file, mode 0600             |
| Web app             | `localStorage` (acceptable for games; HttpOnly   |
|                     | cookie if security requirements increase)         |
| Android app         | Android Keystore or EncryptedSharedPreferences    |
| LLM agent           | Config file, environment variable, or secret store|
| Third-party clients | Their choice -- the protocol doesn't dictate this |

Our TUI client flow:

1. First launch: prompt for player name, send `register`, write token to
   `~/.iac/credentials`.
2. Subsequent launches: read `~/.iac/credentials`, send `login`.
3. If login fails (token invalid / account gone): prompt to re-register.


## Inactive Account Reaping

To prevent the player cap from filling with abandoned accounts:

- Accounts with no login for 90 days are eligible for reaping.
- Reaping clears `token_hash` (preventing login) and marks the name as available
  for re-registration.
- Reaping does NOT delete game state (fleets, buildings, etc.) immediately.
  Orphaned game state is cleaned up lazily or on a separate schedule.
- The 90-day threshold is configurable at server start.
- Reaping runs as a periodic server task (e.g., once per day at server tick
  boundary).


## Future: Multi-Device and Token Recovery

The current design binds one token to one account. A player who logs in from their
phone cannot simultaneously log in from the web without sharing the same token
across devices (which works but requires manual token transfer).

Planned evolution (not in scope for current milestone):

### Multi-Device Sessions

Allow multiple simultaneous connections for the same player account. Each
connection shares the same player state. The server already tracks sessions
independently of players (`ClientSession.player_id`), so the data model supports
this -- the work is in handling concurrent command conflicts (last-write-wins,
command queuing, etc.).

### Token Regeneration

Add an authenticated endpoint to generate a new token (invalidating the old one).
A player logged in on device A can regenerate their token and use the new one on
device B. The old token stops working immediately.

```json
{"auth": {"action": "regenerate_token", "token": "<current_token>"}}
```

Returns a new token, same as registration. This requires an active authenticated
session, so a player who has lost their only token still cannot recover -- they
must re-register. This is intentional.

### Device-Specific Tokens (Further Future)

Issue multiple independent tokens per account, each identified by a device label.
Revoking one does not affect others. This is the full multi-device solution but
adds significant complexity (token table, per-device tracking, revocation UI).

Not planned until there is demonstrated demand.


## Implementation Priority

For the current milestone:

1. **Must-have**: `action` field in AuthRequest, token generation, hash storage,
   token verification, name validation, player cap.
2. **Should-have**: Per-IP registration rate limit, per-IP connection rate limit.
3. **Nice-to-have**: Auth failure lockout, inactive account reaping.
4. **Future**: Multi-device sessions, token regeneration, device-specific tokens.
