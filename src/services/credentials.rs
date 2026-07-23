use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::time::{timeout, Duration};

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelItem>,
}

#[derive(Debug, Deserialize)]
struct ModelItem {
    id: String,
}

#[derive(Debug, Serialize)]
struct ChatCheckRequest {
    model: String,
    messages: Vec<ChatCheckMessage>,
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ChatCheckMessage {
    role: String,
    content: String,
}

pub async fn check_deepgram(
    api_key: String,
    model: String,
    language: String,
) -> Result<String, String> {
    validate_key(&api_key, "your_deepgram_api_key_here")?;
    validate_non_empty(&model, "Deepgram model")?;

    let language = language.trim();
    let url = format!(
        "wss://api.deepgram.com/v1/listen?encoding=linear16&sample_rate=16000&channels=1&model={}&language={}",
        model.trim(),
        if language.is_empty() { "id" } else { language }
    );
    let request = build_deepgram_request(&url, api_key.trim())?;

    let (mut stream, _) = timeout(
        Duration::from_secs(8),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .map_err(|_| "Deepgram check timeout".to_string())?
    .map_err(|e| format!("Deepgram check failed: {e}"))?;
    let _ = stream.close(None).await;

    Ok(format!("OK: Deepgram model {} reachable", model.trim()))
}

pub async fn check_groq(api_key: String, model: String) -> Result<String, String> {
    validate_key(&api_key, "your_groq_api_key_here")?;
    validate_non_empty(&model, "Groq model")?;

    let client = reqwest::Client::new();
    let response = timeout(
        Duration::from_secs(8),
        client
            .get("https://api.groq.com/openai/v1/models")
            .bearer_auth(api_key.trim())
            .send(),
    )
    .await
    .map_err(|_| "Groq check timeout".to_string())?
    .map_err(|e| format!("Groq check failed: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format_http_error("Groq", status, &body));
    }

    if let Ok(models) = serde_json::from_str::<ModelsResponse>(&body) {
        let model = model.trim();
        if !models.data.iter().any(|item| item.id == model) {
            return Err(format!("Groq key valid, model {model} not listed"));
        }
    }

    Ok(format!("OK: Groq model {} reachable", model.trim()))
}

pub async fn check_assemblyai(
    api_key: String,
    model: String,
    _language: String,
) -> Result<String, String> {
    validate_key(&api_key, "your_assemblyai_api_key_here")?;
    validate_non_empty(&model, "AssemblyAI model")?;

    let client = reqwest::Client::new();
    let response = timeout(
        Duration::from_secs(8),
        client
            .get("https://api.assemblyai.com/v2/transcript/rms-ai-recorder-key-check")
            .header("Authorization", api_key.trim())
            .send(),
    )
    .await
    .map_err(|_| "AssemblyAI check timeout".to_string())?
    .map_err(|e| format!("AssemblyAI check failed: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status == StatusCode::UNAUTHORIZED {
        return Err(format_http_error("AssemblyAI", status, &body));
    }
    if status.is_server_error() {
        return Err(format_http_error("AssemblyAI", status, &body));
    }

    Ok(format!("OK: AssemblyAI model {} configured", model.trim()))
}

pub async fn check_openai_compatible(
    api_key: String,
    base_url: String,
    model: String,
) -> Result<String, String> {
    validate_key(&api_key, "your_openai_compatible_api_key_here")?;
    validate_non_empty(&base_url, "OpenAI-compatible base URL")?;
    validate_non_empty(&model, "Summary model")?;

    let endpoint = chat_completions_endpoint(&base_url);
    let request = ChatCheckRequest {
        model: model.trim().to_string(),
        messages: vec![ChatCheckMessage {
            role: "user".to_string(),
            content: "Reply OK".to_string(),
        }],
        max_tokens: 1,
    };

    let client = reqwest::Client::new();
    let response = timeout(
        Duration::from_secs(10),
        client
            .post(endpoint)
            .bearer_auth(api_key.trim())
            .json(&request)
            .send(),
    )
    .await
    .map_err(|_| "OpenAI-compatible check timeout".to_string())?
    .map_err(|e| format!("OpenAI-compatible check failed: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format_http_error("OpenAI-compatible", status, &body));
    }

    Ok(format!("OK: Summary model {} reachable", model.trim()))
}

fn validate_key(api_key: &str, placeholder: &str) -> Result<(), String> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() || trimmed == placeholder {
        Err("API key is empty or still a placeholder".to_string())
    } else {
        Ok(())
    }
}

fn validate_non_empty(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{label} is empty"))
    } else {
        Ok(())
    }
}

fn build_deepgram_request(
    url: &str,
    api_key: &str,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, String> {
    let mut request =
        tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(url)
            .map_err(|e| e.to_string())?;
    let auth_header = tokio_tungstenite::tungstenite::http::header::HeaderValue::from_str(
        &format!("Token {api_key}"),
    )
    .map_err(|e| e.to_string())?;
    request.headers_mut().insert("Authorization", auth_header);
    Ok(request)
}

fn chat_completions_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

fn format_http_error(provider: &str, status: StatusCode, body: &str) -> String {
    if body.trim().is_empty() {
        format!("{provider} check failed HTTP {status}")
    } else {
        format!("{provider} check failed HTTP {status}: {}", body.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_keys_are_rejected() {
        assert!(validate_key("", "placeholder").is_err());
        assert!(validate_key("placeholder", "placeholder").is_err());
    }

    #[test]
    fn chat_completions_endpoint_accepts_base_or_full_endpoint() {
        assert_eq!(
            chat_completions_endpoint("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_endpoint("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
    }
}
