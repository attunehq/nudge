use std::{
    fmt,
    time::{Duration, Instant},
};

use reqwest::{blocking::Client, redirect::Policy};
use serde_json::{Value, json};

use crate::credentials;

pub const MODEL: &str = "jev-1.13.0";
const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";

/// Operational failures never become a negative judgment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluationError {
    MissingCredential,
    InvalidCredentialFile,
    Timeout,
    Http(u16),
    Unavailable,
    InvalidResponse,
    InputTooLarge,
}

impl fmt::Display for EvaluationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCredential => f.write_str("Jev credential is missing; run `nudge login typesafe.ai` or set TYPESAFE_API_KEY"),
            Self::InvalidCredentialFile => f.write_str("could not read Nudge credentials.json; run `nudge login typesafe.ai` to replace it"),
            Self::Timeout => f.write_str("semantic deadline exceeded"),
            Self::Http(status) => write!(f, "Jev returned HTTP {status}"),
            Self::Unavailable => f.write_str("Jev is unavailable"),
            Self::InvalidResponse => f.write_str("Jev returned an invalid response"),
            Self::InputTooLarge => f.write_str("input exceeds the Jev request budget"),
        }
    }
}

/// Transport injection keeps tests offline without configurable credential
/// destinations.
pub trait Transport {
    fn send(&mut self, request: &Value, deadline: Instant) -> Result<Value, EvaluationError>;
}

#[derive(Default)]
pub struct JevClient {
    client: Option<Client>,
}

impl Transport for JevClient {
    fn send(&mut self, request: &Value, deadline: Instant) -> Result<Value, EvaluationError> {
        let key = credentials::load_jev()
            .map_err(|_| EvaluationError::InvalidCredentialFile)?
            .ok_or(EvaluationError::MissingCredential)?;
        self.send_authenticated(ENDPOINT, &key, request, deadline)
    }
}

impl JevClient {
    pub fn verify_key(&mut self, key: &str) -> Result<(), EvaluationError> {
        let request = json!({
            "model": MODEL,
            "state": "Nudge credential verification",
            "questions": {"connected": {"type": "noul", "instructions": "Does the state mention Nudge?"}}
        });
        let response = self.send_authenticated(
            ENDPOINT,
            key,
            &request,
            Instant::now() + Duration::from_secs(10),
        )?;
        probabilities(&request, &response).map(|_| ())
    }

    fn send_authenticated(
        &mut self,
        endpoint: &str,
        key: &str,
        request: &Value,
        deadline: Instant,
    ) -> Result<Value, EvaluationError> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(EvaluationError::Timeout)?;
        if self.client.is_none() {
            self.client = Some(
                Client::builder()
                    .redirect(Policy::none())
                    .retry(reqwest::retry::never())
                    .connect_timeout(remaining)
                    .build()
                    .map_err(|_| EvaluationError::Unavailable)?,
            );
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(EvaluationError::Timeout)?;
        let response = self
            .client
            .as_ref()
            .expect("client initialized")
            .post(endpoint)
            .bearer_auth(key)
            .timeout(remaining)
            .json(request)
            .send()
            .map_err(transport_error)?;
        if !response.status().is_success() {
            return Err(EvaluationError::Http(response.status().as_u16()));
        }
        response.json().map_err(|error| {
            if error.is_timeout() {
                EvaluationError::Timeout
            } else {
                EvaluationError::InvalidResponse
            }
        })
    }
}

fn transport_error(error: reqwest::Error) -> EvaluationError {
    if error.is_timeout() {
        EvaluationError::Timeout
    } else {
        EvaluationError::Unavailable
    }
}

pub fn probabilities(request: &Value, response: &Value) -> Result<Vec<f64>, EvaluationError> {
    if response.get("model").and_then(Value::as_str) != Some(MODEL) {
        return Err(EvaluationError::InvalidResponse);
    }
    let questions = request["questions"]
        .as_object()
        .ok_or(EvaluationError::InvalidResponse)?;
    let answers = response["answers"]
        .as_object()
        .ok_or(EvaluationError::InvalidResponse)?;
    if answers.len() != questions.len() {
        return Err(EvaluationError::InvalidResponse);
    }
    questions
        .keys()
        .map(|id| {
            let answer = answers.get(id).ok_or(EvaluationError::InvalidResponse)?;
            let probability = answer["noul"]
                .as_f64()
                .ok_or(EvaluationError::InvalidResponse)?;
            if answer["type"] != "noul"
                || !probability.is_finite()
                || !(0.0..=1.0).contains(&probability)
            {
                return Err(EvaluationError::InvalidResponse);
            }
            Ok(probability)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    use pretty_assertions::assert_eq as pretty_assert_eq;
    use serde_json::json;

    use super::*;

    fn server(status: u16, body: &str, delay: Duration) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let endpoint = format!(
            "http://{}/v1/systemone",
            listener.local_addr().expect("address")
        );
        let body = body.to_string();
        let handle = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("request");
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("read timeout");
            let mut bytes = Vec::new();
            let mut byte = [0];
            while !bytes.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).expect("request headers");
                bytes.push(byte[0]);
            }
            let headers = String::from_utf8(bytes)
                .expect("headers")
                .to_ascii_lowercase();
            assert!(headers.contains("authorization: bearer test-credential"));
            assert!(headers.starts_with("post /v1/systemone "));
            thread::sleep(delay);
            let response = format!(
                "HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nLocation: http://127.0.0.1:1/forbidden\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes());
        });
        (endpoint, handle)
    }

    #[test]
    fn real_transport_preserves_http_errors_and_does_not_follow_redirects() {
        for status in [302, 401, 429, 500, 529] {
            let (endpoint, handle) = server(status, "{}", Duration::ZERO);
            let result = JevClient::default().send_authenticated(
                &endpoint,
                "test-credential",
                &json!({}),
                Instant::now() + Duration::from_secs(2),
            );
            pretty_assert_eq!(result, Err(EvaluationError::Http(status)));
            handle.join().expect("server");
        }
    }

    #[test]
    fn real_transport_rejects_invalid_json_and_times_out() {
        for (body, delay, expected) in [
            ("not json", Duration::ZERO, EvaluationError::InvalidResponse),
            ("{}", Duration::from_millis(200), EvaluationError::Timeout),
        ] {
            let (endpoint, handle) = server(200, body, delay);
            let result = JevClient::default().send_authenticated(
                &endpoint,
                "test-credential",
                &json!({}),
                Instant::now() + Duration::from_millis(100),
            );
            pretty_assert_eq!(result, Err(expected));
            handle.join().expect("server");
        }
    }
}
