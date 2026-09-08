//! MROP (MITOS Rich Output Protocol) sender.
//! Wraps RichWidgets in the OSC escape sequence expected by mitos-terminal.

use mitos_utils::ipc::RichWidget;
use std::io::{self, Write};

/// Sends a RichWidget to mitos-terminal for GPU-accelerated rendering.
/// This allows the shell to render clickable buttons and progress bars
/// inline with standard text output.
pub fn send_widget(widget: &RichWidget) -> io::Result<()> {
    let json =
        serde_json::to_string(widget).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // OSC 99 is the standard MITOS escape sequence for rich widgets
    // \x1b]99;{json}\x1b\\
    let mut stdout = io::stdout().lock();
    write!(stdout, "\x1b]99;{}\x1b\\", json)?;
    stdout.flush()
}

/// Helper to send a quick progress bar
pub fn send_progress(percent: f32, color: &str) -> io::Result<()> {
    send_widget(&RichWidget::Progress {
        percent,
        color: Some(color.to_string()),
    })
}

/// Helper to send a clickable action button
pub fn send_button(label: &str, command: &str) -> io::Result<()> {
    send_widget(&RichWidget::Button {
        label: label.to_string(),
        cmd: command.to_string(),
    })
}
