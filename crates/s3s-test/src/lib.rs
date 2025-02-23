#![allow(
    clippy::missing_errors_doc, // TODO
    clippy::missing_panics_doc, // TODO
)]

mod error;
mod runner;
mod traits;

pub mod build;
pub mod cli;
pub mod report;
pub mod tcx;

pub use self::error::{Failed, Result};
pub use self::traits::*;
