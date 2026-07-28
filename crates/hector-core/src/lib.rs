//! Pure domain foundations for Hector; this crate owns no runtime, platform, storage, audio, or UI behavior.

mod ids;

pub use ids::{AudioEpoch, GenerationEpoch, RequestId, SessionId, TurnId};
