use farbus_core::{
    make_self_signed, make_server_config, serve_session, simulated_lab_devices, DeviceId,
    FarBusClient, LocalDevice, ServerState,
};
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::test]
async fn unexported_physical_device_cannot_be_attached() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (certs, key, server_fp) = make_self_signed("farbus.local").unwrap();
    let acceptor = make_server_config(certs, key).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let mut devices = simulated_lab_devices();
    devices.push(LocalDevice {
        info: farbus_core::DeviceInfo {
            id: DeviceId(99),
            bus_id: "9-9".into(),
            vid: 0x1234,
            pid: 0x5678,
            usb_class: 3,
            speed: farbus_core::UsbSpeed::Full,
            product: "Hidden HID".into(),
            exported: false,
        },
        backend: farbus_core::DeviceBackend::Host,
    });
    let state = Arc::new(ServerState::new("farbus-server".into(), server_fp, devices));
    let pin = state.pin.lock().await.pin.clone();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(stream).await.unwrap();
        let _ = serve_session(&mut tls, state).await;
    });

    let mut client = FarBusClient::connect(addr, server_fp).await.unwrap();
    client.pair(&pin, server_fp).await.unwrap();
    let list = client.devices().await.unwrap();
    assert!(list.devices.iter().all(|d| d.id != DeviceId(99)));
    let err = client.attach(DeviceId(99)).await.unwrap_err();
    assert!(matches!(err, farbus_core::ClientError::AttachRejected));
}
