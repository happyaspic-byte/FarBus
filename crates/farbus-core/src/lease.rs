use crate::fingerprint::PeerFingerprint;
use farbus_protocol::DeviceId;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Default)]
pub struct LeaseBook {
    owners: HashMap<DeviceId, PeerFingerprint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LeaseError {
    #[error("device already leased")]
    AlreadyLeased { owner: PeerFingerprint },
    #[error("peer is not the lease owner")]
    NotOwner { owner: PeerFingerprint },
    #[error("device is not leased")]
    NotLeased,
}

impl LeaseBook {
    /// Grants or refreshes an exclusive lease for `peer`.
    ///
    /// # Errors
    ///
    /// Returns [`LeaseError::AlreadyLeased`] when another peer owns the device.
    pub fn acquire(&mut self, device: DeviceId, peer: PeerFingerprint) -> Result<(), LeaseError> {
        match self.owners.get(&device) {
            Some(owner) if *owner == peer => Ok(()),
            Some(owner) => Err(LeaseError::AlreadyLeased { owner: *owner }),
            None => {
                self.owners.insert(device, peer);
                Ok(())
            }
        }
    }

    /// Releases a lease owned by `peer`.
    ///
    /// # Errors
    ///
    /// Returns [`LeaseError::NotOwner`] or [`LeaseError::NotLeased`] when the caller
    /// cannot release the device.
    pub fn release(&mut self, device: DeviceId, peer: PeerFingerprint) -> Result<(), LeaseError> {
        match self.owners.get(&device) {
            Some(owner) if *owner == peer => {
                self.owners.remove(&device);
                Ok(())
            }
            Some(owner) => Err(LeaseError::NotOwner { owner: *owner }),
            None => Err(LeaseError::NotLeased),
        }
    }

    #[must_use]
    pub fn owner(&self, device: DeviceId) -> Option<PeerFingerprint> {
        self.owners.get(&device).copied()
    }
}
