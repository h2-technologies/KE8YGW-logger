//! Prometheus-compatible metrics for the KE8YGW Logger server binaries.
//!
//! This crate deliberately has no external dependencies. It owns a small
//! registry (counters, gauges, histograms), a Prometheus text-exposition
//! encoder, bounded route labelling, and the scrape-authorization policy that
//! the hosted (`ham-server`) and self-hosted (`ham-sync-server`) binaries
//! share so a Prometheus/Grafana stack can observe both deployments the same
//! way.
//!
//! The crate stores no request bodies, tokens, callsigns, e-mail addresses, or
//! any other operator data: only aggregated counts, label sets the servers
//! choose from bounded vocabularies, and timing distributions.

use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fmt::Write as _,
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Content type Prometheus expects for the text exposition format.
pub const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Default latency histogram buckets, in seconds.
pub const DEFAULT_DURATION_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Default payload-size histogram buckets, in bytes.
pub const DEFAULT_SIZE_BUCKETS: &[f64] = &[
    128.0,
    512.0,
    2048.0,
    8192.0,
    32768.0,
    131_072.0,
    524_288.0,
    2_097_152.0,
    8_388_608.0,
];

/// Maximum number of distinct label sets stored per metric family.
///
/// Label sets past the cap are dropped and counted in
/// `ham_metrics_series_dropped_total` so a mislabelled call site degrades
/// observability instead of exhausting server memory.
pub const MAX_SERIES_PER_FAMILY: usize = 512;

/// Route label used when a request path matches no known route pattern.
pub const UNMATCHED_ROUTE_LABEL: &str = "unmatched";

/// Metric families exported by every server through [`ServerMetrics`].
pub const HTTP_REQUESTS_TOTAL: &str = "ham_http_requests_total";
pub const HTTP_REQUEST_DURATION_SECONDS: &str = "ham_http_request_duration_seconds";
pub const HTTP_REQUESTS_IN_FLIGHT: &str = "ham_http_requests_in_flight";
pub const HTTP_REQUEST_BYTES_TOTAL: &str = "ham_http_request_bytes_total";
pub const HTTP_RESPONSE_BYTES_TOTAL: &str = "ham_http_response_bytes_total";
pub const HTTP_RESPONSE_SIZE_BYTES: &str = "ham_http_response_size_bytes";
pub const BUILD_INFO: &str = "ham_build_info";
pub const PROCESS_START_TIME_SECONDS: &str = "ham_process_start_time_seconds";
pub const PROCESS_UPTIME_SECONDS: &str = "ham_process_uptime_seconds";
pub const METRICS_SERIES: &str = "ham_metrics_series";
pub const METRICS_SERIES_DROPPED_TOTAL: &str = "ham_metrics_series_dropped_total";

/// Kind of a metric family in the Prometheus data model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricKind {
    Counter,
    Gauge,
    Histogram,
}

impl MetricKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Counter => "counter",
            Self::Gauge => "gauge",
            Self::Histogram => "histogram",
        }
    }
}

#[derive(Debug, Clone)]
struct HistogramSeries {
    bounds: Vec<f64>,
    cumulative: Vec<u64>,
    sum: f64,
    count: u64,
}

impl HistogramSeries {
    fn new(buckets: &[f64]) -> Self {
        let mut bounds: Vec<f64> = buckets
            .iter()
            .copied()
            .filter(|bound| bound.is_finite())
            .collect();
        bounds.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
        bounds.dedup();
        let cumulative = vec![0; bounds.len()];
        Self {
            bounds,
            cumulative,
            sum: 0.0,
            count: 0,
        }
    }

    fn observe(&mut self, value: f64) {
        if !value.is_finite() {
            return;
        }
        for (index, bound) in self.bounds.iter().enumerate() {
            if value <= *bound {
                self.cumulative[index] = self.cumulative[index].saturating_add(1);
            }
        }
        self.sum += value;
        self.count = self.count.saturating_add(1);
    }
}

#[derive(Debug, Clone)]
enum SeriesValue {
    Scalar(f64),
    Histogram(HistogramSeries),
}

#[derive(Debug, Clone)]
struct MetricFamily {
    help: String,
    kind: MetricKind,
    series: BTreeMap<Vec<(String, String)>, SeriesValue>,
}

#[derive(Debug, Default)]
struct RegistryInner {
    families: BTreeMap<String, MetricFamily>,
    dropped_series: u64,
}

/// Thread-safe registry of counters, gauges, and histograms.
#[derive(Debug, Default)]
pub struct MetricsRegistry {
    inner: Mutex<RegistryInner>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, RegistryInner> {
        // A panicking scrape must never take the metrics registry (and with it
        // the server) down: recovering the guard keeps counting.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Add `value` to a counter, creating the series when it is first seen.
    pub fn add_counter(&self, name: &str, help: &str, labels: &[(&str, &str)], value: f64) {
        if value < 0.0 || !value.is_finite() {
            return;
        }
        self.mutate_scalar(name, help, MetricKind::Counter, labels, |current| {
            *current += value;
        });
    }

    /// Increment a counter by one.
    pub fn increment_counter(&self, name: &str, help: &str, labels: &[(&str, &str)]) {
        self.add_counter(name, help, labels, 1.0);
    }

    /// Set a counter to an absolute value that the caller guarantees is
    /// monotonic (for example a running total kept elsewhere).
    pub fn set_counter(&self, name: &str, help: &str, labels: &[(&str, &str)], value: f64) {
        if !value.is_finite() {
            return;
        }
        self.mutate_scalar(name, help, MetricKind::Counter, labels, |current| {
            *current = value;
        });
    }

    /// Set a gauge to an absolute value.
    pub fn set_gauge(&self, name: &str, help: &str, labels: &[(&str, &str)], value: f64) {
        if !value.is_finite() {
            return;
        }
        self.mutate_scalar(name, help, MetricKind::Gauge, labels, |current| {
            *current = value;
        });
    }

    /// Add a (possibly negative) delta to a gauge.
    pub fn add_gauge(&self, name: &str, help: &str, labels: &[(&str, &str)], delta: f64) {
        if !delta.is_finite() {
            return;
        }
        self.mutate_scalar(name, help, MetricKind::Gauge, labels, |current| {
            *current += delta;
        });
    }

    /// Record one histogram observation.
    pub fn observe_histogram(
        &self,
        name: &str,
        help: &str,
        labels: &[(&str, &str)],
        buckets: &[f64],
        value: f64,
    ) {
        if !value.is_finite() {
            return;
        }
        let name = sanitize_metric_name(name);
        let key = label_key(labels);
        let mut inner = self.lock();
        let family = inner.families.entry(name).or_insert_with(|| MetricFamily {
            help: help.to_owned(),
            kind: MetricKind::Histogram,
            series: BTreeMap::new(),
        });
        if family.kind != MetricKind::Histogram {
            return;
        }
        if !family.series.contains_key(&key) && family.series.len() >= MAX_SERIES_PER_FAMILY {
            inner.dropped_series = inner.dropped_series.saturating_add(1);
            return;
        }
        let series = family
            .series
            .entry(key)
            .or_insert_with(|| SeriesValue::Histogram(HistogramSeries::new(buckets)));
        if let SeriesValue::Histogram(histogram) = series {
            histogram.observe(value);
        }
    }

    fn mutate_scalar(
        &self,
        name: &str,
        help: &str,
        kind: MetricKind,
        labels: &[(&str, &str)],
        apply: impl FnOnce(&mut f64),
    ) {
        let name = sanitize_metric_name(name);
        let key = label_key(labels);
        let mut inner = self.lock();
        let family = inner.families.entry(name).or_insert_with(|| MetricFamily {
            help: help.to_owned(),
            kind,
            series: BTreeMap::new(),
        });
        if family.kind != kind {
            return;
        }
        if !family.series.contains_key(&key) && family.series.len() >= MAX_SERIES_PER_FAMILY {
            inner.dropped_series = inner.dropped_series.saturating_add(1);
            return;
        }
        let series = family.series.entry(key).or_insert(SeriesValue::Scalar(0.0));
        if let SeriesValue::Scalar(current) = series {
            apply(current);
        }
    }

    /// Drop every series of one family.
    ///
    /// Snapshot gauges are recomputed on each scrape; clearing first stops a
    /// label set that no longer exists (an upload status with no jobs left,
    /// for example) from being reported forever at its last value.
    pub fn reset_family(&self, name: &str) {
        let name = sanitize_metric_name(name);
        let mut inner = self.lock();
        if let Some(family) = inner.families.get_mut(&name) {
            family.series.clear();
        }
    }

    /// Number of distinct series currently stored across all families.
    pub fn series_count(&self) -> usize {
        self.lock()
            .families
            .values()
            .map(|family| family.series.len())
            .sum()
    }

    /// Number of series dropped because a family reached [`MAX_SERIES_PER_FAMILY`].
    pub fn dropped_series(&self) -> u64 {
        self.lock().dropped_series
    }

    /// Render every family in the Prometheus text exposition format.
    pub fn encode_prometheus(&self) -> String {
        let inner = self.lock();
        let mut out = String::new();
        for (name, family) in &inner.families {
            let _ = writeln!(out, "# HELP {name} {}", escape_help(&family.help));
            let _ = writeln!(out, "# TYPE {name} {}", family.kind.as_str());
            for (labels, value) in &family.series {
                match value {
                    SeriesValue::Scalar(scalar) => {
                        let _ = writeln!(
                            out,
                            "{name}{} {}",
                            format_labels(labels, None),
                            format_value(*scalar)
                        );
                    }
                    SeriesValue::Histogram(histogram) => {
                        for (index, bound) in histogram.bounds.iter().enumerate() {
                            let _ = writeln!(
                                out,
                                "{name}_bucket{} {}",
                                format_labels(labels, Some(("le", &format_value(*bound)))),
                                histogram.cumulative[index]
                            );
                        }
                        let _ = writeln!(
                            out,
                            "{name}_bucket{} {}",
                            format_labels(labels, Some(("le", "+Inf"))),
                            histogram.count
                        );
                        let _ = writeln!(
                            out,
                            "{name}_sum{} {}",
                            format_labels(labels, None),
                            format_value(histogram.sum)
                        );
                        let _ = writeln!(
                            out,
                            "{name}_count{} {}",
                            format_labels(labels, None),
                            histogram.count
                        );
                    }
                }
            }
        }
        out
    }
}

/// Server-facing metrics facade.
///
/// Every series it records carries a `service` label so one Prometheus job can
/// scrape a hosted and a self-hosted deployment without the dashboards mixing
/// them up.
#[derive(Debug)]
pub struct ServerMetrics {
    registry: MetricsRegistry,
    service: String,
    started_at: SystemTime,
}

impl ServerMetrics {
    /// Create a metrics facade and record the immutable build/start series.
    pub fn new(
        service: impl Into<String>,
        version: impl Into<String>,
        mode: impl Into<String>,
    ) -> Self {
        let metrics = Self {
            registry: MetricsRegistry::new(),
            service: service.into(),
            started_at: SystemTime::now(),
        };
        let version = version.into();
        let mode = mode.into();
        metrics.registry.set_gauge(
            BUILD_INFO,
            "Build and deployment identity of the running server; always 1.",
            &[
                ("service", metrics.service.as_str()),
                ("version", version.as_str()),
                ("mode", mode.as_str()),
            ],
            1.0,
        );
        metrics.registry.set_gauge(
            PROCESS_START_TIME_SECONDS,
            "Unix timestamp at which the server process started.",
            &[("service", metrics.service.as_str())],
            metrics
                .started_at
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64(),
        );
        metrics
    }

    pub fn service(&self) -> &str {
        &self.service
    }

    pub fn registry(&self) -> &MetricsRegistry {
        &self.registry
    }

    fn labels<'a>(&'a self, labels: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let mut all = Vec::with_capacity(labels.len() + 1);
        all.push(("service", self.service.as_str()));
        all.extend_from_slice(labels);
        all
    }

    /// Increment a service-scoped counter by one.
    pub fn increment(&self, name: &str, help: &str, labels: &[(&str, &str)]) {
        self.registry
            .increment_counter(name, help, &self.labels(labels));
    }

    /// Add a value to a service-scoped counter.
    pub fn add(&self, name: &str, help: &str, labels: &[(&str, &str)], value: f64) {
        self.registry
            .add_counter(name, help, &self.labels(labels), value);
    }

    /// Set a service-scoped gauge.
    pub fn gauge(&self, name: &str, help: &str, labels: &[(&str, &str)], value: f64) {
        self.registry
            .set_gauge(name, help, &self.labels(labels), value);
    }

    /// Observe a service-scoped histogram value.
    pub fn observe(
        &self,
        name: &str,
        help: &str,
        labels: &[(&str, &str)],
        buckets: &[f64],
        value: f64,
    ) {
        self.registry
            .observe_histogram(name, help, &self.labels(labels), buckets, value);
    }

    /// Clear a snapshot-gauge family before it is recomputed.
    pub fn reset_family(&self, name: &str) {
        self.registry.reset_family(name);
    }

    /// Track one in-flight request until the returned guard is dropped.
    pub fn request_started(&self) -> InFlightGuard<'_> {
        self.registry.add_gauge(
            HTTP_REQUESTS_IN_FLIGHT,
            "HTTP requests currently being served.",
            &[("service", self.service.as_str())],
            1.0,
        );
        InFlightGuard { metrics: self }
    }

    /// Record a completed HTTP request.
    ///
    /// `route` must be a bounded route pattern (see [`RouteMatcher`]), never a
    /// raw path: raw paths carry logbook and report identifiers and would make
    /// the time series cardinality unbounded.
    pub fn record_http_request(
        &self,
        method: &str,
        route: &str,
        status: u16,
        duration: Duration,
        request_bytes: usize,
        response_bytes: usize,
    ) {
        let status = status.to_string();
        let method = normalize_method(method);
        self.increment(
            HTTP_REQUESTS_TOTAL,
            "Total HTTP requests served, by method, route pattern, and status code.",
            &[
                ("method", method.as_str()),
                ("route", route),
                ("status", status.as_str()),
            ],
        );
        self.observe(
            HTTP_REQUEST_DURATION_SECONDS,
            "HTTP request handling duration in seconds.",
            &[("method", method.as_str()), ("route", route)],
            DEFAULT_DURATION_BUCKETS,
            duration.as_secs_f64(),
        );
        self.add(
            HTTP_REQUEST_BYTES_TOTAL,
            "Total HTTP request body bytes received.",
            &[("method", method.as_str()), ("route", route)],
            request_bytes as f64,
        );
        self.add(
            HTTP_RESPONSE_BYTES_TOTAL,
            "Total HTTP response body bytes written.",
            &[("method", method.as_str()), ("route", route)],
            response_bytes as f64,
        );
        self.observe(
            HTTP_RESPONSE_SIZE_BYTES,
            "HTTP response body size in bytes.",
            &[("method", method.as_str()), ("route", route)],
            DEFAULT_SIZE_BUCKETS,
            response_bytes as f64,
        );
    }

    /// Uptime of the process in seconds.
    pub fn uptime_seconds(&self) -> f64 {
        self.started_at.elapsed().unwrap_or_default().as_secs_f64()
    }

    /// Render the scrape body, refreshing the process and registry-health series.
    pub fn render(&self) -> String {
        self.gauge(
            PROCESS_UPTIME_SECONDS,
            "Seconds since the server process started.",
            &[],
            self.uptime_seconds(),
        );
        self.gauge(
            METRICS_SERIES,
            "Distinct time series currently held by the in-process metrics registry.",
            &[],
            self.registry.series_count() as f64,
        );
        self.registry.set_counter(
            METRICS_SERIES_DROPPED_TOTAL,
            "Series dropped because a metric family reached its cardinality cap.",
            &[("service", self.service.as_str())],
            self.registry.dropped_series() as f64,
        );
        self.registry.encode_prometheus()
    }
}

/// Decrements the in-flight gauge when the request finishes.
#[derive(Debug)]
pub struct InFlightGuard<'a> {
    metrics: &'a ServerMetrics,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.metrics.registry.add_gauge(
            HTTP_REQUESTS_IN_FLIGHT,
            "HTTP requests currently being served.",
            &[("service", self.metrics.service.as_str())],
            -1.0,
        );
    }
}

/// Outcome of authorizing one scrape of `/metrics`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrapeAuthorization {
    /// Serve the exposition body.
    Allowed,
    /// The endpoint is switched off for this deployment.
    Disabled,
    /// A scrape token is configured and the request did not present it.
    Unauthorized,
}

/// Deployment policy for the `/metrics` endpoint.
#[derive(Debug, Clone, Default)]
pub struct MetricsAccess {
    enabled: bool,
    token: Option<String>,
}

impl MetricsAccess {
    pub fn new(enabled: bool, token: Option<String>) -> Self {
        Self {
            enabled,
            token: token
                .map(|token| token.trim().to_owned())
                .filter(|token| !token.is_empty()),
        }
    }

    /// Read the policy from the environment.
    ///
    /// The endpoint is enabled unless the enable variable is set to a falsy
    /// value, and unauthenticated unless the token variable is set.
    pub fn from_env(enabled_var: &str, token_var: &str) -> Self {
        let enabled = std::env::var(enabled_var).map_or(true, |value| is_truthy(&value));
        Self::new(enabled, std::env::var(token_var).ok())
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn requires_token(&self) -> bool {
        self.token.is_some()
    }

    /// True when the endpoint is served without any authentication, which is
    /// only safe on a trusted scrape network.
    pub fn is_open(&self) -> bool {
        self.enabled && self.token.is_none()
    }

    /// Authorize one scrape from its `Authorization` header value.
    pub fn authorize(&self, authorization_header: Option<&str>) -> ScrapeAuthorization {
        if !self.enabled {
            return ScrapeAuthorization::Disabled;
        }
        let Some(expected) = self.token.as_deref() else {
            return ScrapeAuthorization::Allowed;
        };
        let presented = authorization_header
            .map(str::trim)
            .and_then(|value| {
                value
                    .strip_prefix("Bearer ")
                    .or_else(|| value.strip_prefix("bearer "))
            })
            .map(str::trim)
            .unwrap_or_default();
        if constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
            ScrapeAuthorization::Allowed
        } else {
            ScrapeAuthorization::Unauthorized
        }
    }
}

/// Maps concrete request paths onto the bounded route patterns used as metric labels.
#[derive(Debug, Clone)]
pub struct RouteMatcher {
    routes: Vec<RoutePattern>,
}

#[derive(Debug, Clone)]
struct RoutePattern {
    method: String,
    segments: Vec<String>,
    label: String,
}

impl RouteMatcher {
    /// Build a matcher from `"METHOD /path/:param"` route strings.
    pub fn new(routes: &[&str]) -> Self {
        let routes = routes
            .iter()
            .filter_map(|route| {
                let (method, path) = route.split_once(' ')?;
                Some(RoutePattern {
                    method: method.trim().to_ascii_uppercase(),
                    segments: path_segments(path.trim())
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                    label: format!("{} {}", method.trim().to_ascii_uppercase(), path.trim()),
                })
            })
            .collect();
        Self { routes }
    }

    /// Extend the matcher with additional route strings.
    pub fn with_routes(mut self, routes: &[&str]) -> Self {
        let extra = Self::new(routes);
        self.routes.extend(extra.routes);
        self
    }

    /// Route-pattern label for one request, or [`UNMATCHED_ROUTE_LABEL`].
    pub fn label(&self, method: &str, path: &str) -> &str {
        let method = method.trim().to_ascii_uppercase();
        let segments = path_segments(path);
        self.routes
            .iter()
            .find(|route| route.matches(&method, &segments))
            .map_or(UNMATCHED_ROUTE_LABEL, |route| route.label.as_str())
    }
}

impl RoutePattern {
    fn matches(&self, method: &str, segments: &[&str]) -> bool {
        if self.method != method || self.segments.len() != segments.len() {
            return false;
        }
        self.segments
            .iter()
            .zip(segments)
            .all(|(pattern, actual)| pattern.starts_with(':') || pattern == actual)
    }
}

fn path_segments(path: &str) -> Vec<&str> {
    path.split('?')
        .next()
        .unwrap_or(path)
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn normalize_method(method: &str) -> String {
    let method = method.trim().to_ascii_uppercase();
    const KNOWN: &[&str] = &[
        "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE", "CONNECT", "QUERY",
    ];
    if KNOWN.contains(&method.as_str()) {
        method
    } else {
        "OTHER".to_owned()
    }
}

fn is_truthy(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "no" | "disabled" | ""
    )
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

fn label_key(labels: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut key: Vec<(String, String)> = labels
        .iter()
        .filter(|(name, _)| !name.trim().is_empty())
        .map(|(name, value)| (sanitize_label_name(name), (*value).to_owned()))
        .collect();
    key.sort_by(|left, right| left.0.cmp(&right.0));
    key.dedup_by(|later, earlier| later.0 == earlier.0);
    key
}

fn format_labels(labels: &[(String, String)], extra: Option<(&str, &str)>) -> String {
    if labels.is_empty() && extra.is_none() {
        return String::new();
    }
    let mut rendered = String::from("{");
    let mut first = true;
    for (name, value) in labels {
        if !first {
            rendered.push(',');
        }
        first = false;
        let _ = write!(rendered, "{name}=\"{}\"", escape_label_value(value));
    }
    if let Some((name, value)) = extra {
        if !first {
            rendered.push(',');
        }
        let _ = write!(rendered, "{name}=\"{}\"", escape_label_value(value));
    }
    rendered.push('}');
    rendered
}

fn sanitize_metric_name(name: &str) -> String {
    let mut sanitized = String::with_capacity(name.len());
    for (index, character) in name.chars().enumerate() {
        let valid = character.is_ascii_alphabetic()
            || character == '_'
            || character == ':'
            || (index > 0 && character.is_ascii_digit());
        sanitized.push(if valid { character } else { '_' });
    }
    if sanitized.is_empty() {
        "invalid_metric_name".to_owned()
    } else {
        sanitized
    }
}

fn sanitize_label_name(name: &str) -> String {
    let mut sanitized = String::with_capacity(name.len());
    for (index, character) in name.chars().enumerate() {
        let valid = character.is_ascii_alphabetic()
            || character == '_'
            || (index > 0 && character.is_ascii_digit());
        sanitized.push(if valid { character } else { '_' });
    }
    if sanitized.is_empty() {
        "invalid_label_name".to_owned()
    } else {
        sanitized
    }
}

fn escape_label_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn escape_help(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn format_value(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "+Inf".to_owned()
        } else {
            "-Inf".to_owned()
        };
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTED_ROUTES: &[&str] = &[
        "GET /health",
        "GET /api/v1/logbooks",
        "GET /api/v1/logbooks/:id",
        "POST /api/v1/qsos/:id/delete",
    ];

    #[test]
    fn counters_gauges_and_histograms_render_in_prometheus_text_format() {
        let registry = MetricsRegistry::new();
        registry.increment_counter("ham_test_total", "Test counter.", &[("route", "/health")]);
        registry.increment_counter("ham_test_total", "Test counter.", &[("route", "/health")]);
        registry.set_gauge("ham_test_gauge", "Test gauge.", &[], 3.5);
        registry.observe_histogram("ham_test_seconds", "Test histogram.", &[], &[0.1, 1.0], 0.5);

        let rendered = registry.encode_prometheus();

        assert!(rendered.contains("# TYPE ham_test_total counter"));
        assert!(rendered.contains("ham_test_total{route=\"/health\"} 2"));
        assert!(rendered.contains("# TYPE ham_test_gauge gauge"));
        assert!(rendered.contains("ham_test_gauge 3.5"));
        assert!(rendered.contains("# TYPE ham_test_seconds histogram"));
        assert!(rendered.contains("ham_test_seconds_bucket{le=\"0.1\"} 0"));
        assert!(rendered.contains("ham_test_seconds_bucket{le=\"1\"} 1"));
        assert!(rendered.contains("ham_test_seconds_bucket{le=\"+Inf\"} 1"));
        assert!(rendered.contains("ham_test_seconds_sum 0.5"));
        assert!(rendered.contains("ham_test_seconds_count 1"));
    }

    #[test]
    fn histogram_buckets_are_cumulative_and_sorted() {
        let registry = MetricsRegistry::new();
        for value in [0.05_f64, 0.2, 2.0] {
            registry.observe_histogram(
                "ham_test_seconds",
                "Test histogram.",
                &[],
                &[1.0, 0.1, 0.1],
                value,
            );
        }

        let rendered = registry.encode_prometheus();

        assert!(rendered.contains("ham_test_seconds_bucket{le=\"0.1\"} 1"));
        assert!(rendered.contains("ham_test_seconds_bucket{le=\"1\"} 2"));
        assert!(rendered.contains("ham_test_seconds_bucket{le=\"+Inf\"} 3"));
        assert!(rendered.contains("ham_test_seconds_count 3"));
    }

    #[test]
    fn label_order_does_not_create_duplicate_series() {
        let registry = MetricsRegistry::new();
        registry.increment_counter(
            "ham_test_total",
            "Test counter.",
            &[("method", "GET"), ("route", "/health")],
        );
        registry.increment_counter(
            "ham_test_total",
            "Test counter.",
            &[("route", "/health"), ("method", "GET")],
        );

        assert_eq!(registry.series_count(), 1);
        assert!(registry
            .encode_prometheus()
            .contains("ham_test_total{method=\"GET\",route=\"/health\"} 2"));
    }

    #[test]
    fn label_values_and_help_text_are_escaped() {
        let registry = MetricsRegistry::new();
        registry.set_gauge(
            "ham_test_gauge",
            "Help with a \\ backslash and a\nnewline.",
            &[("detail", "quote\" backslash\\ newline\n")],
            1.0,
        );

        let rendered = registry.encode_prometheus();

        assert!(
            rendered.contains("# HELP ham_test_gauge Help with a \\\\ backslash and a\\nnewline.")
        );
        assert!(rendered.contains("detail=\"quote\\\" backslash\\\\ newline\\n\""));
        assert_eq!(rendered.lines().count(), 3);
    }

    #[test]
    fn invalid_metric_and_label_names_are_sanitized() {
        let registry = MetricsRegistry::new();
        registry.set_gauge("9 bad-name", "Sanitized.", &[("bad label", "value")], 1.0);

        let rendered = registry.encode_prometheus();

        assert!(rendered.contains("_bad_name{bad_label=\"value\"} 1"));
    }

    #[test]
    fn series_past_the_cardinality_cap_are_dropped_and_counted() {
        let registry = MetricsRegistry::new();
        for index in 0..(MAX_SERIES_PER_FAMILY + 10) {
            let value = index.to_string();
            registry.increment_counter("ham_test_total", "Test counter.", &[("id", &value)]);
        }

        assert_eq!(registry.series_count(), MAX_SERIES_PER_FAMILY);
        assert_eq!(registry.dropped_series(), 10);
    }

    #[test]
    fn counters_reject_negative_and_non_finite_values() {
        let registry = MetricsRegistry::new();
        registry.add_counter("ham_test_total", "Test counter.", &[], 5.0);
        registry.add_counter("ham_test_total", "Test counter.", &[], -3.0);
        registry.add_counter("ham_test_total", "Test counter.", &[], f64::NAN);
        registry.set_gauge("ham_test_gauge", "Test gauge.", &[], f64::INFINITY);

        assert!(registry.encode_prometheus().contains("ham_test_total 5"));
        assert!(!registry.encode_prometheus().contains("ham_test_gauge"));
    }

    #[test]
    fn a_family_keeps_its_first_metric_kind() {
        let registry = MetricsRegistry::new();
        registry.increment_counter("ham_test_total", "Test counter.", &[]);
        registry.set_gauge("ham_test_total", "Test gauge.", &[], 99.0);

        let rendered = registry.encode_prometheus();

        assert!(rendered.contains("# TYPE ham_test_total counter"));
        assert!(rendered.contains("ham_test_total 1"));
    }

    #[test]
    fn resetting_a_family_drops_stale_snapshot_labels() {
        let registry = MetricsRegistry::new();
        registry.set_gauge(
            "ham_test_gauge",
            "Test gauge.",
            &[("status", "queued")],
            4.0,
        );
        registry.reset_family("ham_test_gauge");
        registry.set_gauge(
            "ham_test_gauge",
            "Test gauge.",
            &[("status", "running")],
            1.0,
        );

        let rendered = registry.encode_prometheus();

        assert!(!rendered.contains("queued"));
        assert!(rendered.contains("ham_test_gauge{status=\"running\"} 1"));
    }

    #[test]
    fn server_metrics_label_every_series_with_the_service() {
        let metrics = ServerMetrics::new("ke8ygw-test-server", "9.9.9", "self_hosted");
        metrics.record_http_request("get", "GET /health", 200, Duration::from_millis(12), 0, 64);

        let rendered = metrics.render();

        assert!(rendered.contains(
            "ham_build_info{mode=\"self_hosted\",service=\"ke8ygw-test-server\",version=\"9.9.9\"} 1"
        ));
        assert!(rendered.contains("ham_process_start_time_seconds{service=\"ke8ygw-test-server\"}"));
        assert!(rendered.contains("ham_process_uptime_seconds{service=\"ke8ygw-test-server\"}"));
        assert!(rendered.contains(
            "ham_http_requests_total{method=\"GET\",route=\"GET /health\",service=\"ke8ygw-test-server\",status=\"200\"} 1"
        ));
        assert!(rendered.contains("ham_http_request_duration_seconds_count{method=\"GET\",route=\"GET /health\",service=\"ke8ygw-test-server\"} 1"));
        assert!(rendered.contains("ham_http_response_bytes_total{method=\"GET\",route=\"GET /health\",service=\"ke8ygw-test-server\"} 64"));
        assert!(rendered.contains("ham_metrics_series{service=\"ke8ygw-test-server\"}"));
        assert!(
            rendered.contains("ham_metrics_series_dropped_total{service=\"ke8ygw-test-server\"} 0")
        );
    }

    #[test]
    fn unknown_http_methods_collapse_into_one_label() {
        let metrics = ServerMetrics::new("ke8ygw-test-server", "9.9.9", "hosted");
        metrics.record_http_request("BREW", "unmatched", 404, Duration::from_millis(1), 0, 10);

        assert!(metrics.render().contains("method=\"OTHER\""));
    }

    #[test]
    fn in_flight_requests_return_to_zero_when_the_guard_drops() {
        let metrics = ServerMetrics::new("ke8ygw-test-server", "9.9.9", "hosted");
        {
            let _guard = metrics.request_started();
            assert!(metrics
                .render()
                .contains("ham_http_requests_in_flight{service=\"ke8ygw-test-server\"} 1"));
        }

        assert!(metrics
            .render()
            .contains("ham_http_requests_in_flight{service=\"ke8ygw-test-server\"} 0"));
    }

    #[test]
    fn route_matcher_maps_identifier_paths_onto_bounded_patterns() {
        let matcher = RouteMatcher::new(HOSTED_ROUTES);

        assert_eq!(matcher.label("GET", "/health"), "GET /health");
        assert_eq!(matcher.label("get", "/health/"), "GET /health");
        assert_eq!(
            matcher.label("GET", "/api/v1/logbooks"),
            "GET /api/v1/logbooks"
        );
        assert_eq!(
            matcher.label(
                "GET",
                "/api/v1/logbooks/2ee0a1f4-6f1f-4d42-9a0a-1a4f0a7a0001"
            ),
            "GET /api/v1/logbooks/:id"
        );
        assert_eq!(
            matcher.label(
                "POST",
                "/api/v1/qsos/2ee0a1f4-6f1f-4d42-9a0a-1a4f0a7a0001/delete"
            ),
            "POST /api/v1/qsos/:id/delete"
        );
    }

    #[test]
    fn route_matcher_labels_unknown_paths_and_methods_as_unmatched() {
        let matcher = RouteMatcher::new(HOSTED_ROUTES);

        assert_eq!(
            matcher.label("GET", "/does/not/exist"),
            UNMATCHED_ROUTE_LABEL
        );
        assert_eq!(matcher.label("DELETE", "/health"), UNMATCHED_ROUTE_LABEL);
        assert_eq!(
            matcher.label("GET", "/api/v1/logbooks/a/b"),
            UNMATCHED_ROUTE_LABEL
        );
    }

    #[test]
    fn route_matcher_ignores_query_strings() {
        let matcher = RouteMatcher::new(HOSTED_ROUTES);

        assert_eq!(
            matcher.label("GET", "/api/v1/logbooks?token=secret"),
            "GET /api/v1/logbooks"
        );
    }

    #[test]
    fn additional_routes_can_be_appended_to_a_matcher() {
        let matcher = RouteMatcher::new(HOSTED_ROUTES).with_routes(&["GET /metrics"]);

        assert_eq!(matcher.label("GET", "/metrics"), "GET /metrics");
    }

    #[test]
    fn open_access_serves_every_scrape() {
        let access = MetricsAccess::new(true, None);

        assert!(access.enabled());
        assert!(access.is_open());
        assert!(!access.requires_token());
        assert_eq!(access.authorize(None), ScrapeAuthorization::Allowed);
    }

    #[test]
    fn a_configured_token_is_required_and_compared_in_full() {
        let access = MetricsAccess::new(true, Some("  scrape-token  ".to_owned()));

        assert!(access.requires_token());
        assert!(!access.is_open());
        assert_eq!(
            access.authorize(Some("Bearer scrape-token")),
            ScrapeAuthorization::Allowed
        );
        assert_eq!(
            access.authorize(Some("bearer scrape-token")),
            ScrapeAuthorization::Allowed
        );
        assert_eq!(access.authorize(None), ScrapeAuthorization::Unauthorized);
        assert_eq!(
            access.authorize(Some("Bearer scrape-toke")),
            ScrapeAuthorization::Unauthorized
        );
        assert_eq!(
            access.authorize(Some("Bearer scrape-tokenx")),
            ScrapeAuthorization::Unauthorized
        );
        assert_eq!(
            access.authorize(Some("scrape-token")),
            ScrapeAuthorization::Unauthorized
        );
    }

    #[test]
    fn a_disabled_endpoint_never_authorizes_a_scrape() {
        let access = MetricsAccess::new(false, Some("scrape-token".to_owned()));

        assert!(!access.enabled());
        assert!(!access.is_open());
        assert_eq!(
            access.authorize(Some("Bearer scrape-token")),
            ScrapeAuthorization::Disabled
        );
    }

    #[test]
    fn an_empty_token_is_treated_as_no_token() {
        let access = MetricsAccess::new(true, Some("   ".to_owned()));

        assert!(!access.requires_token());
        assert_eq!(access.authorize(None), ScrapeAuthorization::Allowed);
    }

    #[test]
    fn falsy_environment_values_disable_the_endpoint() {
        for value in ["0", "false", "off", "no", "disabled", ""] {
            assert!(!is_truthy(value), "{value} should disable metrics");
        }
        for value in ["1", "true", "on", "yes"] {
            assert!(is_truthy(value), "{value} should enable metrics");
        }
    }

    #[test]
    fn constant_time_comparison_matches_equality() {
        assert!(constant_time_eq(b"token", b"token"));
        assert!(!constant_time_eq(b"token", b"tokeN"));
        assert!(!constant_time_eq(b"token", b"token-longer"));
        assert!(constant_time_eq(b"", b""));
    }
}
