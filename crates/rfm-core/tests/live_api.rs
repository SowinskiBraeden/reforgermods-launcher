//! Live checks against api.reforgermods.net.
//!
//! Ignored by default so `cargo test` stays offline and deterministic. Run them
//! when changing the client or when the API contract may have moved:
//!
//! ```text
//! cargo test -p rfm-core --test live_api -- --ignored --test-threads=1
//! ```
//!
//! **Run them serially.** The public rate limit is 60 requests/minute and these
//! tests fetch server detail in loops; in parallel they trip it and fail with
//! `RATE_LIMITED`, which is the limiter working rather than a contract break.

use rfm_core::api::{ApiClient, HistoryRange, ServerQuery, ServerSort, DEFAULT_BASE_URL};
use rfm_core::launch::{steam_join_uri, ServerId};

fn client() -> ApiClient {
    // A fixed id, so live runs are attributable to the test suite rather than
    // looking like a new install on every run.
    ApiClient::with_base_url(DEFAULT_BASE_URL, "00000000-0000-4000-8000-00000000test")
        .or_else(|_| ApiClient::with_base_url(DEFAULT_BASE_URL, ""))
        .expect("client builds")
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn server_list_returns_a_usable_page() {
    let query = ServerQuery {
        sort: ServerSort::Players,
        per_page: 10,
        ..Default::default()
    };
    let page = client().servers(&query).await.expect("servers request");

    assert_eq!(page.data.len(), 10, "asked for 10 servers");
    assert!(page.meta.total_servers > 0);
    assert!(
        !page.dataset.warming,
        "dataset should be warm in production"
    );
    assert!(
        page.dataset.last_collection_at.is_some(),
        "freshness drives the header label"
    );

    for server in &page.data {
        // Every id the browser shows must be launchable.
        ServerId::parse(&server.id)
            .unwrap_or_else(|e| panic!("server id {:?} is not launchable: {e}", server.id));
        assert!(!server.name.is_empty());
        assert!(server.online, "default query is online-only");
        assert!(server.max_players > 0);
    }

    // sort=players must actually be descending.
    let players: Vec<u32> = page.data.iter().map(|s| s.players).collect();
    let mut sorted = players.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(players, sorted, "list should be sorted by players desc");
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn filters_are_honoured_by_the_api() {
    let client = client();

    let modded = client
        .servers(&ServerQuery {
            has_mods: Some(true),
            per_page: 10,
            ..Default::default()
        })
        .await
        .expect("modded request");
    assert!(
        modded.data.iter().all(|s| s.mod_count > 0),
        "hasMods=true returned a vanilla server"
    );

    let vanilla = client
        .servers(&ServerQuery {
            has_mods: Some(false),
            per_page: 10,
            ..Default::default()
        })
        .await
        .expect("vanilla request");
    assert!(
        vanilla.data.iter().all(|s| s.mod_count == 0),
        "hasMods=false returned a modded server"
    );

    let frankfurt = client
        .servers(&ServerQuery {
            region: Some("frankfurt".into()),
            per_page: 10,
            ..Default::default()
        })
        .await
        .expect("region request");
    assert!(
        frankfurt
            .data
            .iter()
            .all(|s| s.region.as_deref() == Some("frankfurt")),
        "region filter leaked other regions"
    );
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn search_matches_the_returned_names() {
    let page = client()
        .servers(&ServerQuery {
            search: "conflict".into(),
            per_page: 10,
            ..Default::default()
        })
        .await
        .expect("search request");

    assert!(
        !page.data.is_empty(),
        "expected some matches for 'conflict'"
    );
    assert!(
        page.data
            .iter()
            .any(|s| s.name.to_lowercase().contains("conflict")
                || s.scenario_name.to_lowercase().contains("conflict")),
        "search returned nothing resembling the term"
    );
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn server_detail_carries_mods_and_activity() {
    let client = client();
    let page = client
        .servers(&ServerQuery {
            has_mods: Some(true),
            per_page: 5,
            ..Default::default()
        })
        .await
        .expect("list request");
    let first = page.data.first().expect("at least one modded server");
    let id = ServerId::parse(&first.id).expect("valid id");

    let detail = client.server(&id).await.expect("detail request").server;
    assert_eq!(detail.summary.id, id.as_str());
    assert_eq!(detail.mods.len() as u32, detail.mod_summary.count);
    assert!(!detail.mods.is_empty());
    for m in &detail.mods {
        // Workshop ids are 16 hex characters; the modscan matcher depends on it.
        assert_eq!(m.id.len(), 16, "unexpected mod id {:?}", m.id);
        assert!(m.id.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    println!(
        "{} — {} mods, {} present, fps {}",
        detail.summary.name,
        detail.mods.len(),
        detail.present,
        detail.fps
    );
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn ping_sites_cover_the_regions_servers_report() {
    let sites = client().ping_sites().await.expect("ping sites").sites;
    assert!(sites.len() >= 8);
    assert!(sites.iter().any(|s| s.id == "frankfurt"));
    assert!(sites.iter().all(|s| !s.label.is_empty()));
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn a_missing_server_is_reported_as_not_found() {
    // A well-formed id that is not in the dataset.
    let id = ServerId::parse("00000000-0000-4000-8000-000000000000").unwrap();
    let err = client().server(&id).await.expect_err("should not be found");
    assert_eq!(err.kind(), "not_found", "got {err:?}");
    assert!(!err.is_retryable());
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn launcher_status_degrades_when_v2_status_is_absent() {
    // /v2/status is not deployed yet; the header must still show a usable state.
    let status = client().launcher_status(None).await;
    assert!(status.api_reachable, "health probe should answer");
    if status.reported_status.is_none() {
        assert!(
            status.note.is_some(),
            "an absent /v2/status must explain itself"
        );
        println!("status fallback: {}", status.note.unwrap_or_default());
    } else {
        println!("/v2/status is now deployed: {:?}", status.reported_status);
    }
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn a_live_server_produces_a_launchable_uri() {
    let page = client()
        .servers(&ServerQuery {
            per_page: 1,
            ..Default::default()
        })
        .await
        .expect("list request");
    let server = page.data.first().expect("one server");
    let id = ServerId::parse(&server.id).expect("launchable id");
    let uri = steam_join_uri(&id);

    assert!(uri.starts_with("steam://run/1874880/"));
    assert!(uri.ends_with('/'));
    assert!(!uri.contains('='), "payload padding must be stripped");
    println!("{} -> {uri}", server.name);
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn repeated_identical_requests_are_served_from_cache() {
    let client = client();
    let query = ServerQuery {
        per_page: 5,
        ..Default::default()
    };
    let started = std::time::Instant::now();
    client.servers(&query).await.expect("first request");
    let cold = started.elapsed();

    let started = std::time::Instant::now();
    client.servers(&query).await.expect("second request");
    let warm = started.elapsed();

    assert!(
        warm < cold,
        "second identical request should hit the cache (cold {cold:?}, warm {warm:?})"
    );
    println!("cold {cold:?}, warm {warm:?}");
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn history_is_ordered_and_bucketed_per_range() {
    let client = client();
    let id = ServerId::parse("73e8fb7f-8789-4d88-8aa8-ea69b9aa092f").unwrap();

    for range in HistoryRange::OFFERED {
        let history = client
            .server_history(&id, range)
            .await
            .unwrap_or_else(|e| panic!("history {:?} failed: {e}", range.as_param()));

        assert_eq!(history.range, range.as_param());
        assert!(
            !history.bucket.is_empty(),
            "the API names the bucket width it chose"
        );
        assert!(
            !history.points.is_empty(),
            "{} had no points",
            range.as_param()
        );
        // The chart draws in array order, so the series must be ascending.
        assert!(
            history.points.windows(2).all(|w| w[0].t < w[1].t),
            "{} is not ordered by time",
            range.as_param()
        );
        for point in &history.points {
            assert!(point.min <= point.avg, "{point:?}");
            assert!(point.avg <= point.max, "{point:?}");
        }
        println!(
            "{:>4} -> {:4} points, bucket {}",
            range.as_param(),
            history.points.len(),
            history.bucket
        );
    }
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn readiness_can_be_computed_from_a_live_server() {
    use rfm_core::modscan::{AddonInventory, InstalledAddon, ScanError};
    use rfm_core::readiness;

    /// Pretends nothing is installed, so the figures exercise the missing path
    /// without depending on the machine running the test.
    struct Empty;
    impl AddonInventory for Empty {
        fn installed(&self) -> Result<Vec<InstalledAddon>, ScanError> {
            Ok(Vec::new())
        }
    }

    let client = client();
    let page = client
        .servers(&ServerQuery {
            has_mods: Some(true),
            per_page: 1,
            ..Default::default()
        })
        .await
        .expect("list request");
    let id = ServerId::parse(&page.data[0].id).expect("valid id");
    let detail = client.server(&id).await.expect("detail").server;

    let report = readiness::report(&detail.mods, &Empty);
    assert!(report.available);
    assert!(!report.is_ready(), "nothing is installed in this fixture");
    assert_eq!(report.required, detail.mods.len() as u32);
    assert_eq!(report.missing, report.required);
    // At least some sizes should resolve on a real server, so the UI has a
    // number to show rather than only "download size unknown".
    assert!(
        report.missing_bytes > 0,
        "no mod sizes resolved for {}",
        detail.summary.name
    );
    println!(
        "{}: {} mods, at least {:.2} GiB, {} unresolved",
        detail.summary.name,
        report.required,
        report.missing_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        report.unresolved_mods
    );
}

/// End-to-end readiness against a real addon store.
///
/// Point `RFM_ADDONS_DIR` at an Arma Reforger `addons` directory to exercise the
/// real scanner rather than a stub; the test is skipped when it is unset, so it
/// stays runnable on a machine without the game:
///
/// ```text
/// RFM_ADDONS_DIR="$HOME/Documents/My Games/ArmaReforger/addons" \
///   cargo test -p rfm-core --test live_api -- --ignored readiness_against_a_real
/// ```
#[tokio::test]
#[ignore = "hits the live reforgermods.net API and needs RFM_ADDONS_DIR"]
async fn readiness_against_a_real_addon_store() {
    use rfm_core::modscan::{AddonInventory, FilesystemInventory};
    use rfm_core::readiness;

    let Some(dir) = std::env::var_os("RFM_ADDONS_DIR") else {
        eprintln!("RFM_ADDONS_DIR is not set; skipping");
        return;
    };
    let inventory = FilesystemInventory::new(dir);
    let installed = inventory.installed().expect("addon store is readable");
    assert!(!installed.is_empty(), "no addons found");
    assert!(
        installed.iter().all(|a| a.id.len() == 16),
        "every detected id must be a 16-hex Workshop id"
    );
    println!("local store: {} addons", installed.len());

    let client = client();
    let page = client
        .servers(&ServerQuery {
            has_mods: Some(true),
            per_page: 4,
            ..Default::default()
        })
        .await
        .expect("list request");

    for summary in &page.data {
        let id = ServerId::parse(&summary.id).expect("valid id");
        let Ok(detail) = client.server(&id).await else {
            continue;
        };
        let detail = detail.server;
        let report = readiness::report(&detail.mods, &inventory);

        assert!(report.available, "a readable store must produce a report");
        assert_eq!(report.required, detail.mods.len() as u32);
        // The three buckets must account for every required mod exactly once.
        assert_eq!(
            report.up_to_date + report.outdated + report.missing,
            report.required,
            "counts do not partition the mod list"
        );
        assert_eq!(report.installed_total, installed.len() as u32);

        println!(
            "{:.46} req={:3} ok={:3} old={:2} miss={:3} >= {:.2} GiB",
            detail.summary.name,
            report.required,
            report.up_to_date,
            report.outdated,
            report.missing,
            report.missing_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        );
    }
}

#[tokio::test]
#[ignore = "sends ICMP echo requests to live game servers"]
async fn ping_measures_real_servers() {
    use rfm_core::ping::{self, PingUnavailable};
    use std::time::Duration;

    let client = client();
    let page = client
        .servers(&ServerQuery {
            per_page: 6,
            ..Default::default()
        })
        .await
        .expect("list request");

    let mut measured = 0;
    let mut no_host = 0;
    let mut probed = 0;

    for summary in &page.data {
        let id = ServerId::parse(&summary.id).expect("valid id");
        // A rate-limited or transient detail fetch is not a contract failure;
        // skip it rather than failing the measurement check.
        let Ok(detail) = client.server(&id).await else {
            continue;
        };
        let detail = detail.server;
        let host = detail.host.as_deref();
        if host.is_none() {
            no_host += 1;
            continue;
        }
        probed += 1;

        let result = ping::measure(host, 3, Duration::from_millis(1500)).await;
        match (&result.summary, result.unavailable) {
            (Some(s), None) => {
                measured += 1;
                // A plausible internet round trip, not a middlebox artefact.
                assert!(s.best_ms >= 1 && s.best_ms < 2000, "{s:?}");
                assert!(s.median_ms >= s.best_ms, "median below best: {s:?}");
                assert!(s.received > 0 && s.received <= s.sent);
                println!(
                    "  {:.40} {:>12} {:>4} ms (best {} ms, {}% loss)",
                    detail.summary.name,
                    detail.summary.ping_site_id.clone().unwrap_or_default(),
                    s.median_ms,
                    s.best_ms,
                    s.loss_percent()
                );
            }
            (None, Some(reason)) => {
                assert_eq!(
                    reason,
                    PingUnavailable::NoReply,
                    "a published literal IP should only fail by not replying"
                );
                println!("  {:.40} no reply", detail.summary.name);
            }
            other => panic!("a result must be one or the other: {other:?}"),
        }
    }

    println!("hosts published: {probed}, unmeasurable: {no_host}, answered: {measured}");
    assert!(probed > 0, "no server published a host");
    assert!(
        measured > 0,
        "no server answered ICMP; the transport is not working"
    );
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn locally_built_join_uri_matches_the_api() {
    // The API publishes `joinUrl` for each server. The launcher still builds its
    // own from the validated id — an API-supplied URL must never reach the OS —
    // but the two must agree, so this pins the format against the source of
    // truth rather than against a single hardcoded vector.
    let page = client()
        .servers(&ServerQuery {
            per_page: 20,
            ..Default::default()
        })
        .await
        .expect("list request");

    let mut compared = 0;
    for server in &page.data {
        let Some(published) = server.join_url.as_deref() else {
            continue;
        };
        let id = ServerId::parse(&server.id).expect("launchable id");
        assert_eq!(
            steam_join_uri(&id),
            published,
            "locally built URI diverged for {}",
            server.name
        );
        compared += 1;
    }
    assert!(
        compared > 0,
        "no server published a joinUrl to compare against"
    );
    println!("join URI matches the API for {compared} servers");
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn fleet_population_is_reported() {
    let fleet = client().fleet_now().await.expect("fleet request").now;
    assert!(fleet.players > 0, "no players reported");
    assert!(fleet.online_servers > 0);
    // Capacity is the sum of slots across listed servers, so it bounds players.
    assert!(
        fleet.players <= fleet.capacity,
        "players {} exceed capacity {}",
        fleet.players,
        fleet.capacity
    );
    println!(
        "{} players across {} servers (capacity {}, queue {})",
        fleet.players, fleet.online_servers, fleet.capacity, fleet.queue
    );
}

#[tokio::test]
#[ignore = "hits the live reforgermods.net API"]
async fn official_servers_are_joinable() {
    // Official ids are not UUIDs; rejecting them would make 364 servers
    // unjoinable, and the failure is invisible until someone clicks Join.
    let page = client()
        .servers(&ServerQuery {
            official: Some(true),
            per_page: 10,
            ..Default::default()
        })
        .await
        .expect("official list");
    assert!(!page.data.is_empty());
    for server in &page.data {
        let id = ServerId::parse(&server.id)
            .unwrap_or_else(|e| panic!("official id {:?} not joinable: {e}", server.id));
        assert!(id.is_official_form(), "{}", server.id);
        let uri = steam_join_uri(&id);
        assert!(uri.starts_with("steam://run/1874880//"));
    }
    println!("{} official servers all produce join URIs", page.data.len());
}
