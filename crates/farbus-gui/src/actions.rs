use crate::{DiscoveredServer, GuiDevice, GuiSession};
use farbus_core::{
    discovery, load_session, save_session, DeviceId, FarBusClient, Identity, PeerFingerprint,
    StoredSession,
};
use farbus_protocol::DeviceInfo;
use std::net::SocketAddr;
use std::time::Duration;

#[must_use]
pub fn restore_session() -> Option<GuiSession> {
    load_session(None).map(|saved| GuiSession {
        addr: saved.addr,
        fingerprint: saved.fingerprint,
    })
}

/// Discovers LAN servers for a few seconds.
///
/// # Errors
///
/// Returns a display string when UDP discovery fails.
pub async fn scan_servers() -> Result<Vec<DiscoveredServer>, String> {
    let found = discovery::collect(Duration::from_secs(3))
        .await
        .map_err(|err| err.to_string())?;
    Ok(found
        .into_iter()
        .map(|(fingerprint, addr, hostname)| DiscoveredServer {
            hostname,
            addr,
            fingerprint,
        })
        .collect())
}

/// Pairs with a server using the PIN from the GUI field. The PIN is not persisted.
///
/// # Errors
///
/// Returns a display string when TLS, pairing, or session save fails.
pub async fn pair_server(
    addr: SocketAddr,
    fingerprint: PeerFingerprint,
    pin: &str,
) -> Result<GuiSession, String> {
    if pin.len() != 6 || !pin.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("enter the 6-digit PIN from the server".into());
    }
    let mut client = farbus_core::happy_eyeballs_connect([addr], fingerprint)
        .await
        .map_err(|err| err.to_string())?;
    client
        .pair(pin, fingerprint)
        .await
        .map_err(|_| "pairing rejected".to_string())?;
    let token = client
        .auth_token()
        .ok_or_else(|| "server did not issue token".to_string())?;
    save_session(&StoredSession {
        addr,
        fingerprint,
        auth_token: token,
        client_secret: Some(client.identity_secret()),
    })
    .map_err(|err| err.to_string())?;
    Ok(GuiSession { addr, fingerprint })
}

async fn connect_session(session: GuiSession) -> Result<FarBusClient, String> {
    let saved = load_session(Some(session.fingerprint)).ok_or("run pairing first")?;
    let identity = saved
        .client_secret
        .map(Identity::from_secret)
        .ok_or("run pairing first")?;
    let client = FarBusClient::connect_with_identity(session.addr, saved.fingerprint, identity)
        .await
        .map_err(|err| err.to_string())?;
    Ok(client.with_auth_token(saved.auth_token))
}

fn to_gui_device(info: DeviceInfo, attached: Option<DeviceId>) -> GuiDevice {
    GuiDevice {
        attached: attached == Some(info.id),
        id: info.id,
        bus_id: info.bus_id,
        product: info.product,
        vid: info.vid,
        pid: info.pid,
    }
}

/// Lists exported devices for a paired session.
///
/// # Errors
///
/// Returns a display string when the session is missing or the server rejects the token.
pub async fn load_devices(
    session: GuiSession,
    attached: Option<DeviceId>,
) -> Result<Vec<GuiDevice>, String> {
    let client = connect_session(session).await?;
    let list = client.devices().await.map_err(|err| err.to_string())?;
    Ok(list
        .devices
        .into_iter()
        .map(|info| to_gui_device(info, attached))
        .collect())
}

/// Attaches a device and returns the live client plus USB/IP inventory.
///
/// # Errors
///
/// Returns a display string when attach is rejected or the listen address is not loopback.
pub async fn attach_device(
    session: GuiSession,
    device_id: DeviceId,
    usbip_listen: SocketAddr,
) -> Result<(String, FarBusClient, Vec<farbus_core::LocalDevice>), String> {
    if crate::loopback_usbip(usbip_listen).is_none() {
        return Err("USB/IP listener must use a loopback address".into());
    }
    let client = connect_session(session).await?;
    let attached = client
        .attach(device_id)
        .await
        .map_err(|_| "attach rejected".to_string())?;
    let devices = client
        .devices()
        .await
        .map_err(|err| err.to_string())?
        .devices
        .into_iter()
        .map(|info| farbus_core::LocalDevice {
            info,
            backend: farbus_core::DeviceBackend::Emulated,
        })
        .collect();
    Ok((attached.bus_id, client, devices))
}

/// Releases a previously attached device.
///
/// # Errors
///
/// Returns a display string when detach fails.
pub async fn detach_device(session: GuiSession, device_id: DeviceId) -> Result<(), String> {
    let client = connect_session(session).await?;
    client
        .detach(device_id)
        .await
        .map_err(|err| err.to_string())
}
