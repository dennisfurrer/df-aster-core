//! Live state kept from the streams: a symbol's book, from a snapshot and the diff stream.

pub mod book;

pub use book::{Applied, Book, Snapshot};
