//! Pure domain foundations for Hector; this crate owns no runtime, platform, storage, audio, or UI behavior.

mod effect;
mod event;
mod ids;
mod reducer;
mod state;
mod worker;

pub use effect::Effect;
pub use event::DomainEvent;
pub use ids::{AudioEpoch, GenerationEpoch, RequestId, SessionId, TurnId};
pub use reducer::{Transition, reduce};
pub use state::{ApplicationLifecycle, ApplicationState, GenerationState, SessionState, TurnState};
pub use worker::{WorkerFault, WorkerKind};
