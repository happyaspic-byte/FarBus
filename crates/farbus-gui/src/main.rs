fn main() {
    #[cfg(windows)]
    if let Err(err) = farbus_gui::run() {
        eprintln!("FarBus GUI failed: {err}");
        std::process::exit(1);
    }

    #[cfg(not(windows))]
    {
        eprintln!("FarBus GUI is a Windows client. Use `farbus` on this platform.");
        std::process::exit(1);
    }
}
