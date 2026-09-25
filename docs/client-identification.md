# Launcher client identification — API implementation notes

**Audience:** whoever maintains `api.reforgermods.net` (ReforgerWorkshopAPI).

**Status: implemented on both sides.** The launcher sends the header described
below, and the API records it as a derived key. The sections headed *Suggested
API-side work* are kept as the rationale for what was built; what actually
shipped is summarised in [What the API does](#what-the-api-does).

The header remains additive: an API that ignores it behaves exactly as before,
and the launcher does not care whether it is read.

## The problem

Today the API can see "50 000 requests from the launcher" but not whether that
is 5 000 installs making 10 requests each, or 50 installs making 1 000 each.
`User-Agent` identifies the *application*; client IP is a poor proxy for a
*person* (CGNAT, shared households, mobile, VPNs) and is also the thing you least
want to retain.

## What the launcher sends

Every request carries both of:

```http
User-Agent: reforgermods.net-launcher/0.1.0
X-Launcher-Install-Id: 8f14e45f-ceea-467a-9c2b-1f2a3b4c5d6e
```

| Property | Value |
| --- | --- |
| Header | `X-Launcher-Install-Id` |
| Format | Lowercase canonical UUID, `8-4-4-4-12` hex. Always exactly 36 chars. UUID version 8 (custom derivation) where machine-derived, version 4 where random. |
| Generation | `SHA-256(application salt ‖ machine value)`, first 16 bytes, formatted as a UUID. |
| Lifetime | Survives uninstall and reinstall, because it is derived rather than stored. Changes when the OS is reinstalled. |
| Fallback | A random v4 UUID persisted in the launcher's state file, used only where no machine value is readable. That one does *not* survive a reinstall. |
| Reset | None. There is no reset action, by design — see below. |
| Absent when | The stored value fails validation and no machine value is readable. The launcher sends **no header at all** rather than a malformed one. |

### Machine value by platform

| Platform | Source |
| --- | --- |
| Windows | `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid` |
| Linux | `/etc/machine-id`, else `/var/lib/dbus/machine-id` |
| Other / unreadable | random fallback |

These are per-OS-installation values. They are not device serials, not MAC
addresses, not tied to a user account, and they change when the OS is
reinstalled — which is the intended granularity.

**The machine value never leaves the process.** Only the salted digest is sent,
so the API cannot reverse the identifier into a machine id, and the same machine
running different software derives a different value. An all-zero or blank
machine value is rejected rather than used, since it would collapse every
affected machine onto one identifier.

**Why not IP.** An address changes with DHCP, VPNs, tethering and moving
networks, so it would split one install into many, while merging everyone behind
a shared NAT into one. It is also the one genuinely personal field in the
request, and the API already sees it.

Client-side references: `INSTALL_ID_HEADER` in
`crates/rfm-core/src/api/client.rs`, `stable_install_id` in
`crates/rfm-core/src/identity.rs`.

## What it is not

**It is not authentication, and it must not gate anything.** The launcher is a
public, untrusted client. Anyone can send any value, rotate it per request, or
copy someone else's. Treat it exactly as what it is: a voluntary, spoofable
analytics hint.

Concretely:

- **Do not** rate limit on it alone. Keep the existing IP-based limiting as the
  enforcement mechanism. A caller that wants to evade a per-install limit only
  has to send a new UUID each time, so an install-keyed limit would penalise
  honest clients and stop nobody.
- **Do not** use it for entitlement, quota assignment, or moderation.
- **Do** use it for counting, cohorting, and retention analysis.

If per-client enforcement is ever needed, that requires real launcher
authentication, which is a separate design.

## What the API does

Implemented in `ReforgerWorkshopAPI`, matching the design below except where
noted:

| Concern | Where |
| --- | --- |
| Header constant | `telemetry.InstallIDHeader` |
| Validation | `telemetry.ValidInstallID` — canonical lowercase UUID, any version; the nil UUID is rejected |
| Derived key | `Anonymizer.InstallKey` — HMAC-SHA256(secret, id), truncated to 128 bits, domain-separated from `NetworkID` |
| Storage | `request_events.install_key`, with a partial index; added by the existing `ensureColumn` migration |
| Classification | `launcher` client kind and source; counts as activity |
| Version | split out of the User-Agent into `client_version` |
| Redaction | the raw header is in the middleware's redacted list, so it never reaches the stored header snapshot |

Two deliberate departures from the design below:

- **The key is stored as hex `TEXT`, not `BLOB(16)`**, for consistency with
  `network_id` and `quota_subject_hash`. The value is the same 128 bits.
- **The key is not rotated per time window**, unlike `NetworkID`. A key that
  changes underneath the data every month cannot answer "how many distinct
  installs, and do they come back", which is the only reason the field exists.
  Correlation is bounded by rotating the *secret* instead, which re-partitions
  the whole keyspace from that point on. This is the lever referred to in
  [Note for the API side](#note-for-the-api-side), and it matters more given the
  client value is now stable and derived.

The five assertions under [Testing the API side](#testing-the-api-side) are
covered by tests: `telemetry/install_id_test.go` for validation and keying, and
`TestLauncher*` in `api/telemetry_middleware_test.go` end to end, including that
a caller rotating the id per request gets no more requests than one sending none.

## Suggested API-side work

### 1. Accept and validate the header

Reject-by-ignoring, never by erroring. Suggested handling:

```text
raw := r.Header.Get("X-Launcher-Install-Id")
if raw is not exactly a canonical lowercase v4-shaped UUID -> treat as absent
```

Never log the raw value unvalidated, and never echo it back in a response body
or error envelope.

### 2. Store a salted hash, not the value

The launcher generates the id, but the API does not need to retain it verbatim
to count distinct installs:

```text
install_key = truncate(HMAC-SHA256(server_secret, install_id), 128 bits)
```

With a server-side secret this keeps the stored value useless if the analytics
store leaks, and it prevents anyone who guesses or harvests an id from querying
for that install's activity. Rotating the secret on a schedule (say quarterly)
caps how far back any single key can be correlated.

### 3. Record it on the existing telemetry path

The API already classifies routes in `telemetry/classify.go` and records request
telemetry in `api/telemetry_middleware.go`. The natural shape is one extra
nullable column on whatever already records a request, plus a rollup:

| Field | Type | Notes |
| --- | --- | --- |
| `install_key` | `BLOB(16)` nullable | Hashed per above. Null for non-launcher traffic. |
| `client_kind` | `TEXT` | `launcher` / `web` / `other`, parsed from User-Agent. |
| `client_version` | `TEXT` | From the launcher's User-Agent suffix. |

Daily rollup suitable for the question that prompted this:

```sql
SELECT
  date(ts)                      AS day,
  client_version,
  COUNT(*)                      AS requests,
  COUNT(DISTINCT install_key)   AS installs,
  COUNT(*) * 1.0 /
    NULLIF(COUNT(DISTINCT install_key), 0) AS requests_per_install
FROM request_telemetry
WHERE client_kind = 'launcher'
GROUP BY day, client_version;
```

That answers "50 000 requests, 5 000 installs, 10 each" directly, and splits it
by launcher version so a regression in request volume is attributable to a
release.

### 4. Retention

The install key is only needed at daily/weekly granularity. Recommend keeping
per-request rows for the existing telemetry window, and rolling up to
`(day, client_version, install_key)` presence rows beyond that — or to pure
counts once the cohort window has passed. There is no product reason to retain
per-install request logs indefinitely.

### 5. Do not add it to the public rate-limit response

`GET /v2/rate-limits` reports the caller's limits. Since the install id does not
affect limits (see above), it should not appear there — surfacing it would imply
it does something it does not.

## Privacy position

Worth stating plainly, because this changed: the identifier is now **derived
from the machine rather than randomly generated**, and there is **no reset**.
That is deliberate — a resettable random value made reinstalls look like new
installs, which was the exact number the metric exists to get right — but it is
a real trade, and the honest framing is:

- The identifier is a salted hash of a per-OS-installation value. It carries no
  personal data and cannot be reversed into a machine identity.
- It is stable across reinstalls of the launcher, and changes when the OS is
  reinstalled.
- It is not linked to a reforgermods.net account, because the launcher has none.
- Its sole purpose is aggregate usage measurement.
- It is not shown in the UI. It appears only when the launcher is started with
  `RFM_DEBUG=1`, which is a developer affordance, not a user setting.

Anyone wanting genuinely uncorrelatable requests has to block the header, which
is a thing a user cannot do from inside the launcher. If that matters for the
project's public posture, the mitigation is disclosure — a line in the
launcher's description or privacy text — rather than an in-app toggle, because a
toggle would reintroduce exactly the undercount the derivation fixes.

## Note for the API side

Because the client value is now stable and derived, the server-side hashing in
step 2 matters more, not less: it is what stops the analytics store from holding
a long-lived per-machine key. Rotating the server secret on a schedule also caps
how far back any single key can be correlated, which is the main lever left for
limiting retention.

## Testing the API side

Once implemented, these should hold:

1. A request with no header is accepted and recorded with a null `install_key`.
2. A request with a malformed header (`not-a-uuid`, 10 KB of text, CRLF
   injection attempt) is accepted and recorded as if absent — never 4xx, never
   logged raw.
3. Two requests with the same id produce the same `install_key`; with different
   ids, different keys.
4. The same id sent to two different deployments with different secrets produces
   different keys.
5. Rate limiting behaviour is unchanged by the presence or value of the header.

## Open question for the API owner

Should the launcher also send a coarse platform token (`windows` / `linux`) so
the rollup can distinguish "5 000 Windows installs" from a Proton cohort? That
would be one more low-cardinality header and would answer the Linux-support
question from real data rather than guesswork. The launcher does **not** send one
today; say the word and it is a two-line change.

**Still open, and now worth deciding.** The beta publishes an Arch package, so
there is a Linux cohort to measure for the first time. Note that the question is
partly answerable without a new header: the User-Agent is recorded, and the
Windows and Linux builds could carry distinguishable version suffixes. A separate
low-cardinality header is cleaner, and neither is built.
