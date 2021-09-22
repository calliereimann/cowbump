mod egui_ui;
mod entries_view;
mod thumbnail_loader;

use crate::{
    db::{local::LocalDb, EntryMap, TagSet, Uid},
    entry,
    filter_spec::FilterSpec,
    gui::egui_ui::EguiState,
};
use std::{collections::BTreeMap, error::Error};

use self::{egui_ui::Action, entries_view::EntriesView, thumbnail_loader::ThumbnailLoader};
use arboard::Clipboard;
use egui::{CtxRef, FontDefinitions, FontFamily, TextStyle};
use egui_sfml::SfEgui;
use sfml::{
    graphics::{
        Color, Font, IntRect, RectangleShape, RenderTarget, RenderWindow, Shape, Texture,
        Transformable,
    },
    window::{mouse, Event, Key, Style, VideoMode},
    SfBox,
};
use std::path::Path;

pub fn run(db: &mut LocalDb, no_save: &mut bool) -> Result<(), Box<dyn Error>> {
    let mut window = RenderWindow::new(
        VideoMode::desktop_mode(),
        "Cowbump",
        Style::NONE,
        &Default::default(),
    );
    window.set_vertical_sync_enabled(true);
    window.set_position((0, 0).into());
    let mut state = State::new(window.size().x, db);
    let mut on_screen_uids: Vec<entry::Id> = Vec::new();
    let mut selected_uids: Vec<entry::Id> = Default::default();
    let mut load_anim_rotation = 0.0;
    let mut sf_egui = SfEgui::new(&window);
    let egui_ctx = sf_egui.context();
    let font_defs = FontDefinitions {
        family_and_size: BTreeMap::from([
            (TextStyle::Small, (FontFamily::Proportional, 10.0)),
            (TextStyle::Body, (FontFamily::Proportional, 16.0)),
            (TextStyle::Button, (FontFamily::Proportional, 16.0)),
            (TextStyle::Heading, (FontFamily::Proportional, 20.0)),
            (TextStyle::Monospace, (FontFamily::Monospace, 13.0)),
        ]),
        ..Default::default()
    };
    egui_ctx.set_fonts(font_defs);
    while window.is_open() {
        if !sf_egui.context().wants_keyboard_input() {
            let scroll_speed = 8.0;
            if Key::DOWN.is_pressed() {
                state.y_offset += scroll_speed;
            } else if Key::UP.is_pressed() {
                state.y_offset -= scroll_speed;
                if state.y_offset < 0.0 {
                    state.y_offset = 0.0;
                }
            }
        }
        let mut esc_pressed = false;

        while let Some(event) = window.poll_event() {
            sf_egui.add_event(&event);
            match event {
                Event::Closed => window.close(),
                Event::KeyPressed { code, .. } => {
                    match code {
                        Key::ESCAPE => esc_pressed = true,
                        Key::HOME => {
                            if !sf_egui.context().wants_keyboard_input() {
                                state.y_offset = 0.0;
                            }
                        }
                        Key::END => {
                            if !sf_egui.context().wants_keyboard_input() {
                                // Align the bottom edge of the view with the bottom edge of the last row.
                                // To do align the camera with a bottom edge, we need to subtract the screen
                                // height from it.
                                let bottom_align = |y: f32| y - window.size().y as f32;
                                let n_pics = state.entries_view.filter(db, &state.filter).count();
                                let rows = n_pics as u32 / state.thumbnails_per_row as u32;
                                let bottom = (rows + 1) * state.thumbnail_size;
                                state.y_offset = bottom_align(bottom as f32);
                            }
                        }
                        Key::F1 => state.egui_state.top_bar ^= true,
                        _ => {}
                    }
                }
                _ => {}
            }
            handle_event_viewer(
                event,
                &mut state,
                &mut on_screen_uids,
                db,
                &mut selected_uids,
                &window,
                sf_egui.context(),
            );
        }
        state.begin_frame();
        sf_egui.do_frame(|ctx| {
            egui_ui::do_ui(&mut state, ctx, db);
        });
        if esc_pressed
            && !sf_egui.context().wants_keyboard_input()
            && !sf_egui.context().wants_pointer_input()
            && !state.just_closed_window_with_esc
        {
            selected_uids.clear()
        }
        if let Some(action) = &state.egui_state.action {
            match action {
                Action::Quit => window.close(),
                Action::QuitNoSave => {
                    *no_save = true;
                    window.close();
                }
                Action::SearchNext => search_next(&mut state, db),
                Action::SearchPrev => search_prev(&mut state, db),
                Action::SelectAll => select_all(&mut selected_uids, &state, db),
                Action::SelectNone => selected_uids.clear(),
                Action::SortEntries => state.entries_view.sort(db),
            }
        }
        recalc_on_screen_items(
            &mut on_screen_uids,
            db,
            &state.entries_view,
            &state,
            window.size().y,
        );
        window.clear(Color::BLACK);
        entries_view::draw_thumbnails(
            &mut state,
            &mut window,
            db,
            &on_screen_uids,
            &selected_uids,
            load_anim_rotation,
            !sf_egui.context().wants_pointer_input(),
        );
        if let Some(id) = state.highlight {
            let mut search_highlight = RectangleShape::with_size(
                (state.thumbnail_size as f32, state.thumbnail_size as f32).into(),
            );
            search_highlight.set_fill_color(Color::TRANSPARENT);
            search_highlight.set_outline_color(Color::RED);
            search_highlight.set_outline_thickness(-2.0);
            let y_of_item = id as f32 / state.thumbnails_per_row as f32;
            let pixel_y = y_of_item as f32 * state.thumbnail_size as f32;
            let highlight_offset = pixel_y - state.y_offset;
            let x_of_item = id as f32 % state.thumbnails_per_row as f32;
            search_highlight.set_position((
                x_of_item as f32 * state.thumbnail_size as f32,
                highlight_offset,
            ));
            window.draw(&search_highlight);
        }
        let mut tex_src = TexSrc {
            state: &mut state,
            db,
        };
        sf_egui.draw(&mut window, Some(&mut tex_src));
        window.display();
        load_anim_rotation += 2.0;
    }
    Ok(())
}

fn common_tags(ids: &[entry::Id], db: &LocalDb) -> TagSet {
    let mut set = TagSet::default();
    for &id in ids {
        for &tagid in &db.entries[&id].tags {
            set.insert(tagid);
        }
    }
    set
}

fn entry_at_xy(
    x: i32,
    y: i32,
    state: &State,
    on_screen_entries: &[entry::Id],
) -> Option<entry::Id> {
    let thumb_x = x as u32 / state.thumbnail_size;
    let rel_offset = state.y_offset as u32 % state.thumbnail_size;
    let thumb_y = (y as u32 + rel_offset) / state.thumbnail_size;
    let thumb_index = thumb_y * state.thumbnails_per_row as u32 + thumb_x;
    on_screen_entries.get(thumb_index as usize).copied()
}

fn handle_event_viewer(
    event: Event,
    state: &mut State,
    on_screen_entries: &mut Vec<entry::Id>,
    db: &mut LocalDb,
    selected_entries: &mut Vec<entry::Id>,
    window: &RenderWindow,
    ctx: &CtxRef,
) {
    match event {
        Event::MouseButtonPressed { button, x, y } => {
            if ctx.wants_pointer_input() {
                return;
            }
            let uid = match entry_at_xy(x, y, state, on_screen_entries) {
                Some(uid) => uid,
                None => return,
            };
            if button == mouse::Button::LEFT {
                if Key::LSHIFT.is_pressed() {
                    if selected_entries.contains(&uid) {
                        selected_entries.retain(|&rhs| rhs != uid);
                    } else {
                        selected_entries.push(uid);
                    }
                } else {
                    open_with_external(&[&db.entries[&uid].path]);
                }
            } else if button == mouse::Button::RIGHT {
                let vec = if selected_entries.contains(&uid) {
                    selected_entries.clone()
                } else {
                    vec![uid]
                };
                state.egui_state.add_entries_window(vec);
            }
        }
        Event::KeyPressed { code, ctrl, .. } => {
            if ctx.wants_keyboard_input() {
                return;
            }
            if code == Key::PAGEDOWN {
                state.y_offset += window.size().y as f32;
            } else if code == Key::PAGEUP {
                state.y_offset -= window.size().y as f32;
                if state.y_offset < 0.0 {
                    state.y_offset = 0.0;
                }
            } else if code == Key::ENTER {
                let mut paths: Vec<&Path> = Vec::new();
                for &uid in selected_entries.iter() {
                    paths.push(&db.entries[&uid].path);
                }
                if paths.is_empty() && state.filter.active() {
                    for uid in db.filter(&state.filter) {
                        paths.push(&db.entries[&uid].path);
                    }
                }
                paths.sort();
                open_with_external(&paths);
            } else if code == Key::A && ctrl {
                select_all(selected_entries, state, db);
            } else if code == Key::SLASH {
                state.search_edit = true;
            } else if code == Key::N {
                search_next(state, db);
            } else if code == Key::P {
                search_prev(state, db);
            } else if code == Key::F {
                state.filter_edit = true;
            } else if code == Key::C {
                use arboard::ImageData;
                let mp = window.mouse_position();
                let uid = match entry_at_xy(mp.x, mp.y, state, on_screen_entries) {
                    Some(uid) => uid,
                    None => return,
                };
                let imgpath = &db.entries[&uid].path;
                let buf = std::fs::read(imgpath).unwrap();
                let img = match image::load_from_memory(&buf) {
                    Ok(img) => img,
                    Err(e) => {
                        eprintln!("(clipboard) Image open error: {}", e);
                        return;
                    }
                };
                let rgba = img.to_rgba8();
                let img_data = ImageData {
                    width: rgba.width() as usize,
                    height: rgba.height() as usize,
                    bytes: rgba.into_raw().into(),
                };
                if let Err(e) = state.clipboard_ctx.set_image(img_data) {
                    eprintln!("Error setting clipboard: {}", e);
                }
            } else if code == Key::T {
                state.egui_state.toggle_tag_window();
            } else if code == Key::Q {
                state.egui_state.sequences_window.on ^= true;
            } else if code == Key::S {
                state.entries_view.sort(db);
            }
        }
        _ => {}
    }
}

fn select_all(selected_uids: &mut Vec<entry::Id>, state: &State, db: &LocalDb) {
    selected_uids.clear();
    for uid in db.filter(&state.filter) {
        selected_uids.push(uid);
    }
}

fn search_prev(state: &mut State, db: &mut LocalDb) {
    if state.search_cursor > 0 {
        state.search_cursor -= 1;
    }
    search_goto_cursor(state, db);
}

fn search_next(state: &mut State, db: &mut LocalDb) {
    state.search_cursor += 1;
    search_goto_cursor(state, db);
    if !state.search_success {
        state.search_cursor -= 1;
    }
}

fn find_nth(state: &State, db: &LocalDb, nth: usize) -> Option<Uid> {
    state
        .entries_view
        .filter(db, &state.filter)
        .enumerate()
        .filter(|(_, uid)| {
            let en = &db.entries[uid];
            en.spec_satisfied(&state.search_spec)
        })
        .map(|(i, _)| i as Uid)
        .nth(nth)
}

fn search_goto_cursor(state: &mut State, db: &LocalDb) {
    if let Some(uid) = find_nth(state, db, state.search_cursor) {
        state.highlight = Some(uid);
        state.search_success = true;
        let y_of_item = uid as f32 / state.thumbnails_per_row as f32;
        let y: f32 = (y_of_item * state.thumbnail_size as f32) as f32;
        state.y_offset = y;
    } else {
        state.search_success = false;
    }
}

fn recalc_on_screen_items(
    uids: &mut Vec<entry::Id>,
    db: &LocalDb,
    entries_view: &EntriesView,
    state: &State,
    window_height: u32,
) {
    uids.clear();
    let thumb_size = state.thumbnail_size;
    let mut thumbnails_per_column = (window_height / thumb_size) as u8;
    // Compensate for truncating division
    if window_height % thumb_size != 0 {
        thumbnails_per_column += 1;
    }
    // Since we can scroll, we can have another partially drawn frame per screen
    thumbnails_per_column += 1;
    let thumbnails_per_screen = (state.thumbnails_per_row * thumbnails_per_column) as usize;
    let row_offset = state.y_offset as u32 / thumb_size;
    let skip = row_offset * state.thumbnails_per_row as u32;
    uids.extend(
        entries_view
            .filter(db, &state.filter)
            .skip(skip as usize)
            .take(thumbnails_per_screen),
    );
}

type ThumbnailCache = EntryMap<Option<SfBox<Texture>>>;

struct State {
    thumbnails_per_row: u8,
    y_offset: f32,
    thumbnail_size: u32,
    filter: FilterSpec,
    loading_texture: SfBox<Texture>,
    error_texture: SfBox<Texture>,
    thumbnail_cache: ThumbnailCache,
    thumbnail_loader: ThumbnailLoader,
    font: SfBox<Font>,
    search_edit: bool,
    search_string: String,
    search_spec: FilterSpec,
    /// The same search can be used to seek multiple entries
    search_cursor: usize,
    search_success: bool,
    highlight: Option<Uid>,
    filter_edit: bool,
    filter_string: String,
    clipboard_ctx: Clipboard,
    egui_state: egui_ui::EguiState,
    entries_view: EntriesView,
    // We just closed window with esc, ignore the esc press outside of egui
    just_closed_window_with_esc: bool,
}

struct TexSrc<'state, 'db> {
    state: &'state mut State,
    db: &'db LocalDb,
}

impl<'state, 'db> egui_sfml::UserTexSource for TexSrc<'state, 'db> {
    fn get_texture(&mut self, id: u64) -> (f32, f32, &Texture) {
        let (_has, tex) = get_tex_for_entry(
            &self.state.thumbnail_cache,
            entry::Id(id),
            &self.state.error_texture,
            self.db,
            &mut self.state.thumbnail_loader,
            self.state.thumbnail_size,
            &self.state.loading_texture,
        );
        (tex.size().x as f32, tex.size().y as f32, tex)
    }
}

fn get_tex_for_entry<'t>(
    thumbnail_cache: &'t ThumbnailCache,
    id: entry::Id,
    error_texture: &'t Texture,
    db: &LocalDb,
    thumbnail_loader: &mut ThumbnailLoader,
    thumb_size: u32,
    loading_texture: &'t Texture,
) -> (bool, &'t Texture) {
    let (has_img, texture) = match thumbnail_cache.get(&id) {
        Some(opt_texture) => match *opt_texture {
            Some(ref tex) => (true, tex as &Texture),
            None => (false, error_texture),
        },
        None => {
            let entry = &db.entries[&id];
            thumbnail_loader.request(&entry.path, thumb_size, id);
            (false, loading_texture)
        }
    };
    (has_img, texture)
}

impl State {
    fn new(window_width: u32, db: &LocalDb) -> Self {
        let thumbnails_per_row = 5;
        let thumbnail_size = window_width / thumbnails_per_row as u32;
        let mut loading_texture = Texture::new().unwrap();
        let mut error_texture = Texture::new().unwrap();
        loading_texture
            .load_from_memory(include_bytes!("../../loading.png"), IntRect::default())
            .unwrap();
        error_texture
            .load_from_memory(include_bytes!("../../error.png"), IntRect::default())
            .unwrap();
        let mut egui_state = EguiState::default();
        egui_state.top_bar = true;
        Self {
            thumbnails_per_row,
            y_offset: 0.0,
            thumbnail_size,
            filter: FilterSpec::default(),
            loading_texture,
            error_texture,
            thumbnail_cache: Default::default(),
            thumbnail_loader: Default::default(),
            font: Font::from_memory(include_bytes!("../../Vera.ttf")).unwrap(),
            search_edit: false,
            search_string: String::new(),
            search_cursor: 0,
            search_success: false,
            highlight: None,
            filter_edit: false,
            filter_string: String::new(),
            clipboard_ctx: Clipboard::new().unwrap(),
            egui_state,
            entries_view: EntriesView::from_db(db),
            just_closed_window_with_esc: false,
            search_spec: FilterSpec::default(),
        }
    }
    fn wipe_search(&mut self) {
        self.search_cursor = 0;
        self.search_edit = false;
        self.search_success = false;
        self.highlight = None;
    }
    fn begin_frame(&mut self) {
        self.just_closed_window_with_esc = false;
        self.egui_state.begin_frame();
    }
}

fn open_with_external(paths: &[&Path]) {
    use std::process::Command;
    struct Cmd {
        command: Command,
        have_args: bool,
        exts: &'static [&'static str],
    }
    let mut general_cmd = Cmd {
        command: {
            let mut c = Command::new("feh");
            c.arg("--auto-rotate");
            c
        },
        exts: &[],
        have_args: false,
    };
    let mut commands = vec![
        Cmd {
            command: {
                let mut c = Command::new("mpv");
                c.arg("--ab-loop-a=0");
                c
            },
            exts: &["gif", "webm", "mov", "mp4", "m4v", "wmv", "avi"],
            have_args: false,
        },
        Cmd {
            command: Command::new("ruffle"),
            exts: &["swf"],
            have_args: false,
        },
    ];
    for path in paths {
        let mut cmd = &mut general_cmd;
        if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
            let lower = &ext.to_lowercase();
            for c in &mut commands {
                if c.exts.iter().any(|&e| e == lower) {
                    cmd = c;
                }
            }
        }
        cmd.command.arg(path);
        cmd.have_args = true;
    }
    if general_cmd.have_args {
        general_cmd.command.spawn().unwrap();
    }

    for mut c in commands {
        if c.have_args {
            c.command.spawn().unwrap();
        }
    }
}
