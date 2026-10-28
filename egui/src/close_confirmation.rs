//! Explicit confirmation before disconnecting sessions or quitting the app.
use eframe::egui::{self, Color32, Id, RichText, Stroke, Vec2};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Tab { id: u64, title: String },
    Application { sessions: usize },
    Restart { path: PathBuf, sessions: usize },
}

#[derive(Default)]
pub struct Confirmation {
    pending: Option<Target>,
    just_opened: bool,
    restore_focus: Option<Id>,
    restore_next_frame: Option<Id>,
    last_background_focus: Option<Id>,
    shown_frame: Option<u64>,
}

impl Confirmation {
    pub fn remember_background_focus(&mut self, ctx: &egui::Context) {
        if let Some(id) = ctx.memory(|memory| memory.focused()) {
            self.last_background_focus = Some(id);
        }
    }

    pub fn restore_previous_focus(&mut self, ctx: &egui::Context) {
        // egui's modal layer comes from the previous frame. Requesting focus
        // before that layer disappears is immediately undone by widget registration.
        if self.restore_next_frame.is_some()
            && ctx.memory(|memory| memory.top_modal_layer().is_some())
        {
            ctx.request_repaint();
            return;
        }
        if let Some(id) = self.restore_next_frame.take() {
            ctx.memory_mut(|memory| memory.request_focus(id));
        }
    }

    pub fn active(&self) -> bool {
        self.pending.is_some()
    }

    pub fn request(&mut self, target: Target) {
        if self.pending.as_ref() != Some(&target) {
            self.pending = Some(target);
            self.just_opened = true;
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Target> {
        let target = self.pending.clone()?;
        let opening = std::mem::take(&mut self.just_opened);
        if opening {
            self.shown_frame = Some(ctx.cumulative_frame_nr());
        }
        if opening && self.restore_focus.is_none() {
            self.restore_focus = self
                .last_background_focus
                .or_else(|| ctx.memory(|m| m.focused()));
        }
        let (heading, summary_label, summary, details, action) = match &target {
            Target::Tab { title, .. } => (
                "Close session?", "SESSION", title.clone(),
                "Running commands may be interrupted. This tab and its terminal output will be closed.",
                "Close session",
            ),
            Target::Application { sessions } => (
                "Close OpsTerm?", "ACTIVE SESSIONS", sessions.to_string(),
                "Your active sessions will be disconnected. Running commands may be interrupted.",
                "Close OpsTerm",
            ),
            Target::Restart { sessions, .. } => (
                "Restart OpsTerm?", "ACTIVE SESSIONS", sessions.to_string(),
                "Your active sessions will be interrupted during restart. Running commands may be interrupted.",
                "Restart",
            ),
        };
        let mut cancel = false;
        let confirm_shortcut =
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
        let mut confirm = false;
        let frame = egui::Frame::new()
            .fill(ctx.global_style().visuals.window_fill)
            .stroke(Stroke::new(
                1.0,
                ctx.global_style().visuals.window_stroke.color,
            ))
            .corner_radius(12)
            .inner_margin(24)
            .shadow(egui::epaint::Shadow {
                offset: [0, 12],
                blur: 32,
                spread: 0,
                color: Color32::from_black_alpha(100),
            });
        let modal = egui::Modal::new(Id::new("opsterm-close-confirmation"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(160))
            .show(ctx, |ui| {
                ui.set_width(408.0);
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                ui.horizontal(|ui| {
                    let (badge, _) =
                        ui.allocate_exact_size(Vec2::splat(36.0), egui::Sense::hover());
                    let warning = Color32::from_rgb(224, 132, 137);
                    let painter = ui.painter();
                    painter.rect_filled(
                        badge,
                        10,
                        Color32::from_rgba_unmultiplied(194, 72, 83, 24),
                    );
                    let center = badge.center();
                    let triangle = vec![
                        center + egui::vec2(0.0, -9.0),
                        center + egui::vec2(10.0, 8.0),
                        center + egui::vec2(-10.0, 8.0),
                        center + egui::vec2(0.0, -9.0),
                    ];
                    painter.add(egui::Shape::line(triangle, Stroke::new(1.5, warning)));
                    painter.line_segment(
                        [
                            center + egui::vec2(0.0, -3.0),
                            center + egui::vec2(0.0, 2.0),
                        ],
                        Stroke::new(1.5, warning),
                    );
                    painter.circle_filled(center + egui::vec2(0.0, 5.0), 0.9, warning);
                    ui.add_space(4.0);
                    ui.label(RichText::new(heading).size(21.0).strong());
                });
                ui.add_space(12.0);
                egui::Frame::new()
                    .fill(ui.visuals().extreme_bg_color)
                    .stroke(Stroke::new(
                        1.0,
                        ui.visuals().widgets.noninteractive.bg_stroke.color,
                    ))
                    .corner_radius(6)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(summary_label)
                                .size(10.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(&summary).monospace().size(14.0).strong(),
                            )
                            .wrap(),
                        );
                    });
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(details)
                            .size(14.0)
                            .color(ui.visuals().weak_text_color()),
                    )
                    .wrap(),
                );
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    confirm = ui
                        .scope(|ui| {
                            let widgets = &mut ui.visuals_mut().widgets;
                            for (visual, fill) in [
                                (&mut widgets.inactive, Color32::from_rgb(164, 55, 65)),
                                (&mut widgets.hovered, Color32::from_rgb(188, 66, 77)),
                                (&mut widgets.active, Color32::from_rgb(141, 43, 53)),
                            ] {
                                visual.weak_bg_fill = fill;
                                visual.bg_fill = fill;
                                visual.bg_stroke = Stroke::new(1.0, fill);
                                visual.fg_stroke.color = Color32::WHITE;
                                visual.corner_radius = 6.into();
                            }
                            ui.add_sized(
                                [124.0, 36.0],
                                egui::Button::new(
                                    RichText::new(action).size(14.0).color(Color32::WHITE),
                                ),
                            )
                            .on_hover_text("Ctrl+Enter to confirm")
                            .clicked()
                        })
                        .inner;
                    let response = ui.add_sized(
                        [92.0, 36.0],
                        egui::Button::new(RichText::new("Cancel").size(14.0)).corner_radius(6),
                    );
                    if opening {
                        response.request_focus();
                    }
                    cancel = response.clicked();
                });
            });
        // A plain Enter activates the focused button; Cancel receives initial focus.
        confirm |= modal.is_top_modal && confirm_shortcut;
        if cancel || modal.should_close() {
            self.pending = None;
            ctx.request_repaint();
            // Clear the previous modal layer in a second layout pass, before
            // the next native keyboard event arrives.
            ctx.request_discard("close confirmation dismissed");
            // Restore after the modal layer is gone, on the next frame.
            self.restore_next_frame = self.restore_focus.take();
            return None;
        }
        if confirm && self.shown_frame != Some(ctx.cumulative_frame_nr()) {
            self.pending = None;
            self.restore_focus = None;
            return Some(target);
        }
        None
    }
}

/// The dialog processes keys first; background forms must see no leftover input.
pub fn discard_background_keyboard(ctx: &egui::Context) {
    ctx.input_mut(|input| {
        input.events.retain(|event| {
            !matches!(
                event,
                egui::Event::Key { .. }
                    | egui::Event::Text(_)
                    | egui::Event::Paste(_)
                    | egui::Event::Copy
                    | egui::Event::Cut
                    | egui::Event::Ime(_)
            )
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        dialog: &mut Confirmation,
        key: Option<(egui::Key, egui::Modifiers)>,
    ) -> Option<Target> {
        let events = key
            .map(|(key, modifiers)| {
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }]
            })
            .unwrap_or_default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1180.0, 760.0),
            )),
            events,
            ..Default::default()
        };
        let mut result = None;
        let _ = ctx.run_ui(input, |_| result = dialog.show(ctx));
        result
    }
    fn tab() -> Target {
        Target::Tab {
            id: 42,
            title: "edge-01".into(),
        }
    }

    #[test]
    fn escape_cancels_without_confirming() {
        let ctx = egui::Context::default();
        let mut dialog = Confirmation::default();
        dialog.request(tab());
        assert!(frame(&ctx, &mut dialog, None).is_none());
        assert!(frame(
            &ctx,
            &mut dialog,
            Some((egui::Key::Escape, egui::Modifiers::NONE))
        )
        .is_none());
        assert!(!dialog.active());
    }

    #[test]
    fn enter_defaults_to_cancel_and_ctrl_enter_explicitly_confirms() {
        let ctx = egui::Context::default();
        let mut dialog = Confirmation::default();
        dialog.request(tab());
        frame(&ctx, &mut dialog, None);
        assert!(frame(
            &ctx,
            &mut dialog,
            Some((egui::Key::Enter, egui::Modifiers::NONE))
        )
        .is_none());
        assert!(!dialog.active());
        dialog.request(tab());
        frame(&ctx, &mut dialog, None);
        assert_eq!(
            frame(
                &ctx,
                &mut dialog,
                Some((egui::Key::Enter, egui::Modifiers::CTRL))
            ),
            Some(tab())
        );
        assert!(!dialog.active());
    }
}
