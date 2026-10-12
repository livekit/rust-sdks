// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Where batches go and with which credential — decided here, once, for every platform.
//!
//! A platform hands over exactly two things: the LiveKit server URL a room connects to and the
//! participant token it connects with (again on every token refresh). The core derives the
//! ingest URL, checks that the host is LiveKit Cloud, reads the token's unverified claims (the
//! observability grant, the expiry), and routes every batch to the project its session belongs
//! to, so two rooms talking to two projects never share a token or a destination.

use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{
    alphabet,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
    Engine,
};
use tokio::time::Instant;

/// The environment variable that points every upload at a collector of your own — a local
/// OpenTelemetry collector for end-to-end tests. Not part of any platform API: set it in the
/// test process's environment before the pipeline starts. Either a base URL
/// (`http://localhost:4318`, OTLP paths `/v1/logs` and `/v1/traces` are appended) or a full logs
/// URL ending in `logs` (its traces URL is derived by replacing that segment).
pub const ENDPOINT_OVERRIDE_ENV: &str = "LK_TELEMETRY_ENDPOINT";

/// The longest a token is believed to stay valid, whatever its `exp` says.
const MAX_TOKEN_LIFETIME: Duration = Duration::from_secs(366 * 24 * 60 * 60);

/// LiveKit Cloud project hosts: `<project>.livekit.cloud` in production,
/// `<project>.staging.livekit.cloud` in staging. Self-hosted servers have no ingest.
const CLOUD_SUFFIX: &str = ".livekit.cloud";

/// Which OTLP signal a batch carries; picks the ingest path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Signal {
    Logs,
    Traces,
}

/// A request target: URL per signal and the bearer token, when one is due.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Target {
    /// The project the batch is sent to (`None` for the test override): what its answer is
    /// attributed to — a 404, a disable, a pause.
    pub project: Option<String>,
    pub logs: String,
    pub traces: String,
    pub token: Option<String>,
}

impl Target {
    pub fn url(&self, signal: Signal) -> &str {
        match signal {
            Signal::Logs => &self.logs,
            Signal::Traces => &self.traces,
        }
    }
}

/// What the unverified claims of a participant token say — enough to not send with a token the
/// collector is known to refuse. The signature is the collector's business.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    raw: String,
    /// `observability.write` or `observability.clientWrite`.
    grant: bool,
    /// From `exp`, on the monotonic clock (so tests can move it).
    expires: Option<Instant>,
    /// The credential's identity (a hash of the raw token): what a refusal is recorded against.
    id: u64,
}

impl Token {
    pub fn parse(raw: &str) -> Self {
        let claims = raw
            .split('.')
            .nth(1)
            .and_then(|payload| {
                const URL_SAFE: GeneralPurpose = GeneralPurpose::new(
                    &alphabet::URL_SAFE,
                    GeneralPurposeConfig::new()
                        .with_decode_padding_mode(DecodePaddingMode::Indifferent),
                );
                URL_SAFE.decode(payload).ok()
            })
            .and_then(|json| serde_json::from_slice::<serde_json::Value>(&json).ok());
        let grant = claims.as_ref().is_some_and(|c| {
            let observability = &c["observability"];
            observability["write"].as_bool() == Some(true)
                || observability["clientWrite"].as_bool() == Some(true)
        });
        // Untrusted input, read conservatively: a past or negative `exp` is expired, and so is
        // one that is not a number at all; a fractional one counts in whole seconds; one beyond
        // any real token lifetime is clamped (an `Instant` that far out would overflow). Only a
        // token without `exp` has no known expiry.
        let expires = claims.as_ref().and_then(|c| c.get("exp")).map(|exp| {
            let now =
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64();
            let left = match exp.as_f64() {
                Some(exp) if exp.is_finite() && exp > now => {
                    Duration::from_secs_f64((exp - now).min(MAX_TOKEN_LIFETIME.as_secs_f64()))
                }
                _ => Duration::ZERO,
            };
            Instant::now().checked_add(left).unwrap_or_else(Instant::now)
        });
        let id = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            raw.hash(&mut hasher);
            hasher.finish()
        };
        Self { raw: raw.to_owned(), grant, expires, id }
    }

    fn expired(&self) -> bool {
        self.expires.is_some_and(|at| Instant::now() >= at)
    }

    /// May be sent: carries the grant, not expired, not refused.
    fn usable(&self, refused: &Refusals) -> bool {
        self.grant && !self.expired() && !refused.contains(self)
    }
}

/// Why a project receives nothing (anymore). Each is logged once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dead {
    /// Not a LiveKit Cloud host (self-hosted, a typo): there is no ingest to talk to.
    NotCloud,
    /// The project's own token carries no observability grant: the customer did not opt in.
    NoGrant,
    /// The collector answered 404: no client ingest on this host. Revived by the next token
    /// (a new connect or refresh), so a misrouted answer cannot silence a project for good.
    NotFound,
    /// The collector said the project's data recording is disabled by its owner.
    Disabled,
}

/// One LiveKit Cloud project, keyed by its host.
#[derive(Debug, Default)]
struct Project {
    /// The latest token any room handed over for the project: what process-level batches and
    /// batches from a previous launch are sent with.
    latest: Option<Token>,
    /// A token with the grant was seen: the customer opted in. Refreshed tokens may lack the
    /// grant (today's server drops it on refresh) without withdrawing that.
    consented: bool,
    dead: Option<Dead>,
}

/// Offer `new` for a token slot: a refused token is never taken, into any slot; a token with the
/// grant wins; one without it does not replace a usable granted token (today's refreshes drop
/// the grant).
fn offer(slot: &mut Option<Token>, new: &Token, refused: &Refusals) {
    if refused.contains(new) {
        return;
    }
    let keep = slot.as_ref().is_some_and(|t| t.raw == new.raw || (!new.grant && t.usable(refused)));
    if !keep {
        *slot = Some(new.clone());
    }
}

/// Credentials the collector refused, by identity, shared by every slot: a refused token is never
/// sent again, whichever Room or project hands it over. An entry lives until its token expires;
/// tokens without `exp` stay refused for the life of the process.
// ponytail: grows with refusals of tokens without `exp` (LiveKit tokens always carry one); a cap
// if a server ever mints tokens without expiry.
#[derive(Debug, Default)]
struct Refusals(HashMap<u64, Option<Instant>>);

impl Refusals {
    fn contains(&self, token: &Token) -> bool {
        self.0.contains_key(&token.id)
    }

    /// Returns whether the refusal is new.
    fn add(&mut self, token: &Token) -> bool {
        let now = Instant::now();
        self.0.retain(|_, expires| expires.is_none_or(|at| at > now));
        self.0.insert(token.id, token.expires).is_none()
    }
}

/// The owner of process-level records (device state, pre-room errors, self-telemetry): no Room,
/// so they go to the latest project with its latest token.
pub(crate) const PROCESS_OWNER: &str = "process";

/// Where a batch is headed.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Route {
    /// Ready: send now.
    Send(Target),
    /// Keep it cached: no destination yet, or no usable token (missing, expired, grant-less,
    /// refused) until the platform hands over a fresh one. A hard hold: no escape hatch sends
    /// it anyway.
    Wait,
    /// Drop it: its project receives nothing.
    Drop,
}

/// Every project and session this process talks to, plus the test override. Tokens live here,
/// in memory only: never persisted, never in a batch.
#[derive(Debug, Default)]
pub(crate) struct Destinations {
    projects: HashMap<String, Project>,
    /// Each Room session's own latest token per project, by (host, session id): its batches for
    /// that project are sent with it and never with another Room's, nor with the token it later
    /// got for another project.
    sessions: HashMap<(String, String), Token>,
    /// Each Room session's first project: where the records it captured before it had a server
    /// go. A Room's records never fall back to another Room's project.
    rooms: HashMap<String, String>,
    refused: Refusals,
    /// The host process-level batches (device state, pre-room logs, self-telemetry) go to: the
    /// project most recently handed a token, while it is alive.
    latest: Option<String>,
    /// [`ENDPOINT_OVERRIDE_ENV`]: every batch goes there, no Cloud rules, no token.
    endpoint_override: Option<Target>,
}

impl Destinations {
    pub fn new(endpoint_override: Option<&str>) -> Self {
        Self {
            endpoint_override: endpoint_override.filter(|e| !e.is_empty()).map(override_target),
            ..Self::default()
        }
    }

    /// The first project a Room session was routed to: where what it captured before it had a
    /// server belongs.
    pub fn first_project(&self, owner: &str) -> Option<String> {
        self.rooms.get(owner).cloned()
    }

    pub fn has_override(&self) -> bool {
        self.endpoint_override.is_some()
    }

    /// Session `owner`'s server URL and token (at connect, and again on every refresh). Returns
    /// the project host its batches carry, or `None` when the URL has no host.
    pub fn set(&mut self, url: &str, token: &str, owner: &str) -> Option<String> {
        let Server { key: host, cloud } = Server::parse(url)?;
        if self.endpoint_override.is_some() {
            return Some(host);
        }
        let project = self.projects.entry(host.clone()).or_default();
        if project.dead.is_none() && !cloud {
            project.dead = Some(Dead::NotCloud);
            log::warn!("{host} is not LiveKit Cloud: client telemetry stays on this device");
        }
        if matches!(project.dead, Some(Dead::NotCloud | Dead::Disabled)) {
            return Some(host);
        }
        let token = Token::parse(token);
        if project.dead == Some(Dead::NotFound) {
            log::debug!("new token for {host}: trying its ingest again");
            project.dead = None;
        }
        if token.grant {
            project.consented = true;
            if project.dead == Some(Dead::NoGrant) {
                project.dead = None;
            }
        } else if !project.consented && project.dead.is_none() {
            project.dead = Some(Dead::NoGrant);
            log::info!("the token for {host} has no observability grant: telemetry stays local");
        }
        offer(&mut project.latest, &token, &self.refused);
        let key = (host.clone(), owner.to_owned());
        let mut own = self.sessions.remove(&key);
        offer(&mut own, &token, &self.refused);
        self.sessions.extend(own.map(|own| (key, own)));
        self.rooms.entry(owner.to_owned()).or_insert_with(|| host.clone());
        self.latest = Some(host.clone());
        Some(host)
    }

    /// Where a batch goes: `host` is its project as captured with its records, `owner` the
    /// session that produced them. A Room known to this process sends with its own latest token
    /// for that project and waits when it has none; process-level batches and a previous
    /// launch's use the project's latest. A Room's records captured before it had a server go
    /// to its first project, never to another Room's.
    pub fn route(&self, host: Option<&str>, owner: Option<&str>) -> Route {
        if let Some(target) = &self.endpoint_override {
            return Route::Send(target.clone());
        }
        let room = owner.filter(|o| *o != PROCESS_OWNER);
        let host = match (host, room) {
            (Some(host), _) => host,
            (None, Some(room)) => match self.rooms.get(room) {
                Some(first) => first.as_str(),
                None => return Route::Wait,
            },
            (None, None) => match self.fallback() {
                Some(host) => host,
                // Nothing to talk to yet: wait. Only dead projects so far: nobody will ever
                // take process-level data, so it is not kept either.
                None if self.all_dead() => return Route::Drop,
                None => return Route::Wait,
            },
        };
        let Some(project) = self.projects.get(host) else { return Route::Wait };
        if project.dead.is_some() {
            return Route::Drop;
        }
        let token = match room {
            Some(room) if self.rooms.contains_key(room) => {
                self.sessions.get(&(host.to_owned(), room.to_owned()))
            }
            _ => project.latest.as_ref(),
        };
        match token {
            Some(token) if token.usable(&self.refused) => Route::Send(Target {
                project: Some(host.to_owned()),
                logs: ingest_url(host, Signal::Logs),
                traces: ingest_url(host, Signal::Traces),
                token: Some(token.raw.clone()),
            }),
            _ => Route::Wait,
        }
    }

    /// The host of the project process-level batches go to: the latest, else any alive one.
    fn fallback(&self) -> Option<&str> {
        let alive = |host: &&String| self.projects.get(*host).is_some_and(|p| p.dead.is_none());
        self.latest
            .as_ref()
            .filter(alive)
            .or_else(|| self.projects.keys().find(alive))
            .map(String::as_str)
    }

    fn all_dead(&self) -> bool {
        !self.projects.is_empty() && self.projects.values().all(|p| p.dead.is_some())
    }

    /// Whether a session routed to `host` should still collect: `false` once its project is
    /// known to receive nothing, so its records are dropped at the door instead of cached.
    pub fn alive(&self, host: Option<&str>) -> bool {
        self.endpoint_override.is_some() || !matches!(self.route(host, None), Route::Drop)
    }

    /// The collector refused `token` (401/403): it is not sent again, from any slot; batches that
    /// would use it wait for the next one.
    pub fn refuse(&mut self, token: &str) {
        if self.refused.add(&Token::parse(token)) {
            log::warn!("the collector refused a token; waiting for a new one");
        }
    }

    /// Keep credentials only where something still needs them. `live` maps every session still
    /// alive — held by its Room, or by records, windows or spans not yet cached — to its current
    /// project; its credentials stay (for each project it was routed to: records it captured for
    /// an earlier project may still be queued). `backlog` is (project or none, owner) of every
    /// cached batch: their credentials stay too. Project-level copies stay while a live session
    /// is routed there or the backlog has batches for the project (process-level batches: the
    /// latest project).
    pub fn retain_owners(
        &mut self,
        live: &HashMap<String, Option<String>>,
        backlog: &std::collections::HashSet<(Option<String>, String)>,
    ) {
        // A Room's batch captured before it had a server is cached without a project: it goes
        // to that Room's first project, so it backs that project's credential.
        let backlog: std::collections::HashSet<(Option<String>, String)> = backlog
            .iter()
            .map(|(host, owner)| match (host, self.rooms.get(owner)) {
                (None, Some(first)) if owner != PROCESS_OWNER => {
                    (Some(first.clone()), owner.clone())
                }
                _ => (host.clone(), owner.clone()),
            })
            .collect();
        let backed =
            |host: &str, owner: &str| backlog.contains(&(Some(host.to_owned()), owner.to_owned()));
        self.sessions.retain(|(host, owner), _| backed(host, owner) || live.contains_key(owner));
        self.rooms
            .retain(|owner, _| live.contains_key(owner) || backlog.iter().any(|(_, o)| o == owner));
        let unrouted = backlog.iter().any(|(host, _)| host.is_none());
        let latest = self.latest.clone();
        for (host, project) in &mut self.projects {
            let needed = live.values().any(|route| route.as_deref() == Some(host))
                || backlog.iter().any(|(h, _)| h.as_deref() == Some(host))
                || (unrouted && latest.as_deref() == Some(host));
            if !needed {
                project.latest = None;
            }
        }
    }

    /// How many credentials are held, Room and project copies together.
    #[cfg(test)]
    pub fn credentials(&self) -> usize {
        self.sessions.len() + self.projects.values().filter(|p| p.latest.is_some()).count()
    }

    /// Try `host`'s ingest again if it answered 404 (a reconnect). Returns whether it did.
    pub fn revive(&mut self, host: &str) -> bool {
        match self.projects.get_mut(host) {
            Some(project) if project.dead == Some(Dead::NotFound) => {
                project.dead = None;
                true
            }
            _ => false,
        }
    }

    /// The project on `host` receives nothing (see [`Dead`] for how long).
    pub fn kill(&mut self, host: &str, why: Dead) {
        let project = self.projects.entry(host.to_owned()).or_default();
        if project.dead.is_none() {
            match why {
                Dead::NotFound => log::warn!("{host} has no client telemetry ingest; going silent"),
                Dead::Disabled => log::warn!("{host} disabled data recording; going silent"),
                Dead::NotCloud | Dead::NoGrant => {}
            }
            project.dead = Some(why);
        }
    }
}

/// A server URL, parsed (WHATWG URL rules, the same a browser or `URLSession` applies): its
/// routing key — the canonical host, with the port when one is given — and whether it is a
/// LiveKit Cloud project a token may be sent to.
struct Server {
    key: String,
    cloud: bool,
}

impl Server {
    fn parse(url: &str) -> Option<Self> {
        let url = url::Url::parse(url).ok()?;
        let host = url.host_str().filter(|h| !h.is_empty())?.to_ascii_lowercase();
        let key = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.clone(),
        };
        // Credentials only ever go to `https://<project>.livekit.cloud/…`, built from the
        // validated host alone: a TLS scheme, a domain under the Cloud suffix with a label of its
        // own, the default port, no userinfo. Anything else — a look-alike that only a string
        // check would accept (`evil.example\.livekit.cloud` is host `evil.example`), a
        // plaintext scheme, an odd port — is not Cloud.
        let cloud = matches!(url.scheme(), "wss" | "https")
            && matches!(url.host(), Some(url::Host::Domain(_)))
            && host.strip_suffix(CLOUD_SUFFIX).is_some_and(|label| !label.is_empty())
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none();
        Some(Self { key, cloud })
    }
}

/// `https://<host>/observability/client/{logs,traces}/otlp/v0`: the client table set.
fn ingest_url(host: &str, signal: Signal) -> String {
    let signal = match signal {
        Signal::Logs => "logs",
        Signal::Traces => "traces",
    };
    format!("https://{host}/observability/client/{signal}/otlp/v0")
}

fn override_target(endpoint: &str) -> Target {
    let endpoint = endpoint.trim_end_matches('/');
    let (logs, traces) = match endpoint.rsplit_once("logs") {
        Some((before, after)) => (endpoint.to_owned(), format!("{before}traces{after}")),
        None => (format!("{endpoint}/v1/logs"), format!("{endpoint}/v1/traces")),
    };
    Target { project: None, logs, traces, token: None }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An unsigned JWT with these claims — enough for the core, which never verifies.
    pub(crate) fn jwt(claims: serde_json::Value) -> String {
        let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        format!(
            "{}.{}.sig",
            engine.encode(br#"{"alg":"HS256"}"#),
            engine.encode(claims.to_string())
        )
    }

    pub(crate) fn granted(ttl_secs: u64) -> String {
        let exp = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_secs() + ttl_secs;
        jwt(serde_json::json!({ "exp": exp, "observability": { "write": true } }))
    }

    pub(crate) fn grantless(ttl_secs: u64) -> String {
        let exp = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_secs() + ttl_secs;
        jwt(serde_json::json!({ "exp": exp, "video": { "roomJoin": true } }))
    }

    fn send(route: Route) -> Target {
        match route {
            Route::Send(target) => target,
            other => panic!("expected a target, got {other:?}"),
        }
    }

    #[test]
    fn only_livekit_cloud_hosts_get_an_ingest_url() {
        let mut d = Destinations::default();
        let token = granted(600);
        let host = d.set("wss://Proj.LiveKit.Cloud/rtc?access_token=x#f", &token, "s");
        assert_eq!(host.as_deref(), Some("proj.livekit.cloud"));
        let target = send(d.route(host.as_deref(), Some("s")));
        assert_eq!(target.logs, "https://proj.livekit.cloud/observability/client/logs/otlp/v0");
        assert_eq!(target.traces, "https://proj.livekit.cloud/observability/client/traces/otlp/v0");
        assert_eq!(target.token.as_deref(), Some(token.as_str()));

        let staging = d.set("https://p.staging.livekit.cloud", &token, "s");
        assert!(matches!(d.route(staging.as_deref(), None), Route::Send(_)), "staging is Cloud");

        for url in
            ["ws://localhost:7880", "wss://livekit.example.com", "wss://livekit.cloud.evil.io"]
        {
            let host = d.set(url, &token, "s");
            assert_eq!(d.route(host.as_deref(), None), Route::Drop, "{url}: no ingest");
        }
        assert_eq!(d.set("nonsense", &token, "s"), None);
        let empty = d.set("wss:///rtc", &token, "s"); // the parser reads host `rtc`
        assert_eq!(d.route(empty.as_deref(), None), Route::Drop);
    }

    /// Finding r1-1: whatever a string check would accept, the token only goes to a host the URL
    /// parser agrees is `<project>.livekit.cloud`, over TLS, on the default port, without
    /// userinfo — and the endpoint is built from that host alone.
    #[test]
    fn look_alike_server_urls_never_get_the_token() {
        let token = granted(600);
        for url in [
            "wss://evil.example\\.livekit.cloud",
            "wss://evil.example\\x.livekit.cloud/rtc",
            "wss://user:pass@p.livekit.cloud",
            "wss://evil.example@p.livekit.cloud",
            "wss://p.livekit.cloud:8443",
            "ws://p.livekit.cloud",
            "http://p.livekit.cloud",
            "wss://livekit.cloud",
            "wss://.livekit.cloud",
            "wss://p.livekit.cloud.evil.io",
            "wss://p.livekit.cloud%2eevil.io",
            "wss://evil.io#.livekit.cloud",
            "wss://evil.io?.livekit.cloud",
            "wss://[::1]",
            "wss://1.2.3.4",
            "wss://p.livekit.cloud\u{0}.evil",
        ] {
            let mut d = Destinations::default();
            let host = d.set(url, &token, "s");
            let route = d.route(host.as_deref(), Some("s"));
            assert!(!matches!(route, Route::Send(_)), "{url} → {host:?} must not get the token");
        }
        let mut d = Destinations::default();
        let host = d.set("WSS://P.LiveKit.Cloud:443/rtc?x=1#y", &token, "s");
        let target = send(d.route(host.as_deref(), Some("s")));
        assert_eq!(target.logs, "https://p.livekit.cloud/observability/client/logs/otlp/v0");
    }

    #[test]
    fn a_token_without_the_grant_is_not_consent() {
        let mut d = Destinations::default();
        let host = d.set("wss://p.livekit.cloud", &grantless(600), "s");
        assert_eq!(d.route(host.as_deref(), Some("s")), Route::Drop, "never opted in");
        assert_eq!(d.route(None, None), Route::Drop, "and no project takes process data");
        let host = d.set("wss://p.livekit.cloud", &granted(600), "s");
        assert!(matches!(d.route(host.as_deref(), Some("s")), Route::Send(_)), "a grant opts in");
    }

    #[tokio::test(start_paused = true)]
    async fn a_refresh_that_drops_the_grant_keeps_the_granted_token_until_it_expires() {
        let mut d = Destinations::default();
        let join = granted(60);
        let host = d.set("wss://p.livekit.cloud", &join, "s");
        d.set("wss://p.livekit.cloud", &grantless(600), "s");
        assert_eq!(send(d.route(host.as_deref(), Some("s"))).token, Some(join), "still granted");
        tokio::time::advance(Duration::from_secs(61)).await;
        assert_eq!(d.route(host.as_deref(), Some("s")), Route::Wait, "expired: hold, never drop");
        let refreshed = granted(600);
        d.set("wss://p.livekit.cloud", &refreshed, "s");
        assert_eq!(send(d.route(host.as_deref(), Some("s"))).token, Some(refreshed));
    }

    #[test]
    fn a_refused_token_is_never_sent_again() {
        let mut d = Destinations::default();
        let first = granted(600);
        let host = d.set("wss://p.livekit.cloud", &first, "s").expect("host");
        d.refuse(&first);
        assert_eq!(d.route(Some(&host), Some("s")), Route::Wait);
        d.set("wss://p.livekit.cloud", &first, "s");
        assert_eq!(d.route(Some(&host), Some("s")), Route::Wait, "handing it over again: still no");
        d.set("wss://p.livekit.cloud", &granted(900), "s");
        assert!(matches!(d.route(Some(&host), Some("s")), Route::Send(_)));
    }

    #[test]
    fn rooms_never_borrow_each_others_tokens() {
        let mut d = Destinations::default();
        let (a, b, c) = (granted(600), granted(700), granted(800));
        let host_a = d.set("wss://a.livekit.cloud", &a, "room-a");
        let host_b = d.set("wss://b.livekit.cloud", &b, "room-b");
        d.set("wss://a.livekit.cloud", &c, "room-c"); // a second room on project a
        let target = send(d.route(host_a.as_deref(), Some("room-a")));
        assert!(target.logs.starts_with("https://a.") && target.token == Some(a.clone()));
        let target = send(d.route(host_a.as_deref(), Some("room-c")));
        assert_eq!(target.token, Some(c.clone()), "same project, its own token");
        let target = send(d.route(host_b.as_deref(), Some("room-b")));
        assert!(target.logs.starts_with("https://b.") && target.token == Some(b));
        d.refuse(&a);
        assert_eq!(d.route(host_a.as_deref(), Some("room-a")), Route::Wait, "no borrowing c");
        let previous_launch = send(d.route(host_a.as_deref(), Some("gone")));
        assert_eq!(previous_launch.token, Some(c), "a previous launch's batch: project's latest");
        d.kill("b.livekit.cloud", Dead::Disabled);
        assert!(send(d.route(None, None)).logs.starts_with("https://a."), "process: a live one");
    }

    /// Finding r1-2: a Room's batches keep the project they were captured for — after it
    /// reconnects to another project they still go to the first with the first project's token —
    /// and records it captured before it had a server never go to another Room's project.
    #[test]
    fn ownership_survives_a_room_changing_projects() {
        let mut d = Destinations::default();
        let (a, b) = (granted(600), granted(700));
        let host_a = d.set("wss://a.livekit.cloud", &a, "room").expect("a");
        d.set("wss://b.livekit.cloud", &b, "room");
        let target = send(d.route(Some(&host_a), Some("room")));
        assert!(target.logs.starts_with("https://a.") && target.token == Some(a));

        d.set("wss://c.livekit.cloud", &granted(800), "other");
        assert_eq!(d.route(None, Some("unconnected")), Route::Wait, "no borrowing c");
        let early = send(d.route(None, Some("room")));
        assert!(early.logs.starts_with("https://a."), "a Room's first project");
        assert!(matches!(d.route(None, Some(PROCESS_OWNER)), Route::Send(_)));
    }

    /// Findings r1-13 / r2-4: a refusal belongs to the credential — no slot, Room, project or
    /// process route can make it usable again — and credentials follow the live Rooms and their
    /// backlog, project-level copies included.
    #[test]
    fn refusals_stick_and_credentials_follow_live_rooms() {
        let mut d = Destinations::default();
        let tokens: Vec<String> = (0..40).map(|n| granted(600 + n)).collect();
        for (n, token) in tokens.iter().enumerate() {
            d.set("wss://p.livekit.cloud", token, &format!("room-{n}"));
            d.refuse(token);
        }
        for (n, token) in tokens.iter().enumerate() {
            let room = format!("room-{n}");
            d.set("wss://p.livekit.cloud", token, &room); // handed over again
            assert_eq!(d.route(Some("p.livekit.cloud"), Some(&room)), Route::Wait, "{n}");
            d.set("wss://p.livekit.cloud", token, &format!("new-{n}")); // by a new Room
            assert_eq!(d.route(Some("p.livekit.cloud"), Some(&format!("new-{n}"))), Route::Wait);
            assert_eq!(d.route(None, Some(PROCESS_OWNER)), Route::Wait, "{n}: process route");
            assert_eq!(d.route(Some("p.livekit.cloud"), Some("gone")), Route::Wait, "{n}: restart");
        }
        // Refuse A, let B become the latest, hand A over again: A stays refused everywhere.
        let (a, b) = (granted(900), granted(901));
        d.set("wss://q.livekit.cloud", &a, "ra");
        d.refuse(&a);
        d.set("wss://q.livekit.cloud", &b, "rb");
        d.set("wss://q.livekit.cloud", &a, "ra");
        let process = send(d.route(Some("q.livekit.cloud"), Some("gone")));
        assert_eq!(process.token, Some(b), "the project copy is b, never the refused a");

        // Churn: 50 projects, their Rooms gone, nothing cached: nothing is retained.
        for n in 0..50 {
            d.set(&format!("wss://c{n}.livekit.cloud"), &granted(1000 + n), &format!("churn-{n}"));
        }
        let live: HashMap<String, Option<String>> =
            [("rb".to_owned(), Some("q.livekit.cloud".to_owned()))].into();
        d.retain_owners(&live, &Default::default());
        assert_eq!(d.credentials(), 2, "rb's token and q's project copy, nothing else");
        let backlog = [(Some("q.livekit.cloud".to_owned()), "ra".to_owned())].into();
        d.set("wss://q.livekit.cloud", &granted(902), "ra");
        d.retain_owners(&Default::default(), &backlog);
        assert_eq!(d.credentials(), 2, "a gone Room's backlog keeps its credential and q's copy");
    }

    #[test]
    fn not_found_recovers_with_the_next_token_disabled_does_not() {
        let mut d = Destinations::default();
        let host = d.set("wss://p.livekit.cloud", &granted(600), "s").expect("host");
        d.kill(&host, Dead::NotFound);
        assert_eq!(d.route(Some(&host), Some("s")), Route::Drop);
        d.set("wss://p.livekit.cloud", &granted(700), "s");
        assert!(matches!(d.route(Some(&host), Some("s")), Route::Send(_)), "a new connect retries");
        d.kill(&host, Dead::Disabled);
        d.set("wss://p.livekit.cloud", &granted(800), "s");
        assert_eq!(d.route(Some(&host), Some("s")), Route::Drop, "the owner's switch stands");
    }

    #[test]
    fn the_override_takes_everything_without_a_token() {
        let d = Destinations::new(Some("http://localhost:4318/"));
        let target = send(d.route(None, None));
        assert_eq!(target.logs, "http://localhost:4318/v1/logs");
        assert_eq!(target.traces, "http://localhost:4318/v1/traces");
        assert_eq!(target.token, None);
        let full = Destinations::new(Some("https://h/observability/client/logs/otlp/v0"));
        assert_eq!(
            send(full.route(Some("ignored"), None)).traces,
            "https://h/observability/client/traces/otlp/v0"
        );
        let mut local = Destinations::new(Some("http://localhost:4318"));
        let host = local.set("ws://localhost:7880", "not-a-jwt", "s");
        assert!(matches!(local.route(host.as_deref(), Some("s")), Route::Send(_)));
    }

    /// Finding r2-11: a past, negative, fractional-past or malformed `exp` is expired (a hard
    /// hold); only a missing one means no known expiry.
    #[tokio::test(start_paused = true)]
    async fn untrusted_expiry_claims_fail_closed() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_secs_f64();
        let token = |exp: serde_json::Value| {
            jwt(serde_json::json!({ "exp": exp, "observability": { "write": true } }))
        };
        for exp in [
            serde_json::json!(-1),
            serde_json::json!(0),
            serde_json::json!(now - 0.5),
            serde_json::json!(-1e308),
            serde_json::json!("tomorrow"),
            serde_json::json!(true),
            serde_json::json!(null),
            serde_json::json!({}),
        ] {
            let mut d = Destinations::default();
            let host = d.set("wss://p.livekit.cloud", &token(exp.clone()), "s");
            assert_eq!(d.route(host.as_deref(), Some("s")), Route::Wait, "exp {exp}: expired");
        }
        let mut d = Destinations::default();
        let host = d.set("wss://p.livekit.cloud", &token(serde_json::json!(now + 60.9)), "s");
        assert!(matches!(d.route(host.as_deref(), Some("s")), Route::Send(_)), "fractional future");
        let host = d.set("wss://p.livekit.cloud", &token(serde_json::json!(1e300)), "s");
        assert!(matches!(d.route(host.as_deref(), Some("s")), Route::Send(_)), "extreme: clamped");
    }

    #[test]
    fn tokens_are_read_not_verified() {
        assert!(Token::parse(&granted(60)).grant);
        let client_write = jwt(serde_json::json!({ "observability": { "clientWrite": true } }));
        let parsed = Token::parse(&client_write);
        assert!(parsed.grant && parsed.expires.is_none(), "no exp: no known expiry");
        assert!(!Token::parse("garbage").grant);
        assert!(!Token::parse("a.!!!.c").grant);
    }
}
