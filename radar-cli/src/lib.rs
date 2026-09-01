//! Library target for `radar-cli`.
//!
//! Every module lives here and the `radar` binary consumes them via
//! `radar_cli_lib::…` (O-22). Previously `main.rs` re-declared the shared
//! modules with `mod`, so they were compiled twice — once into the lib and
//! once into the bin — and the two copies could drift under `cfg`.
pub mod ai_provider;
pub mod api_client;
pub mod claude;
pub mod explain;
pub mod github;
pub mod jira;
pub mod policy;
pub mod postman;
pub mod register;
pub mod render;
pub mod scan;
pub mod test_gen;
