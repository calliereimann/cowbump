#![feature(try_blocks, decl_macro, let_chains)]
#![windows_subsystem = "windows"]

mod application;
mod collection;
mod db;
mod entry;
mod filter_reqs;
mod folder_scan;
mod gui;
mod preferences;
mod sequence;
mod serialization;
pub mod set_ext;
mod tag;

const VERSION: &str = include_str!("../version.txt");

use crate::application::Application;

fn try_main() -> anyhow::Result<()> {
    let mut app = Application::new()?;
    gui::run(&mut app)
}

fn main() {
    if let Err(e) = try_main() {
        gui::native_dialog::error("Fatal runtime error", e)
    }
}
