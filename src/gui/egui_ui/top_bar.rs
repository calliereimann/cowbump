use egui_sfml::{
    egui::{self, Button, Color32, Context, Label, RichText, TopBottomPanel},
    sfml::graphics::RenderWindow,
};
use rfd::{FileDialog, MessageButtons, MessageDialog};

use crate::{
    application::Application,
    collection,
    gui::{
        native_dialog::{self, error},
        State,
    },
};

use super::{info_message, load_folder_window, prompt, Action, EguiState, PromptAction};

pub(super) fn do_frame(
    state: &mut State,
    egui_state: &mut EguiState,
    egui_ctx: &Context,
    app: &mut Application,
    win: &RenderWindow,
) -> anyhow::Result<()> {
    if !egui_state.top_bar {
        return Ok(());
    }
    let n_selected = state.selected_uids.len();
    let mut result = Ok(());
    TopBottomPanel::top("top_panel").show(egui_ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("🗁 Load folder").clicked() {
                    ui.close_menu();
                    if let Some(dir_path) = FileDialog::new().pick_folder() {
                        if let Some(id) = app.database.find_collection_by_path(&dir_path) {
                            let changes = match app.load_collection(id) {
                                Ok(changes) => changes,
                                Err(e) => {
                                    result = Err(e);
                                    return;
                                }
                            };
                            if !changes.empty() {
                                egui_state.changes_window.open(changes);
                            }
                            crate::gui::set_active_collection(
                                &mut state.entries_view,
                                app,
                                id,
                                &state.filter,
                            )
                            .unwrap();
                        } else {
                            load_folder_window::open(&mut egui_state.load_folder_window, dir_path);
                        }
                    }
                }
                if ui.button("↺ Reload folder").clicked() {
                    ui.close_menu();
                    let changes = match app.reload_active_collection() {
                        Ok(changes) => changes,
                        Err(e) => {
                            result = Err(e);
                            return;
                        }
                    };
                    if !changes.empty() {
                        egui_state.changes_window.open(changes);
                    }
                }
                if ui
                    .add_enabled(
                        app.active_collection.is_some(),
                        Button::new("🗀 Close folder"),
                    )
                    .clicked()
                {
                    if let Err(e) = app.switch_collection(None) {
                        result = Err(e);
                    }
                    ui.close_menu();
                }
                ui.add_enabled_ui(!app.database.recent.is_empty(), |ui| {
                    ui.menu_button("🕓 Recent", |ui| {
                        enum Action {
                            Open(collection::Id),
                            Remove(collection::Id),
                            None,
                        }
                        let mut action = Action::None;
                        for &id in app.database.recent.iter() {
                            ui.horizontal(|ui| {
                                if ui
                                    .button(&format!(
                                        "🗁 {}",
                                        &app.database.collections[&id].display()
                                    ))
                                    .clicked()
                                {
                                    action = Action::Open(id);
                                    ui.close_menu();
                                }
                                if ui.button("🗑").clicked() {
                                    action = Action::Remove(id);
                                }
                            });
                        }
                        match action {
                            Action::Open(id) => match app.load_collection(id) {
                                Ok(changes) => {
                                    if !changes.empty() {
                                        egui_state.changes_window.open(changes);
                                    }
                                    result = crate::gui::set_active_collection(
                                        &mut state.entries_view,
                                        app,
                                        id,
                                        &state.filter,
                                    );
                                }
                                Err(e) => {
                                    native_dialog::error("Error loading recent collection", e);
                                }
                            },
                            Action::Remove(id) => app.database.recent.remove(id),
                            Action::None => {}
                        }
                    });
                });
                ui.separator();
                if ui.button("⛃⬉ Create backup").clicked() {
                    ui.close_menu();
                    if let Some(path) = FileDialog::new()
                        .set_file_name("cowbump_backup.zip")
                        .save_file()
                    {
                        let result: anyhow::Result<()> = try {
                            app.save_active_collection()?;
                            app.database.save_backups(&path)?;
                        };
                        match result {
                            Ok(_) => {
                                info_message(
                                    &mut egui_state.info_messages,
                                    "Success",
                                    "Backup successfully created.",
                                );
                            }
                            Err(e) => {
                                info_message(
                                    &mut egui_state.info_messages,
                                    "Error",
                                    &e.to_string(),
                                );
                            }
                        }
                    }
                }
                if ui.button("⛃⬊ Restore backup").clicked() {
                    ui.close_menu();
                    let continue_ = MessageDialog::new()
                        .set_buttons(MessageButtons::OkCancel)
                        .set_title("Restore backup")
                        .set_description(
                            "This will replace all your current data with the backup. Continue?",
                        )
                        .show();
                    if continue_ {
                        if let Some(path) = FileDialog::new().pick_file() {
                            app.active_collection = None;
                            if let Err(e) = app.database.restore_backups_from(&path) {
                                native_dialog::error("Failed to restore backup", e);
                            } else {
                                MessageDialog::new().set_title("Backup restored!").show();
                            }
                        }
                    }
                }
                ui.separator();
                if ui.button("☰ Preferences").clicked() {
                    ui.close_menu();
                    egui_state.preferences_window.toggle();
                }
                ui.separator();
                if ui.button("🗙 Quit without saving").clicked() {
                    ui.close_menu();
                    prompt(
                        &mut egui_state.prompts,
                        "Quit without saving",
                        "Warning: All changes made this session will be lost.",
                        PromptAction::QuitNoSave,
                    )
                }
                ui.separator();
                if ui.button("⎆ Quit").clicked() {
                    egui_state.action = Some(Action::Quit);
                }
            });
            ui.menu_button("Actions", |ui| {
                let active_coll = app.active_collection.is_some();
                if ui
                    .add_enabled(active_coll, Button::new("🔍 Filter (F)"))
                    .clicked()
                {
                    ui.close_menu();
                    egui_state.filter_popup.on ^= true;
                }
                ui.separator();
                if ui
                    .add_enabled(active_coll, Button::new("🔍 Find (/)"))
                    .clicked()
                {
                    ui.close_menu();
                    egui_state.find_popup.on ^= true;
                }
                if ui
                    .add_enabled(active_coll, Button::new("⮫ Next result (N)"))
                    .clicked()
                {
                    ui.close_menu();
                    egui_state.action = Some(Action::FindNext);
                }
                if ui
                    .add_enabled(active_coll, Button::new("⮪ Previous result (P)"))
                    .clicked()
                {
                    ui.close_menu();
                    egui_state.action = Some(Action::FindPrev);
                }
                ui.separator();
                if ui
                    .add_enabled(active_coll, Button::new("☑ Select All (ctrl+A)"))
                    .clicked()
                {
                    ui.close_menu();
                    egui_state.action = Some(Action::SelectAll);
                }
                if ui
                    .add_enabled(active_coll, Button::new("☐ Select None (Esc)"))
                    .clicked()
                {
                    ui.close_menu();
                    egui_state.action = Some(Action::SelectNone);
                }
                ui.separator();
                if ui
                    .add_enabled(
                        n_selected > 0,
                        Button::new("Ｓ Open entries window for selected entries (F2)"),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    egui_state.action = Some(Action::OpenEntriesWindow);
                }
                ui.separator();
                if ui
                    .add_enabled(active_coll, Button::new("♻ Sort by filename (S)"))
                    .clicked()
                {
                    ui.close_menu();
                    egui_state.action = Some(Action::SortByPath);
                }
                if ui
                    .add_enabled(active_coll, Button::new("♻ Sort by id"))
                    .clicked()
                {
                    ui.close_menu();
                    egui_state.action = Some(Action::SortById);
                }
            });
            ui.menu_button("Collection", |ui| {
                if ui.button("＃ Tag list (T)").clicked() {
                    ui.close_menu();
                    egui_state.tag_window.toggle();
                }
                if ui.button("⬌ Sequences (Q)").clicked() {
                    ui.close_menu();
                    egui_state.sequences_window.on ^= true;
                }
                if ui.button("Changes").clicked() {
                    ui.close_menu();
                    egui_state.changes_window.open ^= true;
                }
                if ui.button("Tag specific applications").clicked() {
                    ui.close_menu();
                    egui_state.tag_specific_apps_window.open ^= true;
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About").clicked() {
                    ui.close_menu();
                    MessageDialog::new()
                        .set_description(&format!("Cowbump version {}", crate::VERSION))
                        .show();
                }
                ui.separator();
                ui.vertical_centered(|ui| {
                    ui.label("= Debug =");
                });
                if ui.button("Save screenshot (F11)").clicked() {
                    ui.close_menu();
                    crate::gui::util::take_and_save_screenshot(win);
                }
                if ui.button("Open data dir").clicked() {
                    ui.close_menu();
                    if let Err(e) = open::that(&app.database.data_dir) {
                        error("Error opening", e);
                    }
                }
                if ui.button("Debug window").clicked() {
                    ui.close_menu();
                    egui_state.debug_window.toggle();
                }
            });
            if n_selected > 0 {
                ui.separator();
                ui.add(Label::new(
                    RichText::new(format!("{} entries selected", n_selected)).color(Color32::GREEN),
                ));
                if ui
                    .add(Button::new(
                        RichText::new("(Click here (or Esc) to deselect)").color(Color32::YELLOW),
                    ))
                    .clicked()
                {
                    state.selected_uids.clear();
                }
            }
            ui.separator();
            ui.label("(F1 to toggle this panel)");
        });
    });
    result
}
