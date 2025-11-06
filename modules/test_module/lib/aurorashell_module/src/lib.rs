pub mod register;
pub mod setup;
pub mod surface;
pub mod theme;
mod view;
pub mod widget;

pub use widget::Element;

use std::{error::Error, fmt};

pub use view::{CallbackType, ElementsMemoryArena, ViewFuncData, view_build_ui};

#[derive(Debug)]
pub struct MessageError(pub String);

impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for MessageError {}

pub mod macros {
    pub use macros::*;
}
