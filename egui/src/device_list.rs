//! Virtualized device rows and the folder-aware device tree.
use super::{accent, border, card, muted, text_c};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Vec2};
use opsterm_core::domain::{Folder, SavedSession};
use std::collections::HashMap;

fn label(ui: &egui::Ui, rect: Rect, text: &str, size: f32, color: Color32) {
    let mut job =
        egui::text::LayoutJob::simple_singleline(text.into(), FontId::proportional(size), color);
    job.wrap.max_width = rect.width();
    job.wrap.max_rows = 1;
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let pos = Pos2::new(rect.left(), rect.center().y - galley.size().y / 2.0);
    ui.painter().with_clip_rect(rect).galley(pos, galley, color);
}

pub type FolderChoice = (String, Option<String>);

pub enum Action {
    Activate(usize),
    OpenNew(usize),
    Edit(usize),
    PickFolder(usize),
    NewSubfolder(String),
    RenameFolder(String),
    DeleteFolder(String),
}

pub fn show(
    ui: &mut egui::Ui,
    devices: &[SavedSession],
    indices: &[usize],
    active: Option<&str>,
    folder_choices: &[FolderChoice],
) -> Option<Action> {
    let mut action = None;
    egui::ScrollArea::vertical()
        .id_salt("device-list")
        .auto_shrink([false, false])
        .show_rows(ui, 52.0, indices.len(), |ui, range| {
            for row_index in range {
                let index = indices[row_index];
                let selected = active == Some(devices[index].id.as_str());
                if let Some(next) = row(ui, index, &devices[index], selected, folder_choices) {
                    action = Some(next);
                }
            }
        });
    action
}

pub fn row(
    ui: &mut egui::Ui,
    index: usize,
    device: &SavedSession,
    selected: bool,
    folder_choices: &[FolderChoice],
) -> Option<Action> {
    let mut action = None;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), egui::Sense::click());
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            4.0,
            if selected {
                card()
            } else {
                Color32::from_rgb(0x1C, 0x1C, 0x1C)
            },
        );
    }
    if selected {
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min + Vec2::new(0.0, 9.0), Vec2::new(2.0, 34.0)),
            1.0,
            accent(),
        );
    }
    let name_rect = Rect::from_min_size(
        rect.min + Vec2::new(10.0, 5.0),
        Vec2::new((rect.width() - 57.0).max(30.0), 22.0),
    );
    label(ui, name_rect, &device.name, 14.5, text_c());
    let detail = if device.protocol == "local" {
        "Local shell".to_string()
    } else {
        device.host.clone()
    };
    label(
        ui,
        Rect::from_min_size(
            rect.min + Vec2::new(10.0, 28.0),
            Vec2::new((rect.width() - 50.0).max(30.0), 18.0),
        ),
        &detail,
        13.0,
        muted(),
    );
    if response.clicked() {
        action = Some(Action::Activate(index));
    }
    response.context_menu(|ui| {
        if ui.button("Open in new tab").clicked() {
            action = Some(Action::OpenNew(index));
            ui.close();
        }
        if ui.button("Edit device").clicked() {
            action = Some(Action::Edit(index));
            ui.close();
        }
        if !folder_choices.is_empty() && ui.button("Move to folder…").clicked() {
            action = Some(Action::PickFolder(index));
            ui.close();
        }
    });
    let edit = ui.place(
        Rect::from_min_size(
            Pos2::new(rect.right() - 43.0, rect.top() + 12.0),
            Vec2::new(39.0, 26.0),
        ),
        egui::Button::new(
            egui::RichText::new("Edit")
                .font(FontId::proportional(11.5))
                .color(muted()),
        )
        .frame(false),
    );
    if edit.clicked() {
        action = Some(Action::Edit(index));
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(0.5, border()),
    );
    action
}

pub fn show_tree(
    ui: &mut egui::Ui,
    devices: &[SavedSession],
    indices: &[usize],
    folders: &[Folder],
    active: Option<&str>,
    folder_choices: &[FolderChoice],
) -> Option<Action> {
    let mut action = None;
    let mut child_folders: HashMap<Option<String>, Vec<&Folder>> = HashMap::new();
    for folder in folders {
        child_folders
            .entry(folder.parent_id.clone())
            .or_default()
            .push(folder);
    }
    let mut child_devices: HashMap<Option<String>, Vec<usize>> = HashMap::new();
    for &index in indices {
        child_devices
            .entry(devices[index].folder_id.clone())
            .or_default()
            .push(index);
    }
    let root_folders = child_folders.get(&None).cloned().unwrap_or_default();
    let root_devices = child_devices.get(&None).cloned().unwrap_or_default();
    egui::ScrollArea::vertical()
        .id_salt("device-tree")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for folder in root_folders {
                if let Some(next) = folder_node(
                    ui,
                    folder,
                    0,
                    devices,
                    &child_folders,
                    &child_devices,
                    active,
                    folder_choices,
                ) {
                    action = Some(next);
                }
            }
            for index in root_devices {
                let device = &devices[index];
                if let Some(next) = row(
                    ui,
                    index,
                    device,
                    active == Some(device.id.as_str()),
                    folder_choices,
                ) {
                    action = Some(next);
                }
            }
        });
    action
}

#[allow(clippy::too_many_arguments)]
fn folder_node(
    ui: &mut egui::Ui,
    folder: &Folder,
    depth: usize,
    devices: &[SavedSession],
    child_folders: &HashMap<Option<String>, Vec<&Folder>>,
    child_devices: &HashMap<Option<String>, Vec<usize>>,
    active: Option<&str>,
    folder_choices: &[FolderChoice],
) -> Option<Action> {
    let mut action = None;
    let folders = child_folders
        .get(&Some(folder.id.clone()))
        .cloned()
        .unwrap_or_default();
    let device_indices = child_devices
        .get(&Some(folder.id.clone()))
        .cloned()
        .unwrap_or_default();
    let count = folder_count(&folder.id, child_folders, child_devices);
    let title = format!("{}  ({count})", folder.name);
    let response = egui::CollapsingHeader::new(
        egui::RichText::new(title)
            .size(14.0)
            .strong()
            .color(text_c()),
    )
    .id_salt(("device-folder", folder.id.clone()))
    .default_open(depth < 2)
    .show(ui, |ui| {
        for child in folders {
            if let Some(next) = folder_node(
                ui,
                child,
                depth + 1,
                devices,
                child_folders,
                child_devices,
                active,
                folder_choices,
            ) {
                action = Some(next);
            }
        }
        for index in device_indices {
            let device = &devices[index];
            if let Some(next) = row(
                ui,
                index,
                device,
                active == Some(device.id.as_str()),
                folder_choices,
            ) {
                action = Some(next);
            }
        }
    });
    response.header_response.context_menu(|ui| {
        if ui.button("New subfolder").clicked() {
            action = Some(Action::NewSubfolder(folder.id.clone()));
            ui.close();
        }
        if ui.button("Rename folder").clicked() {
            action = Some(Action::RenameFolder(folder.id.clone()));
            ui.close();
        }
        if ui.button("Delete folder").clicked() {
            action = Some(Action::DeleteFolder(folder.id.clone()));
            ui.close();
        }
    });
    action
}

fn folder_count(
    id: &str,
    child_folders: &HashMap<Option<String>, Vec<&Folder>>,
    child_devices: &HashMap<Option<String>, Vec<usize>>,
) -> usize {
    let mut total = child_devices
        .get(&Some(id.to_string()))
        .map(Vec::len)
        .unwrap_or(0);
    if let Some(folders) = child_folders.get(&Some(id.to_string())) {
        for folder in folders {
            total += folder_count(&folder.id, child_folders, child_devices);
        }
    }
    total
}
