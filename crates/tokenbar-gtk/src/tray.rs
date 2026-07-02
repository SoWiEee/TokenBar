//! System-tray (StatusNotifierItem) integration via ksni.
//!
//! ksni runs the D-Bus service on its own thread; tray clicks and menu actions
//! arrive there and are forwarded to the GTK main thread over a channel. GNOME
//! shows tray items only with the AppIndicator/KStatusNotifierItem extension —
//! without a StatusNotifier host, `spawn` fails softly and the app runs as a
//! plain window.

use ksni::menu::StandardItem;
use ksni::{Category, MenuItem, Status, ToolTip, Tray};

/// Commands the tray thread sends to the GTK main loop.
#[derive(Debug, Clone, Copy)]
pub enum TrayCmd {
    Toggle,
    Quit,
}

pub struct TokenBarTray {
    tx: async_channel::Sender<TrayCmd>,
}

impl TokenBarTray {
    fn send(&self, cmd: TrayCmd) {
        let _ = self.tx.send_blocking(cmd);
    }
}

impl Tray for TokenBarTray {
    fn id(&self) -> String {
        "com.nyanako.tokenbar.gtk".into()
    }

    fn title(&self) -> String {
        "TokenBar".into()
    }

    fn category(&self) -> Category {
        Category::ApplicationStatus
    }

    fn status(&self) -> Status {
        Status::Active
    }

    fn icon_name(&self) -> String {
        // TODO: ship a branded (and eventually animated) icon; placeholder for now.
        "utilities-system-monitor".into()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_name: "utilities-system-monitor".into(),
            icon_pixmap: Vec::new(),
            title: "TokenBar".into(),
            description: "AI token usage monitor".into(),
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayCmd::Toggle);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Show TokenBar".into(),
                activate: Box::new(|t: &mut Self| t.send(TrayCmd::Toggle)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|t: &mut Self| t.send(TrayCmd::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Spawn the tray on its own thread. Keep the returned handle alive for the
/// process lifetime (dropping it removes the tray). Returns None when no
/// StatusNotifier host is available (e.g. GNOME without the AppIndicator
/// extension) — the app then runs window-only.
pub fn spawn(tx: async_channel::Sender<TrayCmd>) -> Option<ksni::blocking::Handle<TokenBarTray>> {
    use ksni::blocking::TrayMethods;
    match (TokenBarTray { tx }).spawn() {
        Ok(handle) => {
            eprintln!("tray: registered with StatusNotifier host");
            Some(handle)
        }
        Err(e) => {
            eprintln!("tray: StatusNotifier host unavailable ({e}); running window-only");
            None
        }
    }
}
