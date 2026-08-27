//! Shared `FarBus` state machines. Implementation follows tests.

mod fingerprint;
mod lease;
mod path;
mod state;

pub use fingerprint::{FingerprintError, PeerFingerprint};
pub use lease::{LeaseBook, LeaseError};
pub use path::connection_order;
pub use state::{ConnectionEvent, ConnectionMachine, ConnectionState, TransitionError};
