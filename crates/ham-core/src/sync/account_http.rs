//! Blocking HTTPS transport for hosted account requests.
//!
//! This adapter is transport-only: it carries a Rust-planned request to the
//! hosted server and returns the observed status and JSON body. It never
//! interprets outcomes, mutates the durable account record, or persists
//! secrets. Native iOS keeps using its own URLSession transport against the
//! same [`crate::sync::account`] planning and interpretation functions.

use std::{io::Read, time::Duration};

use serde_json::Value as JsonValue;

use crate::sync::account::{
    HostedAccountHttpResponse, HostedAccountRequestPlan, HostedAccountTransport,
};

/// `ureq`-backed hosted account transport for desktop, hosted web, and CLI.
#[derive(Debug, Clone, Default)]
pub struct HttpHostedAccountTransport;

impl HttpHostedAccountTransport {
    pub fn new() -> Self {
        Self
    }
}

impl HostedAccountTransport for HttpHostedAccountTransport {
    fn execute(
        &self,
        plan: &HostedAccountRequestPlan,
        body: Option<&JsonValue>,
        session_token: Option<&str>,
    ) -> Result<HostedAccountHttpResponse, String> {
        let timeout = Duration::from_secs(plan.timeout_seconds.clamp(1, 120));
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout)
            .timeout(timeout)
            .build();
        let mut request = agent
            .request(&plan.method, &plan.url)
            .set("accept", "application/json")
            .set("x-request-id", &plan.request_id.to_string());
        if let Some(token) = session_token {
            request = request.set("authorization", &format!("Bearer {token}"));
        }

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
        Ok(HostedAccountHttpResponse::new(status, parsed))
    }
}
