use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Discovered,
    Paired,
    Attached,
    Reconnecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEvent {
    Pair,
    Attach,
    Detach,
    ConnectionLost,
    ReconnectSucceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("invalid transition from {from:?} on {event:?}")]
pub struct TransitionError {
    pub from: ConnectionState,
    pub event: ConnectionEvent,
}

#[derive(Debug, Default)]
pub struct ConnectionMachine {
    state: ConnectionState,
}

impl ConnectionMachine {
    #[must_use]
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Applies one lifecycle event.
    ///
    /// # Errors
    ///
    /// Returns [`TransitionError`] when the event is illegal in the current state.
    pub fn apply(&mut self, event: ConnectionEvent) -> Result<(), TransitionError> {
        let next = match (self.state, event) {
            (ConnectionState::Discovered, ConnectionEvent::Pair)
            | (ConnectionState::Attached, ConnectionEvent::Detach) => ConnectionState::Paired,
            (ConnectionState::Paired, ConnectionEvent::Attach)
            | (ConnectionState::Reconnecting, ConnectionEvent::ReconnectSucceeded) => {
                ConnectionState::Attached
            }
            (ConnectionState::Attached, ConnectionEvent::ConnectionLost) => {
                ConnectionState::Reconnecting
            }
            (from, event) => {
                return Err(TransitionError { from, event });
            }
        };
        self.state = next;
        Ok(())
    }
}
