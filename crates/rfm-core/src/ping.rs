//! Direct ICMP latency measurement to game servers.
//!
//! A browser cannot do this; a native launcher can, which is one of the reasons
//! for building one. Measured against 20 live servers: `host` is present on
//! roughly two thirds of servers, is always a literal IP address, and 65% of
//! those answer ICMP echo with region-coherent latencies.
//!
//! # Why not TCP connect timing
//!
//! It was tried and it lies. Connecting to a Reforger server's UDP game port
//! "succeeded" in 12–18 ms — as did port 9 and port 47123, which are closed.
//! Some middlebox completes every handshake, so TCP timing produces
//! confident-looking numbers that are pure fiction. ICMP to the same hosts
//! returned 68–183 ms, coherent with their regions. Only ICMP is used.
//!
//! # Privileges
//!
//! Windows uses `IcmpSendEcho` through the IP Helper API, which needs no
//! elevation. Unix uses an unprivileged ICMP datagram socket, which works when
//! `net.ipv4.ping_group_range` covers the user's group — the default on current
//! distributions — and otherwise needs `CAP_NET_RAW`. A failure to send is
//! reported as "no reply", never as a fabricated number.

use std::net::IpAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Echo requests sent per measurement.
pub const DEFAULT_ATTEMPTS: usize = 3;
/// Per-request deadline.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(1500);

/// The outcome of measuring one host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingSummary {
    /// Fastest round trip observed, milliseconds.
    pub best_ms: u32,
    /// Median round trip, milliseconds. Preferred for display: one delayed
    /// packet should not change the figure the user reads.
    pub median_ms: u32,
    /// Replies received.
    pub received: u32,
    /// Requests sent.
    pub sent: u32,
}

impl PingSummary {
    /// Percentage of requests that went unanswered, 0–100.
    pub fn loss_percent(&self) -> u32 {
        if self.sent == 0 {
            return 0;
        }
        ((self.sent - self.received) * 100) / self.sent
    }
}

/// Why a host could not be measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PingUnavailable {
    /// The API did not report a host for this server.
    NoHost,
    /// The reported host was not a public literal IP address.
    UnroutableHost,
    /// Every echo request went unanswered. Common: many hosts filter ICMP.
    NoReply,
    /// The platform refused to send. On Unix this usually means ICMP sockets
    /// are not permitted for this user.
    NotPermitted,
}

impl PingUnavailable {
    /// Display text, written to be shown as-is.
    pub fn message(self) -> &'static str {
        match self {
            PingUnavailable::NoHost => "This server does not publish an address to measure.",
            PingUnavailable::UnroutableHost => {
                "This server's published address cannot be measured."
            }
            PingUnavailable::NoReply => "No reply — this server does not answer ping.",
            PingUnavailable::NotPermitted => {
                "This system does not permit the launcher to send ping requests."
            }
        }
    }
}

/// A measurement result for the UI: a figure, or a stated reason there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<PingSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<PingUnavailable>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl PingResult {
    /// A successful measurement.
    pub fn measured(summary: PingSummary) -> Self {
        Self {
            summary: Some(summary),
            unavailable: None,
            message: None,
        }
    }

    /// A stated absence of a measurement.
    pub fn unavailable(reason: PingUnavailable) -> Self {
        Self {
            summary: None,
            unavailable: Some(reason),
            message: Some(reason.message().to_string()),
        }
    }
}

/// Resolves a ping-site address into a pingable public IP.
///
/// Ping sites are published as hostnames (`ping-location-de.nitrado.net`),
/// unlike servers, which publish literal IPs. Resolving remote-supplied names
/// is a wider door than [`parse_target`] allows, so the result is put through
/// the same public-address check: a name that resolves into loopback, private
/// or link-local space is refused exactly as a literal would be.
pub async fn resolve_public(address: &str) -> Result<IpAddr, PingUnavailable> {
    let address = address.trim();
    if address.is_empty() {
        return Err(PingUnavailable::NoHost);
    }
    // A literal needs no lookup.
    if let Ok(ip) = address.parse::<IpAddr>() {
        return if is_public(&ip) {
            Ok(ip)
        } else {
            Err(PingUnavailable::UnroutableHost)
        };
    }
    // Reject anything that is not a plain hostname before it reaches a resolver.
    if !address
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return Err(PingUnavailable::UnroutableHost);
    }

    // `lookup_host` wants a port; 0 is fine, nothing connects.
    let resolved = tokio::net::lookup_host((address, 0))
        .await
        .map_err(|_| PingUnavailable::UnroutableHost)?;
    resolved
        .map(|addr| addr.ip())
        .find(is_public)
        .ok_or(PingUnavailable::UnroutableHost)
}

/// Measures a ping-site address, resolving it first.
pub async fn measure_address(address: &str, attempts: usize, timeout: Duration) -> PingResult {
    let ip = match resolve_public(address).await {
        Ok(ip) => ip,
        Err(reason) => return PingResult::unavailable(reason),
    };
    match platform::probe(ip, attempts.max(1), timeout).await {
        Ok(samples) => match summarise(&samples) {
            Some(summary) => PingResult::measured(summary),
            None => PingResult::unavailable(PingUnavailable::NoReply),
        },
        Err(reason) => PingResult::unavailable(reason),
    }
}

/// Validates a host string from the API into something safe to probe.
///
/// This is the security boundary for pinging. API-supplied text must not be
/// able to make the launcher emit packets anywhere it likes, so:
///
/// - Only literal IP addresses are accepted. A hostname would mean a DNS lookup
///   chosen by remote data; every observed server publishes a literal IP anyway
///   (20 of 20 sampled).
/// - Loopback, private, link-local, multicast, broadcast and unspecified
///   addresses are refused, so a hostile or broken record cannot point the
///   launcher at the user's own machine or LAN.
pub fn parse_target(host: Option<&str>) -> Result<IpAddr, PingUnavailable> {
    let host = host.map(str::trim).filter(|h| !h.is_empty());
    let Some(host) = host else {
        return Err(PingUnavailable::NoHost);
    };
    let ip: IpAddr = host.parse().map_err(|_| PingUnavailable::UnroutableHost)?;
    if is_public(&ip) {
        Ok(ip)
    } else {
        Err(PingUnavailable::UnroutableHost)
    }
}

/// True for addresses it is reasonable to send an echo request to.
fn is_public(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_documentation()
                // 100.64.0.0/10, carrier-grade NAT.
                || (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
                // 0.0.0.0/8 and 240.0.0.0/4.
                || v4.octets()[0] == 0
                || v4.octets()[0] >= 240)
        }
        IpAddr::V6(v6) => {
            !(v6.is_loopback()
                || v6.is_multicast()
                || v6.is_unspecified()
                // fe80::/10 link-local and fc00::/7 unique-local.
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || (v6.segments()[0] & 0xfe00) == 0xfc00)
        }
    }
}

/// Reduces raw samples to the figure shown to the user.
///
/// `None` entries are unanswered requests. Returns `None` when nothing came
/// back at all, so the caller states "no reply" instead of rendering a zero.
pub fn summarise(samples: &[Option<Duration>]) -> Option<PingSummary> {
    let mut received: Vec<u32> = samples
        .iter()
        .flatten()
        .map(|d| {
            // Round to the nearest millisecond, with a floor of 1: a sub-
            // millisecond reply is real, and "0 ms" reads as a bug.
            let ms = d.as_secs_f64() * 1000.0;
            (ms.round() as u32).max(1)
        })
        .collect();
    if received.is_empty() {
        return None;
    }
    received.sort_unstable();

    let median = if received.len() % 2 == 1 {
        received[received.len() / 2]
    } else {
        let hi = received.len() / 2;
        // Even counts average the middle pair. Written as an offset from the
        // lower value rather than `(a + b) / 2` so a large pair cannot overflow,
        // and without `u32::midpoint`, which postdates this crate's MSRV.
        received[hi - 1] + (received[hi] - received[hi - 1]) / 2
    };

    Some(PingSummary {
        best_ms: received[0],
        median_ms: median,
        received: received.len() as u32,
        sent: samples.len() as u32,
    })
}

/// Measures `host`, returning either a figure or a stated reason there is none.
pub async fn measure(host: Option<&str>, attempts: usize, timeout: Duration) -> PingResult {
    let ip = match parse_target(host) {
        Ok(ip) => ip,
        Err(reason) => return PingResult::unavailable(reason),
    };
    match platform::probe(ip, attempts.max(1), timeout).await {
        Ok(samples) => match summarise(&samples) {
            Some(summary) => PingResult::measured(summary),
            None => PingResult::unavailable(PingUnavailable::NoReply),
        },
        Err(reason) => PingResult::unavailable(reason),
    }
}

#[cfg(unix)]
mod platform {
    use super::*;

    /// Unprivileged ICMP datagram socket. Requires `net.ipv4.ping_group_range`
    /// to cover the user's group, which is the default on current distributions.
    pub async fn probe(
        ip: IpAddr,
        attempts: usize,
        timeout: Duration,
    ) -> Result<Vec<Option<Duration>>, PingUnavailable> {
        use surge_ping::{Client, Config, PingIdentifier, PingSequence, ICMP};

        let config = match ip {
            IpAddr::V4(_) => Config::default(),
            IpAddr::V6(_) => Config::builder().kind(ICMP::V6).build(),
        };
        let client = Client::new(&config).map_err(|_| PingUnavailable::NotPermitted)?;
        // The identifier only has to be unique among this process's in-flight
        // pings; the low bits of the clock are sufficient and cheap.
        let identifier = PingIdentifier(std::process::id() as u16);
        let mut pinger = client.pinger(ip, identifier).await;
        pinger.timeout(timeout);

        let payload = [0u8; 32];
        let mut samples = Vec::with_capacity(attempts);
        for seq in 0..attempts {
            let reply = pinger.ping(PingSequence(seq as u16), &payload).await;
            samples.push(reply.ok().map(|(_, rtt)| rtt));
        }
        Ok(samples)
    }
}

#[cfg(windows)]
mod platform {
    use super::*;

    /// `IcmpSendEcho` through the IP Helper API, which needs no elevation.
    /// It is blocking, so it runs on the blocking pool.
    pub async fn probe(
        ip: IpAddr,
        attempts: usize,
        timeout: Duration,
    ) -> Result<Vec<Option<Duration>>, PingUnavailable> {
        let timeout_ms = timeout.as_millis().min(u32::MAX as u128) as u32;
        tokio::task::spawn_blocking(move || {
            let mut pinger = winping::Pinger::new().map_err(|_| PingUnavailable::NotPermitted)?;
            pinger.set_timeout(timeout_ms);
            let mut buffer = winping::Buffer::new();
            let mut samples = Vec::with_capacity(attempts);
            for _ in 0..attempts {
                // `send` reports the round trip in whole milliseconds; a
                // timeout or filtered host comes back as an error, which is a
                // missing sample rather than a failure of the measurement.
                let rtt = pinger
                    .send(ip, &mut buffer)
                    .ok()
                    .map(|ms| Duration::from_millis(u64::from(ms)));
                samples.push(rtt);
            }
            Ok(samples)
        })
        .await
        .map_err(|_| PingUnavailable::NotPermitted)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Option<Duration> {
        Some(Duration::from_millis(value))
    }

    #[test]
    fn real_server_addresses_are_accepted() {
        // Literal IPs observed on live servers.
        for host in ["208.92.232.141", "103.193.80.16", "69.67.175.16"] {
            assert!(parse_target(Some(host)).is_ok(), "{host}");
        }
    }

    #[test]
    fn a_missing_host_is_distinguished_from_a_bad_one() {
        assert_eq!(parse_target(None), Err(PingUnavailable::NoHost));
        assert_eq!(parse_target(Some("")), Err(PingUnavailable::NoHost));
        assert_eq!(parse_target(Some("   ")), Err(PingUnavailable::NoHost));
    }

    #[test]
    fn hostnames_are_refused_so_api_data_cannot_drive_dns() {
        for host in [
            "example.com",
            "localhost",
            "evil.attacker.test",
            "ping-location-de.nitrado.net",
        ] {
            assert_eq!(
                parse_target(Some(host)),
                Err(PingUnavailable::UnroutableHost),
                "{host}"
            );
        }
    }

    #[test]
    fn private_and_loopback_addresses_are_refused() {
        // A hostile or broken record must not aim the launcher at the user's
        // own machine or LAN.
        for host in [
            "127.0.0.1",
            "0.0.0.0",
            "10.0.0.1",
            "172.16.4.9",
            "192.168.1.1",
            "169.254.1.1",
            "224.0.0.1",
            "255.255.255.255",
            "100.64.0.1",
            "240.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fd00::1",
            "ff02::1",
        ] {
            assert_eq!(
                parse_target(Some(host)),
                Err(PingUnavailable::UnroutableHost),
                "{host} must be refused"
            );
        }
    }

    #[test]
    fn public_ipv6_is_accepted() {
        assert!(parse_target(Some("2606:4700:4700::1111")).is_ok());
    }

    #[test]
    fn garbage_hosts_are_refused() {
        for host in [
            "208.92.232.141; rm -rf /",
            "208.92.232.141:2001",
            "not an ip",
            "999.999.999.999",
            "../../etc/passwd",
        ] {
            assert_eq!(
                parse_target(Some(host)),
                Err(PingUnavailable::UnroutableHost),
                "{host}"
            );
        }
    }

    #[test]
    fn summarise_reports_best_and_median() {
        let summary = summarise(&[ms(80), ms(70), ms(120)]).unwrap();
        assert_eq!(summary.best_ms, 70);
        assert_eq!(summary.median_ms, 80);
        assert_eq!(summary.received, 3);
        assert_eq!(summary.sent, 3);
        assert_eq!(summary.loss_percent(), 0);
    }

    #[test]
    fn the_median_resists_one_delayed_packet() {
        // A single 900 ms outlier must not become the displayed figure.
        let summary = summarise(&[ms(70), ms(72), ms(900)]).unwrap();
        assert_eq!(summary.median_ms, 72);
        assert_eq!(summary.best_ms, 70);
    }

    #[test]
    fn partial_loss_is_summarised_not_discarded() {
        let summary = summarise(&[ms(80), None, ms(100)]).unwrap();
        assert_eq!(summary.received, 2);
        assert_eq!(summary.sent, 3);
        assert_eq!(summary.loss_percent(), 33);
        assert_eq!(summary.best_ms, 80);
    }

    #[test]
    fn total_loss_yields_no_figure_rather_than_zero() {
        assert!(summarise(&[None, None, None]).is_none());
        assert!(summarise(&[]).is_none());
    }

    #[test]
    fn even_sample_counts_average_the_middle_pair() {
        let summary = summarise(&[ms(70), ms(80), ms(90), ms(100)]).unwrap();
        assert_eq!(summary.median_ms, 85);
    }

    #[test]
    fn sub_millisecond_replies_floor_at_one() {
        // A LAN-speed reply is real; "0 ms" reads as a broken measurement.
        let summary = summarise(&[Some(Duration::from_micros(200))]).unwrap();
        assert_eq!(summary.best_ms, 1);
        assert_eq!(summary.median_ms, 1);
    }

    #[tokio::test]
    async fn an_unmeasurable_host_states_why_without_sending_anything() {
        let result = measure(None, 3, Duration::from_millis(50)).await;
        assert_eq!(result.unavailable, Some(PingUnavailable::NoHost));
        assert!(result.summary.is_none());
        assert!(result.message.is_some());

        let result = measure(Some("192.168.0.1"), 3, Duration::from_millis(50)).await;
        assert_eq!(result.unavailable, Some(PingUnavailable::UnroutableHost));
    }

    #[tokio::test]
    async fn resolve_public_accepts_a_literal_without_a_lookup() {
        assert_eq!(
            resolve_public("208.92.232.141").await,
            Ok("208.92.232.141".parse().unwrap())
        );
    }

    #[tokio::test]
    async fn resolve_public_refuses_names_that_are_not_hostnames() {
        // Nothing resembling a URL, path or command reaches the resolver.
        for address in [
            "",
            "   ",
            "http://example.com",
            "example.com/path",
            "example.com:80",
            "a b",
            "host;rm -rf /",
            "host|cat",
        ] {
            assert!(resolve_public(address).await.is_err(), "{address:?}");
        }
    }

    #[tokio::test]
    async fn resolve_public_refuses_private_literals() {
        for address in ["127.0.0.1", "192.168.1.1", "::1", "10.0.0.5"] {
            assert_eq!(
                resolve_public(address).await,
                Err(PingUnavailable::UnroutableHost),
                "{address}"
            );
        }
    }

    #[tokio::test]
    async fn measure_address_states_why_it_cannot_measure() {
        let result = measure_address("", 1, Duration::from_millis(50)).await;
        assert_eq!(result.unavailable, Some(PingUnavailable::NoHost));
        let result = measure_address("127.0.0.1", 1, Duration::from_millis(50)).await;
        assert_eq!(result.unavailable, Some(PingUnavailable::UnroutableHost));
    }

    #[test]
    fn every_unavailable_reason_has_distinct_copy() {
        let reasons = [
            PingUnavailable::NoHost,
            PingUnavailable::UnroutableHost,
            PingUnavailable::NoReply,
            PingUnavailable::NotPermitted,
        ];
        let messages: std::collections::HashSet<&str> =
            reasons.iter().map(|r| r.message()).collect();
        assert_eq!(messages.len(), reasons.len());
    }

    #[test]
    fn results_serialise_without_null_noise() {
        let json = serde_json::to_value(PingResult::measured(PingSummary {
            best_ms: 70,
            median_ms: 72,
            received: 3,
            sent: 3,
        }))
        .unwrap();
        assert_eq!(json["summary"]["medianMs"], 72);
        assert!(json.get("unavailable").is_none());

        let json = serde_json::to_value(PingResult::unavailable(PingUnavailable::NoReply)).unwrap();
        assert_eq!(json["unavailable"], "no_reply");
        assert!(json.get("summary").is_none());
    }
}
