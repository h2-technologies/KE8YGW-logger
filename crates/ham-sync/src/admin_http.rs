//! Blocking HTTPS transport for hosted administration requests.
//!
//! This adapter is transport-only: it carries a Rust-planned request to the
//! hosted server and returns the observed status and JSON body. It never
//! interprets outcomes, mutates the durable administration record, or persists
//! secrets. Native iOS keeps using its own URLSession transport against the
//! same [`crate::admin`] planning and interpretation functions.

use std::{io::Read, time::Duration};

use serde_json::Value as JsonValue;

use crate::admin::{HostedAdminHttpResponse, HostedAdminRequestPlan, HostedAdminTransport};

/// `ureq`-backed hosted administration transport for desktop, hosted web, and CLI.
#[derive(Debug, Clone, Default)]
pub struct HttpHostedAdminTransport;

impl HttpHostedAdminTransport {
    pub fn new() -> Self {
        Self
    }
}

impl HostedAdminTransport for HttpHostedAdminTransport {
    fn execute(
        &self,
        plan: &HostedAdminRequestPlan,
        body: Option<&JsonValue>,
        session_token: &str,
    ) -> Result<HostedAdminHttpResponse, String> {
        let timeout = Duration::from_secs(plan.timeout_seconds.clamp(1, 120));
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout)
            .timeout(timeout)
            .build();
        let request = agent
            .request(&plan.method, &plan.url)
            .set("accept", "application/json")
            .set("x-request-id", &plan.request_id.to_string())
            .set("authorization", &format!("Bearer {session_token}"));

        let outcome = match body {
            Some(body) => request
                .set("content-type", "application/json")
                .send_string(&body.to_string()),
            None => request.call(),
        };

        let (status, response) = match outcome {
            Ok(response) => (response.status(), response),
            Err(ureq::Error::Status(status, response)) => (status, response),
            Err(ureq::Error::Transport(error)) => return Err(error.to_string()),
        };

        let mut buffer = Vec::new();
        response
            .into_reader()
            .take(plan.max_response_bytes as u64)
            .read_to_end(&mut buffer)
            .map_err(|error| error.to_string())?;
        let parsed = if buffer.is_empty() {
            None
        } else {
            serde_json::from_slice::<JsonValue>(&buffer).ok()
        };
        Ok(HostedAdminHttpResponse::new(status, parsed))
    }
}
