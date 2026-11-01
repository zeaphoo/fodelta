//! Minimal color support. Explicit `--color` wins over `NO_COLOR`; `auto`
//! enables color only for a terminal.

use std::sync::atomic::{AtomicBool, Ordering};

use anstyle::{AnsiColor, Style};

static COLOR: AtomicBool = AtomicBool::new(false);

pub fn set_enabled(enabled: bool) {
    COLOR.store(enabled, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    COLOR.load(Ordering::Relaxed)
}

pub fn green() -> Style {
    Style::new().fg_color(Some(AnsiColor::Green.into()))
}

pub fn red() -> Style {
    Style::new().fg_color(Some(AnsiColor::Red.into()))
}

pub fn yellow() -> Style {
    Style::new().fg_color(Some(AnsiColor::Yellow.into()))
}

pub fn cyan() -> Style {
    Style::new().fg_color(Some(AnsiColor::Cyan.into()))
}

pub fn bold() -> Style {
    Style::new().bold()
}

pub fn paint(style: Style, text: &str) -> String {
    if !enabled() {
        return text.to_string();
    }
    format!("{}{}{}", style.render(), text, style.render_reset())
}

/// Color a unified diff by line kind.
pub fn colorize_patch(text: &str) -> String {
    if !enabled() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 64);
    for line in text.split_inclusive('\n') {
        let styled = if line.starts_with("@@") {
            paint(cyan(), line)
        } else if line.starts_with("+++") || line.starts_with("---") {
            paint(bold(), line)
        } else if line.starts_with('+') {
            paint(green(), line)
        } else if line.starts_with('-') {
            paint(red(), line)
        } else {
            line.to_string()
        };
        out.push_str(&styled);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_enabled_flag() {
        set_enabled(false);
        assert_eq!(paint(green(), "x"), "x");
        set_enabled(true);
        assert!(paint(green(), "x").contains('\u{1b}'));
        set_enabled(false);
    }

    #[test]
    fn colorizes_patch_lines() {
        set_enabled(true);
        let text = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n";
        let colored = colorize_patch(text);
        assert!(colored.contains("\u{1b}[32m"));
        assert!(colored.contains("\u{1b}[31m"));
        set_enabled(false);
    }
}
