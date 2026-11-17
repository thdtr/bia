//! Language servers: completions, errors as you type, go to definition,
//! hover, formatting and rename, from a program you choose per language in
//! `settings.json` (`"language_servers"`). BIA ships none and starts none
//! on its own.

pub mod client;
pub mod text;
pub mod types;

pub use client::{Client, Event, ServerId};
pub use text::{Encoding, Position, Range};
