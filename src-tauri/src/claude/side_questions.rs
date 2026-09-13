//! Cancellation and ownership for operations outside the main turn.

use crate::harness::{SideAnswer, SideRefusal};
use crate::storage::native_side_questions::SideExchange;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

pub(crate) fn request(question: &str, history: &[SideExchange]) -> Value {
    let history: Vec<_> = history
        .iter()
        .map(|entry| {
            let mut value = json!({"question": entry.question, "response": entry.response});
            if let Some(refusal) = &entry.refusal {
                value["fallback_notice"] = json!(refusal.content);
            }
            value
        })
        .collect();
    json!({"subtype": "side_question", "question": question, "history": history})
}

pub(crate) fn answer(value: Value) -> Result<SideAnswer, String> {
    let response = value
        .get("response")
        .ok_or("Claude returned an invalid side-question response")?;
    if !response.is_null() && !response.is_string() {
        return Err("Claude returned an invalid side-question response".into());
    }
    let refusal = value
        .get("refusal_fallback")
        .map(|value| -> Result<SideRefusal, String> {
            let field = |name| {
                value
                    .get(name)
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| "Claude returned invalid refusal information".to_string())
            };
            Ok(SideRefusal {
                original_model: field("original_model")?,
                fallback_model: field("fallback_model")?,
                content: field("content")?,
            })
        })
        .transpose()?;
    Ok(SideAnswer {
        response: response.as_str().map(str::to_string),
        synthetic: value
            .get("synthetic")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        refusal,
    })
}

type Requests = Arc<Mutex<HashMap<String, (String, watch::Sender<bool>)>>>;

#[derive(Default)]
pub(crate) struct SideRequests(Requests);

pub(crate) struct SideRequest {
    requests: Requests,
    conversation: String,
    pub(crate) cancelled: watch::Receiver<bool>,
}

impl SideRequests {
    pub(crate) fn begin(&self, conversation: &str, request: &str) -> Result<SideRequest, String> {
        let mut requests = self.0.lock().map_err(|e| e.to_string())?;
        if requests.contains_key(conversation) {
            return Err("A side question is already running in this conversation".into());
        }
        let (sender, cancelled) = watch::channel(false);
        requests.insert(conversation.into(), (request.into(), sender));
        Ok(SideRequest {
            requests: self.0.clone(),
            conversation: conversation.into(),
            cancelled,
        })
    }

    pub(crate) fn cancel(&self, conversation: &str, request: &str) {
        if let Ok(requests) = self.0.lock() {
            if let Some((id, sender)) = requests.get(conversation) {
                if id == request {
                    sender.send_replace(true);
                }
            }
        }
    }

    pub(crate) fn active(&self, conversation: &str, request: &str) -> bool {
        self.0.lock().is_ok_and(|requests| {
            requests
                .get(conversation)
                .is_some_and(|(id, _)| id == request)
        })
    }
}

impl Drop for SideRequest {
    fn drop(&mut self) {
        if let Ok(mut requests) = self.requests.lock() {
            requests.remove(&self.conversation);
        }
    }
}

pub(crate) async fn cancelled(receiver: &mut watch::Receiver<bool>) {
    loop {
        if *receiver.borrow_and_update() {
            return;
        }
        if receiver.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_answers_keep_refusal_metadata_and_reject_invalid_payloads() {
        let parsed = answer(json!({"response": "Answer", "synthetic": false, "refusal_fallback": {"original_model": "first", "fallback_model": "second", "content": "Model changed"}})).unwrap();
        assert_eq!(parsed.response.as_deref(), Some("Answer"));
        assert_eq!(parsed.refusal.unwrap().content, "Model changed");
        assert!(answer(json!({"status": "started"})).is_err());
        assert!(answer(json!({"response": 123})).is_err());
        assert!(answer(json!({"response": null}))
            .unwrap()
            .response
            .is_none());
    }

    #[tokio::test]
    async fn cancellation_is_scoped_and_survives_cancelling_before_wait() {
        let requests = SideRequests::default();
        let mut first = requests.begin("one", "a").unwrap();
        let second = requests.begin("two", "b").unwrap();
        assert!(requests.begin("one", "c").is_err());
        requests.cancel("one", "old");
        assert!(!*first.cancelled.borrow());
        requests.cancel("one", "a");
        cancelled(&mut first.cancelled).await;
        assert!(!*second.cancelled.borrow());
        drop(first);
        assert!(!requests.active("one", "a"));
        assert!(requests.begin("one", "c").is_ok());
    }
}
