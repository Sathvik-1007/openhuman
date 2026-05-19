//! Per-session state for the voice assistant.
//!
//! Each session holds inbound PCM, outbound PCM, VAD state, conversation
//! history, and provider configuration. Sessions are keyed by a UUID and
//! stored in a process-wide registry.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use base64::Engine as _;
use log::debug;

use crate::openhuman::meet_agent::ops::{Vad, VadEvent};

use super::types::SessionState;

const LOG_PREFIX: &str = "[voice-assistant-session]";

/// Maximum inbound PCM buffer: 30 seconds @ 16 kHz.
const MAX_INBOUND_SAMPLES: usize = 16_000 * 30;

/// Maximum outbound PCM buffer: 30 seconds @ 16 kHz.
const MAX_OUTBOUND_SAMPLES: usize = 16_000 * 30;

/// Maximum conversation history entries.
const MAX_HISTORY: usize = 50;

/// A single voice assistant session.
pub struct VoiceAssistantSession {
    pub session_id: String,
    pub stt_provider: String,
    pub tts_provider: String,
    pub language: Option<String>,
    pub state: SessionState,
    pub turn_count: u32,
    pub inbound_samples: usize,
    pub outbound_samples: usize,

    /// Inbound PCM buffer (user speech, pre-STT).
    inbound_pcm: Vec<i16>,
    /// Outbound PCM buffer (assistant speech, post-TTS).
    outbound_pcm: Vec<i16>,
    /// VAD state machine.
    vad: Vad,
    /// Last transcript from STT.
    pub last_transcript: String,
    /// Last reply from LLM.
    pub last_reply: String,
    /// Conversation history for LLM context.
    pub history: Vec<ConversationTurn>,
}

/// A single conversation turn (user said X, assistant replied Y).
#[derive(Debug, Clone)]
pub struct ConversationTurn {
    pub user_text: String,
    pub assistant_text: String,
}

impl VoiceAssistantSession {
    pub fn new(
        session_id: String,
        stt_provider: String,
        tts_provider: String,
        language: Option<String>,
    ) -> Self {
        Self {
            session_id,
            stt_provider,
            tts_provider,
            language,
            state: SessionState::Listening,
            turn_count: 0,
            inbound_samples: 0,
            outbound_samples: 0,
            inbound_pcm: Vec::with_capacity(16_000), // 1s initial
            outbound_pcm: Vec::new(),
            vad: Vad::new(),
            last_transcript: String::new(),
            last_reply: String::new(),
            history: Vec::new(),
        }
    }

    /// Push inbound PCM samples and run VAD. Returns the VAD event.
    pub fn push_inbound_pcm(&mut self, samples: &[i16]) -> VadEvent {
        if samples.is_empty() {
            return VadEvent::Idle;
        }
        // Enforce max buffer size.
        let remaining = MAX_INBOUND_SAMPLES.saturating_sub(self.inbound_pcm.len());
        let to_push = samples.len().min(remaining);
        self.inbound_pcm.extend_from_slice(&samples[..to_push]);
        self.inbound_samples += to_push;

        self.vad.feed(samples)
    }

    /// Drain the inbound PCM buffer (called by brain after VAD fires).
    pub fn drain_inbound_pcm(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.inbound_pcm)
    }

    /// Enqueue outbound PCM (TTS output for the user to hear).
    pub fn enqueue_outbound_pcm(&mut self, samples: &[i16]) {
        let remaining = MAX_OUTBOUND_SAMPLES.saturating_sub(self.outbound_pcm.len());
        let to_push = samples.len().min(remaining);
        self.outbound_pcm.extend_from_slice(&samples[..to_push]);
        self.outbound_samples += to_push;
    }

    /// Poll outbound PCM. Returns (base64_pcm, utterance_done).
    pub fn poll_outbound(&mut self) -> (String, bool) {
        if self.outbound_pcm.is_empty() {
            return (String::new(), self.state != SessionState::Speaking);
        }
        let samples = std::mem::take(&mut self.outbound_pcm);
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        // Mark utterance done when buffer is fully drained and not processing.
        let done = self.state != SessionState::Processing;
        (b64, done)
    }

    /// Record a completed turn.
    pub fn record_turn(&mut self, user_text: &str, assistant_text: &str) {
        self.last_transcript = user_text.to_string();
        self.last_reply = assistant_text.to_string();
        self.turn_count += 1;
        self.history.push(ConversationTurn {
            user_text: user_text.to_string(),
            assistant_text: assistant_text.to_string(),
        });
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
    }

    /// Total seconds of inbound audio processed.
    pub fn listened_seconds(&self) -> f64 {
        self.inbound_samples as f64 / 16_000.0
    }

    /// Total seconds of outbound audio synthesized.
    pub fn spoken_seconds(&self) -> f64 {
        self.outbound_samples as f64 / 16_000.0
    }
}

// ---------------------------------------------------------------------------
// Process-wide session registry
// ---------------------------------------------------------------------------

static REGISTRY: OnceLock<Mutex<HashMap<String, VoiceAssistantSession>>> = OnceLock::new();

fn registry_map() -> &'static Mutex<HashMap<String, VoiceAssistantSession>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Public registry handle for RPC handlers.
pub struct SessionRegistry;

impl SessionRegistry {
    /// Start a new session. Returns error if session_id already exists.
    pub fn start(
        session_id: &str,
        stt_provider: &str,
        tts_provider: &str,
        language: Option<&str>,
    ) -> Result<(), String> {
        let mut map = registry_map()
            .lock()
            .map_err(|e| format!("{LOG_PREFIX} lock poisoned: {e}"))?;
        if map.contains_key(session_id) {
            // Idempotent restart: close old, open new.
            debug!("{LOG_PREFIX} restarting existing session={session_id}");
            map.remove(session_id);
        }
        let session = VoiceAssistantSession::new(
            session_id.to_string(),
            stt_provider.to_string(),
            tts_provider.to_string(),
            language.map(str::to_string),
        );
        map.insert(session_id.to_string(), session);
        debug!("{LOG_PREFIX} started session={session_id} stt={stt_provider} tts={tts_provider}");
        Ok(())
    }

    /// Execute a closure with mutable access to a session.
    pub fn with_session<F, R>(session_id: &str, f: F) -> Result<R, String>
    where
        F: FnOnce(&mut VoiceAssistantSession) -> R,
    {
        let mut map = registry_map()
            .lock()
            .map_err(|e| format!("{LOG_PREFIX} lock poisoned: {e}"))?;
        let session = map
            .get_mut(session_id)
            .ok_or_else(|| format!("{LOG_PREFIX} session not found: {session_id}"))?;
        Ok(f(session))
    }

    /// Stop and remove a session. Returns the final session state.
    pub fn stop(session_id: &str) -> Result<VoiceAssistantSession, String> {
        let mut map = registry_map()
            .lock()
            .map_err(|e| format!("{LOG_PREFIX} lock poisoned: {e}"))?;
        map.remove(session_id)
            .ok_or_else(|| format!("{LOG_PREFIX} session not found: {session_id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_lifecycle() {
        let id = format!("test-{}", uuid::Uuid::new_v4());
        SessionRegistry::start(&id, "whisper", "piper", Some("en")).unwrap();

        SessionRegistry::with_session(&id, |s| {
            assert_eq!(s.state, SessionState::Listening);
            assert_eq!(s.turn_count, 0);
        })
        .unwrap();

        let stopped = SessionRegistry::stop(&id).unwrap();
        assert_eq!(stopped.session_id, id);
    }

    #[test]
    fn push_pcm_triggers_vad() {
        let id = format!("test-vad-{}", uuid::Uuid::new_v4());
        SessionRegistry::start(&id, "whisper", "piper", None).unwrap();

        // Push silence — should get Idle or Silence.
        let event = SessionRegistry::with_session(&id, |s| {
            let silence = vec![0i16; 1600]; // 100ms silence
            s.push_inbound_pcm(&silence)
        })
        .unwrap();
        assert!(matches!(event, VadEvent::Idle | VadEvent::Silence));

        SessionRegistry::stop(&id).unwrap();
    }

    #[test]
    fn outbound_poll_returns_base64() {
        let id = format!("test-poll-{}", uuid::Uuid::new_v4());
        SessionRegistry::start(&id, "whisper", "piper", None).unwrap();

        SessionRegistry::with_session(&id, |s| {
            s.enqueue_outbound_pcm(&[100i16, 200, 300]);
            let (b64, _done) = s.poll_outbound();
            assert!(!b64.is_empty());
            // Second poll should be empty.
            let (b64_2, _) = s.poll_outbound();
            assert!(b64_2.is_empty());
        })
        .unwrap();

        SessionRegistry::stop(&id).unwrap();
    }

    #[test]
    fn record_turn_increments_counter() {
        let id = format!("test-turn-{}", uuid::Uuid::new_v4());
        SessionRegistry::start(&id, "whisper", "piper", None).unwrap();

        SessionRegistry::with_session(&id, |s| {
            s.record_turn("hello", "hi there");
            assert_eq!(s.turn_count, 1);
            assert_eq!(s.last_transcript, "hello");
            assert_eq!(s.last_reply, "hi there");
        })
        .unwrap();

        SessionRegistry::stop(&id).unwrap();
    }
}
