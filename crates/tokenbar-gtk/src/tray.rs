//! System-tray (StatusNotifierItem) integration via ksni.
//!
//! ksni runs the D-Bus service on its own thread; tray clicks and menu actions
//! arrive there and are forwarded to the GTK main thread over a channel. GNOME
//! shows tray items only with the AppIndicator/KStatusNotifierItem extension —
//! without a StatusNotifier host, `spawn` fails softly and the app runs as a
//! plain window.

use ksni::menu::StandardItem;
use ksni::{Category, Icon, MenuItem, Status, ToolTip, Tray};

use crate::format::compact;

/// The signature TokenBar cat, embedded at build time (48×48).
const CAT_ICON_PNG: &[u8] = include_bytes!("../assets/cat/frame-00.png");

/// Commands the tray thread sends to the GTK main loop.
#[derive(Debug, Clone, Copy)]
pub enum TrayCmd {
    Toggle,
    Quit,
}

pub struct TokenBarTray {
    tx: async_channel::Sender<TrayCmd>,
    icon: Vec<Icon>,
    /// Tooltip description line (today's usage), updated via the handle.
    tooltip: String,
}

impl TokenBarTray {
    fn new(tx: async_channel::Sender<TrayCmd>) -> Self {
        Self {
            tx,
            icon: decode_png(CAT_ICON_PNG).into_iter().collect(),
            tooltip: "Loading today's usage…".to_string(),
        }
    }

    fn send(&self, cmd: TrayCmd) {
        let _ = self.tx.send_blocking(cmd);
    }

    /// Update the tooltip with today's totals (called via the tray handle).
    pub fn set_today(&mut self, tokens: i64, cost: f64) {
        self.tooltip = format!("Today: {} tokens · ${:.2}", compact(tokens), cost);
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
        // Empty → the pixmap below is used.
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icon.clone()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_name: String::new(),
            icon_pixmap: self.icon.clone(),
            title: "TokenBar".into(),
            description: self.tooltip.clone(),
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

/// Handle for updating the tray after spawn (e.g. today's usage into the tooltip).
pub type TrayHandle = ksni::blocking::Handle<TokenBarTray>;

/// Spawn the tray on its own thread. Keep the returned handle alive for the
/// process lifetime (dropping it removes the tray). Returns None when no
/// StatusNotifier host is available (e.g. GNOME without the AppIndicator
/// extension) — the app then runs window-only.
pub fn spawn(tx: async_channel::Sender<TrayCmd>) -> Option<TrayHandle> {
    use ksni::blocking::TrayMethods;
    match TokenBarTray::new(tx).spawn() {
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

/// Decode an 8-bit PNG into a ksni `Icon` (ARGB32, network byte order).
fn decode_png(bytes: &[u8]) -> Option<Icon> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let pixels = &buf[..info.buffer_size()];
    let mut data = Vec::with_capacity(info.width as usize * info.height as usize * 4);
    match info.color_type {
        png::ColorType::Rgba => {
            for px in pixels.chunks_exact(4) {
                data.extend_from_slice(&[px[3], px[0], px[1], px[2]]); // A,R,G,B
            }
        }
        png::ColorType::Rgb => {
            for px in pixels.chunks_exact(3) {
                data.extend_from_slice(&[255, px[0], px[1], px[2]]);
            }
        }
        _ => return None,
    }
    Some(Icon { width: info.width as i32, height: info.height as i32, data })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_cat_icon_decodes_to_48x48_argb() {
        let icon = decode_png(CAT_ICON_PNG).expect("cat icon should decode");
        assert_eq!(icon.width, 48);
        assert_eq!(icon.height, 48);
        assert_eq!(icon.data.len(), 48 * 48 * 4, "ARGB32 = 4 bytes/pixel");
    }
}
