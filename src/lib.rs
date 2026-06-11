//! Patient Management System library crate.
//! Re-exports all modules for integration testing.

pub mod auth;
pub mod db;
pub mod errors;
pub mod time;
pub mod traits;

pub mod users;
pub mod appointments;
pub mod availability;
pub mod records;
pub mod billing;
pub mod audit;
pub mod dashboard;
