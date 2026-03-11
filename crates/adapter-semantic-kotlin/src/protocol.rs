use serde::{Deserialize, Serialize};

/// A request sent to the Kotlin analysis bridge over stdin.
///
/// Uses the same Content-Length framed JSON protocol as the TypeScript
/// adapter for consistency across semantic adapter implementations.
#[derive(Debug, Clone, Serialize)]
pub struct KotlinRequest {
    pub seq: u32,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

impl KotlinRequest {
    /// Creates a new request with the given sequence number and command.
    pub fn new(seq: u32, command: &str) -> Self {
        Self {
            seq,
            msg_type: "request".to_string(),
            command: command.to_string(),
            arguments: None,
        }
    }

    /// Creates a new request with arguments.
    pub fn with_arguments(seq: u32, command: &str, arguments: serde_json::Value) -> Self {
        Self {
            seq,
            msg_type: "request".to_string(),
            command: command.to_string(),
            arguments: Some(arguments),
        }
    }

    /// Encodes the request as a wire-format message (Content-Length header + JSON body).
    pub fn encode(&self) -> Vec<u8> {
        let body = serde_json::to_string(self).expect("request serialization is infallible");
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
    }
}

/// A response received from the Kotlin analysis bridge over stdout.
#[derive(Debug, Clone, Deserialize)]
pub struct KotlinResponse {
    pub seq: u32,
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub request_seq: Option<u32>,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    #[serde(default)]
    pub message: Option<String>,
}

impl KotlinResponse {
    /// Returns true if this is a successful response to the given request sequence.
    pub fn is_success_for(&self, request_seq: u32) -> bool {
        self.msg_type == "response"
            && self.request_seq == Some(request_seq)
            && self.success == Some(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_without_arguments() {
        let req = KotlinRequest::new(1, "analyze");
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        assert_eq!(json["seq"], 1);
        assert_eq!(json["type"], "request");
        assert_eq!(json["command"], "analyze");
        assert!(json.get("arguments").is_none());
    }

    #[test]
    fn request_serializes_with_arguments() {
        let req = KotlinRequest::with_arguments(
            2,
            "analyze",
            serde_json::json!({"file": "/tmp/test.kt"}),
        );
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        assert_eq!(json["arguments"]["file"], "/tmp/test.kt");
    }

    #[test]
    fn request_encode_produces_content_length_header() {
        let req = KotlinRequest::new(1, "analyze");
        let encoded = req.encode();
        let encoded_str = String::from_utf8(encoded).unwrap();
        assert!(encoded_str.starts_with("Content-Length: "));
        assert!(encoded_str.contains("\r\n\r\n"));

        let parts: Vec<&str> = encoded_str.splitn(2, "\r\n\r\n").collect();
        let declared_len: usize = parts[0]
            .strip_prefix("Content-Length: ")
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(declared_len, parts[1].len());
    }

    #[test]
    fn response_deserializes_success() {
        let json =
            r#"{"seq":0,"type":"response","command":"analyze","request_seq":1,"success":true}"#;
        let resp: KotlinResponse = serde_json::from_str(json).unwrap();
        assert!(resp.is_success_for(1));
        assert!(!resp.is_success_for(2));
    }

    #[test]
    fn response_deserializes_failure() {
        let json = r#"{"seq":0,"type":"response","command":"analyze","request_seq":1,"success":false,"message":"file not found"}"#;
        let resp: KotlinResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.is_success_for(1));
        assert_eq!(resp.message.as_deref(), Some("file not found"));
    }
}
