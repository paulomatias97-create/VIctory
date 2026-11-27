//! The Spotibuy phone, drawn on the CPU into a premultiplied RGBA picture
//! the size of the output, to be laid over the title's frame by the
//! presentation compositor.

pub mod draw;
pub mod fonts;
pub mod phone;

pub use fonts::Fonts;
pub use phone::{Phone, PhoneState, Track};
