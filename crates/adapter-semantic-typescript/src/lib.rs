// This crate intentionally uses QualityLevel until adapter-api is retired in Ticket 3.
#![allow(deprecated)]

pub mod adapter;
pub mod config;
pub mod error;
pub mod mapping;
pub mod process;
pub mod protocol;
pub mod runtime;
