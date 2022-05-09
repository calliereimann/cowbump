use egui_sfml::egui::{Align2, Color32, Context, Key, Modifiers, TextEdit};

use crate::{
    collection::Collection,
    gui::{
        egui_ui::{
            entries_window::text_edit_cursor_set_to_end, tag_autocomplete::tag_autocomplete_popup,
        },
        State,
    },
};

use super::{tag_autocomplete::AcState, EguiState};

#[derive(Default)]
pub struct FilterPopup {
    pub on: bool,
    pub string: String,
    pub err_string: String,
    ac_state: AcState,
}

/// Returns whether filter state changed
pub(super) fn do_frame(
    state: &mut State,
    egui_state: &mut EguiState,
    egui_ctx: &Context,
    coll: &mut Collection,
) -> bool {
    let popup = &mut egui_state.filter_popup;
    let mut text_changed = false;
    let mut success = false;
    if popup.on {
        egui_sfml::egui::Window::new("Filter")
            .anchor(Align2::LEFT_TOP, [32.0, 32.0])
            .title_bar(false)
            .auto_sized()
            .show(egui_ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("filter");
                    let count = coll.filter(&state.filter).count();
                    let te_id = ui.make_persistent_id("text_edit_tag_popup");
                    let up_pressed = ui
                        .input_mut()
                        .consume_key(Modifiers::default(), Key::ArrowUp);
                    let down_pressed = ui
                        .input_mut()
                        .consume_key(Modifiers::default(), Key::ArrowDown);
                    let mut te = TextEdit::singleline(&mut popup.string)
                        .lock_focus(true)
                        .id(te_id);
                    if count == 0 {
                        te = te.text_color(Color32::RED);
                    }
                    if popup.ac_state.applied {
                        text_edit_cursor_set_to_end(ui, te_id);
                    }
                    let re = ui.add(te);
                    if tag_autocomplete_popup(
                        &mut popup.string,
                        &mut popup.ac_state,
                        coll,
                        ui,
                        &re,
                        up_pressed,
                        down_pressed,
                    ) {
                        state.wipe_search();
                        text_changed = true;
                    }
                    ui.label(&format!("{} results", count));
                    popup.string.make_ascii_lowercase();
                    let enter_pressed = egui_ctx.input().key_pressed(Key::Enter);
                    if enter_pressed || egui_ctx.input().key_pressed(Key::Escape) {
                        popup.on = false;
                    }
                    if re.changed() || text_changed || enter_pressed {
                        popup.err_string.clear();
                        match state.filter.parse_and_resolve(&popup.string, coll) {
                            Ok(()) => {
                                success = true;
                            }
                            Err(e) => {
                                popup.err_string = format!("Error: {}", e);
                                success = false;
                            }
                        }
                        popup.ac_state.input_changed = true;
                        state.wipe_search();
                        text_changed = true;
                    }
                    ui.memory().request_focus(re.id);
                });
                if !popup.err_string.is_empty() {
                    ui.label(&popup.err_string);
                }
            });
    }
    text_changed && success
}
