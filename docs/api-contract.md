# reforgermods.net API surface used by the launcher

Base URL: `https://api.reforgermods.net`, fixed. There is deliberately **no
setting** for it: a user-editable origin could redirect every request — including
the install identifier — at an arbitrary host. Development builds override it
with the `RFM_API_BASE_URL` environment variable.

The launcher is an anonymous public client. It sends no API key and no
credentials — only:

```http
User-Agent: reforgermods.net-launcher/<version>
X-Launcher-Install-Id: <random v4 UUID, generated on first run, user-resettable>
```

The install id lets the API count distinct installs rather than only requests.
It is a spoofable analytics hint, never authentication, and must not gate
anything. Full spec and suggested API-side handling:
[`client-identification.md`](client-identification.md).

Public rate limit at time of writing (`GET /v2/rate-limits`, unauthenticated):
60 requests/minute, burst 20, 5 000/day, keyed by client IP.

## Endpoints

### `GET /v2/servers`

The server browser. Parameters the launcher sends:

| Parameter | Values | Notes |
| --- | --- | --- |
| `page` | 1–10000 | |
| `perPage` | 1–500 | Launcher default 100 |
| `sort` | `players`, `name`, `newest`, `lastSeen` | Default `players` |
| `search` | free text | Matches server names |
| `region` | ping-site id | From `/v2/servers/ping-sites` |
| `platform` | `pc`, `xbox`, `psn` | Client platform, canonicalised server-side |
| `official` | `true`/`false` | Omitted entirely when unset |
| `hasMods` | `true`/`false` | |
| `battleye` | `true`/`false` | |
| `locked` | `true`/`false` | Password-protected |
| `includeOffline` | `true` | Omitted for online-only, the default |

Tri-state filters are omitted when unset, because `official=false` means
"community only" and is not the same as "do not filter".

**Two filters do not exist server-side** and are applied client-side to the
loaded page. Neither is ever sent, and neither varies the response cache key:

- `minPlayers` — no minimum-population parameter exists.
- `scenario` — no scenario parameter exists. `search` *does* match scenario text
  as well as server names, but sending a scenario name there would also match
  server names and would collide with the user's own search term. The picker is
  populated from the distinct `scenarioName` values on the loaded page, collected
  before filtering so choosing one does not empty the picker that chose it.

Both are labelled as page-local in the UI, and the launcher reports how many rows
they removed rather than silently showing fewer.

Response carries `meta` (pagination), `dataset` (snapshot freshness) and `data`.

### `GET /v2/servers/{id}`

Detail for one server, keyed by the Reforger lobby room UUID. Adds `present`,
`joinable`, `fps`, `activity` (24 h / 7 d population aggregates), `modSummary`
and the full `mods` array with per-mod `version` and `size`.

The launcher reads `fps` as *server simulation FPS*, which is what the API
reports. It is not a client ping, and is not labelled as one.

### `GET /v2/servers/{id}/history`

Population history for the inspector's chart.

| Parameter | Values |
| --- | --- |
| `range` | `6h`, `24h`, `72h`, `7d`, `30d`, `90d`, `1y`, `all` |

Anything else is rejected with `INVALID_RANGE`, so the launcher models this as an
enum rather than a free string. Omitting `range` defaults to `24h`.

The response names the bucket width it chose, which the launcher displays rather
than assuming:

| Range | Points | Bucket |
| --- | ---: | --- |
| `6h` | 72 | `5m` |
| `24h` | 288 | `5m` |
| `7d` | 167 | `hour` |
| `30d` | 636 | `hour` |

Points are `{t, avg, min, max, cap}` with `t` in unix seconds, ascending. A server
with no recorded history returns an empty `points` array — a normal state, not an
error, and rendered as "no recorded history for this range".

The launcher offers `6h`/`24h`/`7d`/`30d`. `90d`, `1y` and `all` are supported by
the API but are not useful in a compact panel.

### `GET /v2/servers/ping-sites`

Region ids and human labels, used to populate the region filter. Cached for an
hour; the launcher falls back to humanising the raw id if the call fails.

### `GET /v2/health`

Liveness probe, used only as the status fallback described below.

## Snapshot freshness, and `/v2/status`

**`GET /v2/status` is not deployed on `api.reforgermods.net`.** Verified
2026-09-24: it answers `404 NOT_FOUND`. The launcher still calls it, treats a 404
on that fixed route as `EndpointUnavailable` (distinct from "server not found"),
and degrades rather than showing an error.

The header does not need it in the common case anyway. Every server response
already carries the indexer's snapshot freshness:

```json
"dataset": {
  "warming": false,
  "stale": false,
  "snapshotAgeSeconds": 8.27,
  "lastCollectionAt": "2026-09-24T06:54:58Z",
  "onlineServerCount": 4999
}
```

So "Server data updated 18s ago" is rendered from a request the launcher was
making regardless — no extra polling, and no duplication of backend health logic.
The resolution order is:

1. `/v2/status` — if it answers, its `status` word and `dataset` win.
2. `/v2/health` plus the most recent `dataset` seen on a server response.
3. Neither answers → an unobtrusive "API unreachable" state.

When `/v2/status` ships, `StatusReport` in `crates/rfm-core/src/api/models.rs`
should be tightened to its real shape; it is intentionally all-optional today so
an unknown response body cannot break the header.

## Server identifiers

Two shapes, both joinable:

| Kind | Example | Count |
| --- | --- | --- |
| Community | `73e8fb7f-8789-4d88-8aa8-ea69b9aa092f` | the majority |
| Official | `nitrado_17516010` | 364 at time of writing |

Sampled across 600 servers, every non-UUID id was official and every official id
was `nitrado_` followed by digits. `ServerId::parse` accepts both; treating ids
as UUID-only silently made every official server unjoinable, and the failure
only showed when someone pressed Join.

The API publishes `joinUrl` for community servers but **not** for official ones,
so the official join URI is built locally using the same payload shape with the
provider id in place of the UUID. That is consistent with the verified community
mechanism but is **not yet confirmed against a live client** for official
servers specifically.

## Fleet population

`GET /v2/servers/history?range=6h` returns a time series plus a `now` block:

```json
"now": {"onlineServers": 5089, "players": 16352, "capacity": 224986, "queue": 694}
```

`now.players` is the only fleet-wide player total the API exposes — the
`dataset` object on other responses counts servers, not players — so the header
total comes from here. The launcher reads only `now` and ignores the series.

## Scenario imagery

`scenarioImageUrl` and `scenarioThumbnailUrl` are absent for roughly half of
servers. Across 300 sampled servers, every URL that *was* present used `https`
on exactly one host:

```text
https://ar-gcp-cdn.bistudio.com/...
```

The launcher validates against that exact host before rendering
(`safeImageUrl` in `src/lib/format.ts`), and the page CSP `img-src` enforces the
same allowlist independently. Anything else renders as no image.

## Server addresses and ping

`GET /v2/servers/{id}` returns `host` and `port`. Measured across 30 live
servers: `host` was present on 20 and was a **literal IP address in all 20** —
never a hostname.

The launcher uses `host` for direct ICMP latency measurement, and validates it
first: literal public IPs only, with loopback, private, link-local, CGNAT,
multicast and documentation ranges refused. Remote data must not be able to
choose a DNS lookup or aim probes at the user's own network.

`port` is parsed but nothing connects to it. TCP connect timing was evaluated as
a ping method and **rejected**: see `README.md`.

## Field names worth noting

- `battlEye` — capital E, mid-word. Mis-spelling the rename silently reads
  `false` for every server, so there is a test asserting the mapping against a
  captured response.
- `platform` is the **host OS** (`LINUX`/`WINDOWS`). `platforms` is the array of
  **client** platforms (`pc`, `xbox`, `psn`). They are unrelated.
- Workshop mod ids are 16 uppercase hex characters; server ids are lowercase
  UUIDs. Both are validated before use.

## Deliberately unused

- `/v2/servers/{id}/mods` — the detail endpoint already returns the mod list
  *with* resolved sizes; this one skips size enrichment by default.
- `/v2/mods/*`, `/v2/tools/*` — mod browsing and planning are not in scope yet.
- `activity` on the server detail — superseded in the inspector by the history
  chart, which shows the same shape with more resolution. Still parsed.
- `analytics` on each server — returned and parsed, but not displayed yet.

## Test fixtures

`crates/rfm-core/tests/fixtures/` holds real captured responses, and the model
tests parse them. Refresh them with:

```sh
curl -s 'https://api.reforgermods.net/v2/servers?perPage=3&sort=players' \
  -o crates/rfm-core/tests/fixtures/servers_page.json
curl -s 'https://api.reforgermods.net/v2/servers/73e8fb7f-8789-4d88-8aa8-ea69b9aa092f' \
  -o crates/rfm-core/tests/fixtures/server_detail.json
curl -s 'https://api.reforgermods.net/v2/servers/ping-sites' \
  -o crates/rfm-core/tests/fixtures/ping_sites.json
curl -s 'https://api.reforgermods.net/v2/servers/73e8fb7f-8789-4d88-8aa8-ea69b9aa092f/history?range=24h' \
  -o crates/rfm-core/tests/fixtures/server_history.json
```

`crates/rfm-core/tests/live_api.rs` checks the contract against the live API and
is ignored by default:

```sh
cargo test -p rfm-core --test live_api -- --ignored
```
