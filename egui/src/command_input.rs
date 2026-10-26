//! The command editor has a different Enter action than a terminal or a note.
use eframe::egui::{self, FontId};

pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\r").replace('\n', "\r")
}

pub fn show(ui: &mut egui::Ui, command: &mut String) -> bool {
    let response = ui.add(
        egui::TextEdit::multiline(command)
            .id(egui::Id::new("command-editor"))
            .return_key(egui::KeyboardShortcut::new(
                egui::Modifiers::SHIFT,
                egui::Key::Enter,
            ))
            .hint_text("type a command for the active session…")
            .desired_width(f32::INFINITY)
            .desired_rows(2)
            .font(FontId::monospace(12.5)),
    );
    response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enter(shift: bool) -> (bool, String) {
        let ctx = egui::Context::default();
        let mut text = "show version".to_string();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(ui, &mut text);
        });
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("command-editor")));
        let modifiers = egui::Modifiers {
            shift,
            ..Default::default()
        };
        let input = egui::RawInput {
            modifiers,
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let mut send = false;
        let _ = ctx.run_ui(input, |ui| {
            send = show(ui, &mut text);
        });
        (send, text)
    }

    #[test]
    fn enter_sends_without_inserting_a_line_break() {
        let (send, text) = enter(false);
        assert!(send);
        assert_eq!(text, "show version");
    }

    #[test]
    fn shift_enter_inserts_a_line_without_sending() {
        let (send, text) = enter(true);
        assert!(!send);
        assert!(text.contains('\n'));
    }
}
