#![cfg(windows)]

use crate::actions;
use crate::{apply, GuiEvent, GuiPhase, GuiSession, GuiState};
use eframe::egui::{self, Color32, RichText, Vec2};
use farbus_core::DeviceId;
use std::sync::{mpsc, Arc};
use std::thread;
use tokio::runtime::Runtime;
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

struct FarBusApp {
    state: GuiState,
    runtime: Arc<Runtime>,
    events: mpsc::Receiver<GuiEvent>,
    events_tx: mpsc::Sender<GuiEvent>,
    attach_stop: Option<oneshot::Sender<()>>,
    attach_task: Option<JoinHandle<()>>,
    last_visible: bool,
    _tray: Option<TrayIcon>,
    show_item: MenuItem,
    hide_item: MenuItem,
    quit_item: MenuItem,
}

impl FarBusApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals.dark_mode = true;
        style.visuals.panel_fill = Color32::from_rgb(18, 18, 20);
        style.visuals.window_fill = Color32::from_rgb(24, 24, 27);
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(36, 36, 40);
        style.visuals.selection.bg_fill = Color32::from_rgb(46, 160, 67);
        cc.egui_ctx.set_style(style);

        let mut state = GuiState::new();
        if let Some(session) = actions::restore_session() {
            apply(
                &mut state,
                GuiEvent::PairSucceeded {
                    addr: session.addr,
                    fingerprint: session.fingerprint,
                },
            );
        }

        let (events_tx, events) = mpsc::channel();
        let runtime = Arc::new(Runtime::new().expect("tokio runtime"));
        if let Some(session) = state.session {
            spawn_load(&runtime, events_tx.clone(), session, None);
        }

        let show_item = MenuItem::new("Show FarBus", true, None);
        let hide_item = MenuItem::new("Hide", true, None);
        let quit_item = MenuItem::new("Quit", true, None);
        let menu = Menu::new();
        let _ = menu.append(&show_item);
        let _ = menu.append(&hide_item);
        let _ = menu.append(&quit_item);
        let tray = TrayIconBuilder::new()
            .with_tooltip("FarBus")
            .with_menu(Box::new(menu))
            .build()
            .ok();

        Self {
            state,
            runtime,
            events,
            events_tx,
            attach_stop: None,
            attach_task: None,
            last_visible: state.window_visible,
            _tray: tray,
            show_item,
            hide_item,
            quit_item,
        }
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.events.try_recv() {
            apply(&mut self.state, event);
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.show_item.id() {
                apply(&mut self.state, GuiEvent::TrayShown);
            } else if event.id == self.hide_item.id() {
                apply(&mut self.state, GuiEvent::TrayHidden);
            } else if event.id == self.quit_item.id() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        while TrayIconEvent::receiver().try_recv().is_ok() {
            apply(&mut self.state, GuiEvent::TrayShown);
        }
        if self.last_visible != self.state.window_visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(self.state.window_visible));
            self.last_visible = self.state.window_visible;
        }
        if self.state.take_focus_request() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    fn add_manual(&self) {
        let host = self.state.manual_host.clone();
        let fingerprint = self.state.manual_fingerprint.clone();
        let tx = self.events_tx.clone();
        if !fingerprint.is_empty() {
            apply_owned(&tx, GuiEvent::ManualServerAdded);
            return;
        }
        let runtime = Arc::clone(&self.runtime);
        thread::spawn(
            move || match runtime.block_on(actions::probe_server(&host)) {
                Ok(server) => {
                    let _ = tx.send(GuiEvent::ServersFound(vec![server.clone()]));
                    let _ = tx.send(GuiEvent::ServerSelected(server.fingerprint));
                }
                Err(err) => {
                    let _ = tx.send(GuiEvent::Failed(err));
                }
            },
        );
    }

    fn scan(&self) {
        let tx = self.events_tx.clone();
        let _ = tx.send(GuiEvent::ScanStarted);
        let runtime = Arc::clone(&self.runtime);
        thread::spawn(move || {
            let event = match runtime.block_on(actions::scan_servers()) {
                Ok(servers) => GuiEvent::ServersFound(servers),
                Err(err) => GuiEvent::Failed(err),
            };
            let _ = tx.send(event);
        });
    }

    fn pair(&self) {
        let tx = self.events_tx.clone();
        let _ = tx.send(GuiEvent::PairStarted);
        if self.state.pin.len() != 6 {
            return;
        }
        let Some(server) = self
            .state
            .servers
            .iter()
            .find(|server| Some(server.fingerprint) == self.state.selected)
            .cloned()
        else {
            let _ = tx.send(GuiEvent::Failed("select a server first".into()));
            return;
        };
        let pin = self.state.pin.clone();
        let runtime = Arc::clone(&self.runtime);
        thread::spawn(move || {
            match runtime.block_on(actions::pair_server(server.addr, server.fingerprint, &pin)) {
                Ok(session) => {
                    let _ = tx.send(GuiEvent::PairSucceeded {
                        addr: session.addr,
                        fingerprint: session.fingerprint,
                    });
                    spawn_load(&runtime, tx, session, None);
                }
                Err(err) => {
                    let _ = tx.send(GuiEvent::PairRejected(err));
                }
            }
        });
    }

    fn attach(&mut self, id: DeviceId) {
        let Some(session) = self.state.session else {
            return;
        };
        if let Some(stop) = self.attach_stop.take() {
            let _ = stop.send(());
        }
        let listen = self.state.usbip_listen;
        let tx = self.events_tx.clone();
        let runtime = Arc::clone(&self.runtime);
        let (stop_tx, stop_rx) = oneshot::channel();
        self.attach_stop = Some(stop_tx);
        self.attach_task = Some(runtime.spawn(async move {
            match actions::attach_device(session, id, listen).await {
                Ok((bus_id, client, devices)) => {
                    let _ = tx.send(GuiEvent::AttachSucceeded { id, bus_id });
                    let shared = Arc::new(Mutex::new(client));
                    let listen_addr = listen.to_string();
                    tokio::select! {
                        _ = farbus_core::serve_usbip_forward(&listen_addr, devices, shared) => {}
                        _ = stop_rx => {}
                    }
                }
                Err(err) => {
                    let _ = tx.send(GuiEvent::Failed(err));
                }
            }
        }));
    }

    fn detach(&mut self, id: DeviceId) {
        if let Some(stop) = self.attach_stop.take() {
            let _ = stop.send(());
        }
        let Some(session) = self.state.session else {
            return;
        };
        let tx = self.events_tx.clone();
        let runtime = Arc::clone(&self.runtime);
        thread::spawn(
            move || match runtime.block_on(actions::detach_device(session, id)) {
                Ok(()) => {
                    let _ = tx.send(GuiEvent::DetachSucceeded(id));
                    spawn_load(&runtime, tx, session, None);
                }
                Err(err) => {
                    let _ = tx.send(GuiEvent::Failed(err));
                }
            },
        );
    }
}

fn spawn_load(
    runtime: &Arc<Runtime>,
    tx: mpsc::Sender<GuiEvent>,
    session: GuiSession,
    attached: Option<DeviceId>,
) {
    let runtime = Arc::clone(runtime);
    thread::spawn(
        move || match runtime.block_on(actions::load_devices(session, attached)) {
            Ok(devices) => {
                let _ = tx.send(GuiEvent::DevicesLoaded(devices));
            }
            Err(err) => {
                let _ = tx.send(GuiEvent::Failed(err));
            }
        },
    );
}

fn apply_owned(tx: &std::sync::mpsc::Sender<GuiEvent>, event: GuiEvent) {
    let _ = tx.send(event);
}

fn short_fp(fp: &farbus_core::PeerFingerprint) -> String {
    let text = fp.to_string();
    format!("{}…{}", &text[..8], &text[text.len() - 8..])
}

impl eframe::App for FarBusApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(200));

        egui::TopBottomPanel::top("status").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("FarBus").strong());
                ui.label(
                    RichText::new(self.state.public_status())
                        .color(Color32::from_rgb(180, 180, 186)),
                );
            });
            ui.add_space(8.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Scan LAN").clicked() {
                    self.scan();
                }
                if self.state.phase == GuiPhase::Scanning {
                    ui.spinner();
                }
            });
            ui.label(
                RichText::new(
                    "LAN scan stays on the local broadcast domain. Over Tailscale, add ubuntu or 100.x.x.x:7420 — the GUI reads the fingerprint from TLS.",
                )
                .weak(),
            );
            ui.horizontal(|ui| {
                let mut host = self.state.manual_host.clone();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut host)
                            .hint_text("ubuntu or 100.x.x.x:7420")
                            .desired_width(220.0),
                    )
                    .changed()
                {
                    apply(&mut self.state, GuiEvent::ManualHostChanged(host));
                }
                let mut fp = self.state.manual_fingerprint.clone();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut fp)
                            .hint_text("optional fingerprint")
                            .desired_width(180.0),
                    )
                    .changed()
                {
                    apply(&mut self.state, GuiEvent::ManualFingerprintChanged(fp));
                }
                if ui.button("Add server").clicked() {
                    self.add_manual();
                }
            });
            ui.add_space(8.0);
            ui.label("Servers");
            egui::ScrollArea::vertical()
                .max_height(140.0)
                .show(ui, |ui| {
                    if self.state.servers.is_empty() {
                        ui.label(
                            RichText::new(
                                "No servers yet. Scan the LAN, or add a Tailscale host.",
                            )
                            .weak(),
                        );
                    }
                    let mut selected = None;
                    for server in &self.state.servers {
                        let checked = self.state.selected == Some(server.fingerprint);
                        if ui
                            .selectable_label(
                                checked,
                                format!(
                                    "{}  {}  {}",
                                    server.hostname,
                                    server.addr,
                                    short_fp(&server.fingerprint)
                                ),
                            )
                            .clicked()
                        {
                            selected = Some(server.fingerprint);
                        }
                    }
                    if let Some(fingerprint) = selected {
                        apply(&mut self.state, GuiEvent::ServerSelected(fingerprint));
                    }
                });

            ui.add_space(12.0);
            ui.label("Pairing PIN (from the Linux server terminal)");
            let mut pin = self.state.pin.clone();
            let pin_edit = egui::TextEdit::singleline(&mut pin)
                .password(true)
                .hint_text("6 digits")
                .desired_width(120.0);
            if ui.add(pin_edit).changed() {
                apply(&mut self.state, GuiEvent::PinChanged(pin));
            }
            if ui.button("Pair").clicked() {
                self.pair();
            }

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(8.0);
            ui.label("Exported devices");
            if self.state.devices.is_empty() {
                ui.label(RichText::new("Pair to list USB devices.").weak());
            }
            let mut attach = None;
            let mut detach = None;
            for device in &self.state.devices {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&device.product).strong());
                            ui.label(format!(
                                "[{}] {}  {:04x}:{:04x}",
                                device.id.0, device.bus_id, device.vid, device.pid
                            ));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if device.attached {
                                if ui.button("Detach").clicked() {
                                    detach = Some(device.id);
                                }
                                ui.colored_label(Color32::from_rgb(46, 160, 67), "Attached");
                            } else if ui.button("Attach").clicked() {
                                attach = Some(device.id);
                            }
                        });
                    });
                });
            }
            if let Some(id) = attach {
                self.attach(id);
            }
            if let Some(id) = detach {
                self.detach(id);
            }

            ui.add_space(16.0);
            ui.label(RichText::new("After Attach, run usbip-win2 against 127.0.0.1 only.").weak());
        });
    }
}

/// Starts the native Windows client window and tray icon.
///
/// # Errors
///
/// Returns an egui/eframe error when the window cannot be created.
pub fn run() -> Result<(), eframe::Error> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(Vec2::new(520.0, 640.0))
            .with_min_inner_size(Vec2::new(420.0, 480.0))
            .with_title("FarBus")
            .with_decorations(true),
        ..Default::default()
    };
    eframe::run_native(
        "FarBus",
        options,
        Box::new(|cc| Ok(Box::new(FarBusApp::new(cc)))),
    )
}
