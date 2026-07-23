use chrono::Local;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: AssistantMessage,
}

#[derive(Debug, Deserialize)]
struct AssistantMessage {
    content: String,
}

pub async fn generate_summary_text(
    deepgram_transcript: String,
    whisper_transcript: String,
    assemblyai_transcript: String,
    system_prompt: String,
    api_key: String,
    base_url: String,
    model: String,
    output_path: String,
) -> Result<String, String> {
    validate_config(&api_key, &base_url, &model)?;

    if deepgram_transcript.trim().is_empty()
        && whisper_transcript.trim().is_empty()
        && assemblyai_transcript.trim().is_empty()
    {
        return Err("Summary skipped: all transcripts are empty".to_string());
    }

    let endpoint = chat_completions_endpoint(&base_url);
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("Summary HTTP client setup failed: {e}"))?;
    let request = ChatCompletionRequest {
        model,
        messages: build_messages(
            &deepgram_transcript,
            &whisper_transcript,
            &assemblyai_transcript,
            &system_prompt,
        ),
        temperature: 0.2,
        max_tokens: 2400,
    };

    let response = client
        .post(endpoint)
        .bearer_auth(api_key.trim())
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Summary request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format_summary_http_error(status, response).await);
    }

    let parsed: ChatCompletionResponse = response
        .json()
        .await
        .map_err(|e| format!("Summary response is invalid: {e}"))?;

    let summary_text = parsed
        .choices
        .first()
        .map(|choice| choice.message.content.trim().to_string())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| "Summary response is empty".to_string())?;

    fs::write(&output_path, &summary_text)
        .map_err(|e| format!("Failed to save summary to {output_path}: {e}"))?;

    Ok(summary_text)
}

fn validate_config(api_key: &str, base_url: &str, model: &str) -> Result<(), String> {
    if api_key.trim().is_empty() || api_key == "your_openai_compatible_api_key_here" {
        return Err("OPENAI_API_KEY is empty or invalid for summary".to_string());
    }

    if base_url.trim().is_empty() {
        return Err("OPENAI_BASE_URL is empty for summary".to_string());
    }

    if model.trim().is_empty() {
        return Err("OPENAI_MODEL is empty for summary".to_string());
    }

    Ok(())
}

fn chat_completions_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

fn build_messages(
    deepgram_transcript: &str,
    whisper_transcript: &str,
    assemblyai_transcript: &str,
    system_prompt: &str,
) -> Vec<ChatMessage> {
    let prompt = if system_prompt.trim().is_empty() {
        default_system_prompt()
    } else {
        system_prompt.trim()
    };

    vec![
        ChatMessage {
            role: "system".to_string(),
            content: prompt.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!(
                "Summary date: {}\n\nUse the transcripts below to create a clear, natural plain-text summary.\n\n<deepgram_realtime_transcript>\n{}\n</deepgram_realtime_transcript>\n\n<groq_whisper_final_transcript>\n{}\n</groq_whisper_final_transcript>\n\n<assemblyai_wav_final_transcript>\n{}\n</assemblyai_wav_final_transcript>",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                fallback_empty(deepgram_transcript),
                fallback_empty(whisper_transcript),
                fallback_empty(assemblyai_transcript)
            ),
        },
    ]
}

pub fn default_system_prompt() -> &'static str {
    SYSTEM_PROMPT
}

fn fallback_empty(value: &str) -> &str {
    if value.trim().is_empty() {
        "(empty / unavailable)"
    } else {
        value.trim()
    }
}

async fn format_summary_http_error(status: StatusCode, response: reqwest::Response) -> String {
    let body = response.text().await.unwrap_or_default();
    if body.trim().is_empty() {
        format!("Summary API error HTTP {status}")
    } else {
        format!("Summary API error HTTP {status}: {body}")
    }
}

const SYSTEM_PROMPT: &str = r#"You are a careful senior meeting-notes assistant.

Meeting context:
- Meetings usually have 2 main sessions:
  1. Session 1: Daily Updates
  2. Session 2: Work Review & Blocker Discussion
- The Daily Updates session contains multiple people reporting one by one. The number of reporters can vary, and the same person may speak more than once.
- The Work Review & Blocker Discussion session usually has an MC/facilitator asking about project, task, issue, blocker, decision, or follow-up progress.
- The input contains two transcripts: Deepgram realtime and final Groq/Whisper.

Source priority:
- Prioritize final Groq/Whisper for word accuracy because it comes from the final WAV file.
- Use Deepgram realtime as context comparison when Groq/Whisper is ambiguous or has missing parts.
- Do not invent names, numbers, ticket IDs, decisions, statuses, blockers, or deadlines that do not appear in the transcript.
- If a person/topic/ticket/status is unclear, write "needs confirmation" naturally in the related bullet.
- If one person or topic is discussed multiple times, merge it into one section when it is clearly the same item. If unsure, keep it separate.

Output style:
- Output MUST be plain text, not Markdown.
- Write the final summary in the same primary language as the transcript.
- If the transcript is mostly Indonesian, write the summary in Indonesian.
- If the transcript mixes Indonesian and English, keep the natural mixed style from the meeting.
- Do not translate names, ticket IDs, product names, or technical terms.
- Use natural section headings in the transcript language.
- Do not use generic openings or closings such as "Here is the summary...", "Overall...", or "That is all...".
- Style should feel like human internal notes: natural, concise, and not overly formal.
- Do not create empty sections or extra templates such as TL;DR, Action Items, Risks, Conclusion, or Transcript Quality.
- Avoid repetitive formal phrases unless they are the most natural phrasing.
- Use normal workplace terms when they appear in the transcript: follow up, continue, pairing, recheck, review MR, bug fix, retesting, still blocked.
- Each bullet should ideally contain one idea. Do not make bullets too long and do not repeat the same context.
- Do not invent action items. Write targets only when they are clearly mentioned.
- Use person names as plain lines in Session 1, without Markdown symbols.
- Use bullet "•" for main points.
- Use sub-bullet "o" for nested details in Session 2.
- Do not use Markdown characters such as #, ##, ###, **bold**, checkboxes, tables, or backticks.

Required output structure:

Session 1: Daily Updates

<Person Name or Reporter needs confirmation>
• Completed progress, written naturally and concisely.
• Issues/blockers/follow-ups mentioned, if any.
• Pairing/discussion/review/MR/testing done, if any.
• Today's target: plan or next step mentioned, if any.

Repeat the person-name format above for every detected reporter.
If today's target is not mentioned, do not force it; just write the available progress and blockers.

Session 2: Work Review & Blocker Discussion

• Issue/ticket/topic name: Summary of the main discussion, decision, status, or blocker.
  o Extra detail if available, such as flow differences, technical reasoning, discussion result, follow-up, or PIC.
  o If there is an explicit decision, write it naturally in the same bullet.

Repeat the bullet format above for every detected issue/ticket/topic.
If no ticket ID is mentioned but the topic is clear, use a short accurate topic name from the transcript.
If the MC/facilitator is clear, you may mention them in the opening bullet for Session 2. If unclear, do not create a special section.

Final rules:
- If Session 1 or Session 2 is not clearly detected, still create the heading and write one bullet: "• This session was not clearly detected / needs confirmation."
- Focus on meeting content, not transcript quality.
- Do not add information outside the transcript.
- Do not make the result sound like an article, formal report, or chatbot answer. Make it read like workplace notes ready to send to an internal chat.
"#;
