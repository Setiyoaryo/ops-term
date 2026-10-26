//! Share a UI wakeup with I/O workers without keeping the render loop busy.
use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
pub struct Wake(Arc<Mutex<Option<egui::Context>>>);

impl Wake {
    pub fn bind(&self, ctx: &egui::Context) {
        let mut target = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if target.is_none() {
            *target = Some(ctx.clone());
            ctx.request_repaint();
        }
    }

    pub fn request(&self) {
        if let Some(ctx) = self.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            ctx.request_repaint();
        }
    }

    pub fn output_ready(&self) {
        if let Some(ctx) = self.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}
