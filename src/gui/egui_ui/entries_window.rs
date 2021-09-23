use std::{
    io::Read,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
};

use egui::{Button, Color32, ImageButton, Key, Label, Rgba, ScrollArea, TextureId};
use retain_mut::RetainMut;

use crate::{
    db::local::LocalDb,
    entry,
    gui::{common_tags, entries_view::EntriesView, State},
};

use super::sequences::SequenceWindow;

#[derive(Default)]
pub struct EntriesWindow {
    ids: Vec<entry::Id>,
    add_tag_buffer: String,
    rename_buffer: String,
    adding_tag: bool,
    renaming: bool,
    delete_confirm: bool,
    custom_command_prompt: bool,
    cmd_buffer: String,
    args_buffer: String,
    err_str: String,
    new_tags: Vec<String>,
    children: Vec<ChildWrapper>,
}

struct ChildWrapper {
    child: Child,
    exit_status: Option<ExitStatus>,
    stdout: String,
    stderr: String,
    name: String,
}

impl ChildWrapper {
    fn new(child: Child, name: String) -> Self {
        Self {
            child,
            exit_status: None,
            stdout: String::new(),
            stderr: String::new(),
            name,
        }
    }
}

impl EntriesWindow {
    pub fn new(ids: Vec<entry::Id>) -> Self {
        Self {
            ids,
            ..Default::default()
        }
    }
}

fn get_filename_from_path(path: &Path) -> String {
    path.components()
        .last()
        .unwrap()
        .as_os_str()
        .to_string_lossy()
        .to_string()
}

pub(super) fn do_frame(state: &mut State, db: &mut LocalDb, egui_ctx: &egui::CtxRef) {
    state.egui_state.entries_windows.retain_mut(|win| {
        let mut open = true;
        let n_entries = win.ids.len();
        let title = {
            if win.ids.len() == 1 {
                get_filename_from_path(&db.entries[&win.ids[0]].path)
            } else {
                format!("{} entries", n_entries)
            }
        };
        let esc_pressed = egui_ctx.input().key_pressed(Key::Escape);
        let mut close = esc_pressed;
        egui::Window::new(title)
            .open(&mut open)
            .show(egui_ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.set_max_width(512.0);
                        let n_visible_entries = n_entries.min(64);
                        for &id in win.ids.iter().take(n_visible_entries) {
                            ui.image(
                                TextureId::User(id.0),
                                (
                                    512.0 / n_visible_entries as f32,
                                    512.0 / n_visible_entries as f32,
                                ),
                            );
                        }
                    });
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            for tagid in common_tags(&win.ids, db) {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        let tag_name = match db.tags.get(&tagid) {
                                            Some(tag) => &tag.names[0],
                                            None => "<unknown tag>",
                                        };
                                        ui.add(
                                            Label::new(tag_name)
                                                .wrap(false)
                                                .background_color(Color32::from_rgb(50, 40, 45)),
                                        );
                                        if ui.button("x").clicked() {
                                            // TODO: This only works for 1 item windows
                                            db.entries
                                                .get_mut(&win.ids[0])
                                                .unwrap()
                                                .tags
                                                .retain(|&t| t != tagid);
                                        }
                                    })
                                });
                            }
                        });
                        let plus_re = ui.button("Add tags");
                        if plus_re.clicked() {
                            win.adding_tag ^= true;
                        }
                        if win.adding_tag {
                            let re = ui.text_edit_singleline(&mut win.add_tag_buffer);
                            re.request_focus();
                            if esc_pressed {
                                win.adding_tag = false;
                                win.add_tag_buffer.clear();
                                close = false;
                            }
                            if re.ctx.input().key_pressed(Key::Enter) {
                                let add_tag_buffer: &str = &win.add_tag_buffer;
                                let entry_uids: &[entry::Id] = &win.ids;
                                let tags = add_tag_buffer.split_whitespace();
                                for tag in tags {
                                    match db.resolve_tag(tag) {
                                        Some(tag_uid) => {
                                            db.add_tag_for_multi(entry_uids, tag_uid);
                                        }
                                        None => {
                                            win.new_tags.push(tag.to_owned());
                                        }
                                    }
                                }
                                win.add_tag_buffer.clear();
                                win.adding_tag = false;
                            }
                        }

                        if !win.new_tags.is_empty() {
                            ui.label(
                                "You added the following tags to the entry,\
                                 but they aren't present in the database: ",
                            );
                        }
                        win.new_tags.retain_mut(|tag| {
                            let mut retain = true;
                            ui.horizontal(|ui| {
                                ui.label(&tag[..]);
                                if ui.button("Add").clicked() {
                                    let tag_uid = db.add_new_tag_from_text(tag.to_owned());
                                    db.add_tag_for_multi(&win.ids, tag_uid);
                                    retain = false;
                                }
                                if ui.button("Cancel").clicked() {
                                    retain = false;
                                }
                            });
                            retain
                        });

                        if ui
                            .add(
                                Button::new("Rename")
                                    .wrap(false)
                                    .enabled(win.ids.len() == 1),
                            )
                            .clicked()
                        {
                            win.renaming ^= true;
                        }
                        if win.renaming {
                            let re = ui.text_edit_singleline(&mut win.rename_buffer);
                            if re.ctx.input().key_pressed(egui::Key::Enter) {
                                db.rename(win.ids[0], &win.rename_buffer);
                                win.renaming = false;
                            }
                            if re.lost_focus() {
                                win.renaming = false;
                                close = false;
                            }
                            ui.memory().request_focus(re.id);
                        }
                        if !win.delete_confirm {
                            if ui
                                .add(Button::new("Delete from disk").wrap(false))
                                .clicked()
                            {
                                win.delete_confirm ^= true;
                            }
                        } else {
                            let del_uids = &mut win.ids;
                            let del_len = del_uids.len();
                            let label_string = if del_len == 1 {
                                format!(
                                    "About to delete {}",
                                    db.entries[&del_uids[0]].path.display()
                                )
                            } else {
                                format!("About to delete {} entries", del_len)
                            };
                            ui.label(&label_string);
                            ui.horizontal(|ui| {
                                if ui.add(Button::new("Confirm").fill(Color32::RED)).clicked() {
                                    remove_entries(&mut state.entries_view, del_uids, db);
                                    win.delete_confirm = false;
                                    close = true;
                                }
                                if esc_pressed || ui.add(Button::new("Cancel")).clicked() {
                                    win.delete_confirm = false;
                                    close = false;
                                }
                            });
                        }
                        if ui.button("Add to sequence").clicked() {
                            state.egui_state.sequences_window.on = true;
                            state.egui_state.sequences_window.pick_mode = true;
                        }
                        if let Some(uid) = state.egui_state.sequences_window.pick_result {
                            db.add_entries_to_sequence(uid, &win.ids);
                            state.egui_state.sequences_window.pick_mode = false;
                            state.egui_state.sequences_window.pick_result = None;
                        }
                        if ui
                            .add(Button::new("Run custom command").wrap(false))
                            .clicked()
                        {
                            win.custom_command_prompt ^= true;
                        }
                        if win.custom_command_prompt {
                            if esc_pressed {
                                win.custom_command_prompt = false;
                                close = false;
                            }
                            ui.label("Command");
                            let re = ui.text_edit_singleline(&mut win.cmd_buffer);
                            ui.label("Args (use {} for entry path, or leave empty)");
                            ui.text_edit_singleline(&mut win.args_buffer);
                            if re.ctx.input().key_pressed(egui::Key::Enter) {
                                let mut cmd = Command::new(&win.cmd_buffer);
                                cmd.stderr(Stdio::piped());
                                cmd.stdin(Stdio::piped());
                                cmd.stdout(Stdio::piped());
                                for uid in &win.ids {
                                    let en = &db.entries[uid];
                                    for arg in win.args_buffer.split_whitespace() {
                                        if arg == "{}" {
                                            cmd.arg(&en.path);
                                        } else {
                                            cmd.arg(arg);
                                        }
                                    }
                                    if win.args_buffer.is_empty() {
                                        cmd.arg(&en.path);
                                    }
                                }
                                match cmd.spawn() {
                                    Ok(child) => {
                                        win.err_str.clear();
                                        win.custom_command_prompt = false;
                                        win.children
                                            .push(ChildWrapper::new(child, win.cmd_buffer.clone()));
                                    }
                                    Err(e) => win.err_str = e.to_string(),
                                }
                            }
                            if !win.err_str.is_empty() {
                                ui.add(
                                    Label::new(format!("Error: {}", win.err_str))
                                        .text_color(Rgba::RED),
                                );
                            }
                        }
                        win.children.retain_mut(|c_wrap| {
                            ui.separator();
                            ui.heading(&c_wrap.name);
                            let mut retain = true;
                            if let Some(status) = c_wrap.exit_status {
                                ui.label("stdout:");
                                ui.code(&c_wrap.stdout);
                                ui.label("stderr:");
                                ui.code(&c_wrap.stderr);
                                let exit_code_msg = match status.code() {
                                    Some(code) => code.to_string(),
                                    None => "<terminated>".to_string(),
                                };
                                ui.label(&format!(
                                    "Exit code: {} ({})",
                                    exit_code_msg,
                                    status.success()
                                ));
                                return !ui.button("x").clicked();
                            }
                            let mut clicked = false;
                            ui.horizontal(|ui| {
                                clicked = ui.button("x").clicked();
                                ui.label(&format!("[running] ({})", c_wrap.child.id()));
                            });
                            if clicked {
                                let _ = c_wrap.child.kill();
                                return false;
                            }
                            match c_wrap.child.try_wait() {
                                Ok(opt_status) => {
                                    c_wrap.exit_status = opt_status;
                                    if let Some(status) = opt_status {
                                        if !status.success() {
                                            if let Some(stdout) = &mut c_wrap.child.stdout {
                                                let mut buf = String::new();
                                                let _ = stdout.read_to_string(&mut buf);
                                                c_wrap.stdout = buf;
                                            }
                                            if let Some(stderr) = &mut c_wrap.child.stderr {
                                                let mut buf = String::new();
                                                let _ = stderr.read_to_string(&mut buf);
                                                c_wrap.stderr = buf;
                                            }
                                        } else {
                                            retain = false;
                                        }
                                    }
                                }
                                Err(e) => {
                                    win.err_str = e.to_string();
                                }
                            }
                            retain
                        })
                    });
                });
                let seqs = db.find_related_sequences(&win.ids);
                if !seqs.is_empty() {
                    ui.separator();
                    ui.heading("Related sequences");
                    for seq_id in seqs {
                        let seq = &db.sequences[&seq_id];
                        ui.label(&seq.name);
                        {}
                        ui.horizontal(|ui| {
                            ScrollArea::horizontal().show(ui, |ui| {
                                for &img_id in seq.entries.iter() {
                                    let img_but =
                                        ImageButton::new(TextureId::User(img_id.0), (128., 128.));
                                    if ui.add(img_but).clicked() {
                                        state
                                            .egui_state
                                            .sequence_windows
                                            .push(SequenceWindow::new(seq_id, Some(img_id)));
                                    }
                                }
                            });
                        });
                    }
                }
            });
        if close {
            state.egui_state.just_closed_window_with_esc = true;
            open = false;
        }
        open
    });
}

fn remove_entries(view: &mut EntriesView, entries: &mut Vec<entry::Id>, db: &mut LocalDb) {
    for uid in entries.drain(..) {
        let path = &db.entries[&uid].path;
        if let Err(e) = std::fs::remove_file(path) {
            eprintln!("Remove error: {}", e);
        }
        view.delete(uid);
        db.entries.remove(&uid);
    }
}
