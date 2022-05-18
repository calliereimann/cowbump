use std::{ops::RangeInclusive, path::PathBuf};

use fnv::FnvHashMap;
use serde_derive::{Deserialize, Serialize};

use crate::db::Uid;

#[derive(Serialize, Deserialize)]
pub struct Preferences {
    pub open_last_coll_at_start: bool,
    pub applications: FnvHashMap<AppId, App>,
    pub associations: FnvHashMap<String, Option<AppId>>,
    #[serde(default = "ScrollWheelMultiplier::default")]
    pub scroll_wheel_multiplier: f32,
    #[serde(default = "UpDownArrowScrollSpeed::default")]
    pub arrow_key_scroll_speed: f32,
    #[serde(default)]
    pub style: Style,
}

impl Preferences {
    pub fn resolve_app(&self, name: &str) -> Option<AppId> {
        self.applications
            .iter()
            .find(|(_k, v)| v.name == name)
            .map(|(k, _v)| *k)
    }
}

#[derive(Serialize, Deserialize)]
pub struct Style {
    pub heading_size: f32,
    pub button_size: f32,
    pub body_size: f32,
    pub monospace_size: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            heading_size: 20.0,
            body_size: 16.0,
            button_size: 16.0,
            monospace_size: 14.0,
        }
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct App {
    pub name: String,
    pub path: PathBuf,
    /// A custom-parsed arguments string with `{}` placeholding for the entry list
    pub args_string: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Hash, PartialEq, Eq, Debug)]
pub struct AppId(pub Uid);

pub trait FloatPref {
    const DEFAULT: f32;
    const RANGE: RangeInclusive<f32>;
    const NAME: &'static str;
    fn default() -> f32 {
        Self::DEFAULT
    }
}

pub enum ScrollWheelMultiplier {}
impl FloatPref for ScrollWheelMultiplier {
    const DEFAULT: f32 = 64.0;
    const RANGE: RangeInclusive<f32> = 2.0..=512.0;
    const NAME: &'static str = "Mouse wheel scrolling multiplier";
}

pub enum UpDownArrowScrollSpeed {}
impl FloatPref for UpDownArrowScrollSpeed {
    const DEFAULT: f32 = 8.0;
    const RANGE: RangeInclusive<f32> = 1.0..=64.0;
    const NAME: &'static str = "Up/Down arrow key scroll speed";
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            open_last_coll_at_start: true,
            applications: Default::default(),
            associations: Default::default(),
            scroll_wheel_multiplier: ScrollWheelMultiplier::DEFAULT,
            arrow_key_scroll_speed: UpDownArrowScrollSpeed::DEFAULT,
            style: Default::default(),
        }
    }
}
