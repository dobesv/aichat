pub mod config;
pub mod dispatch;
pub mod executor;
pub mod matcher;
pub mod types;

#[allow(unused_imports)]
pub use config::{HookConfig, HooksConfig};
#[allow(unused_imports)]
pub use dispatch::dispatch_hooks;
#[allow(unused_imports)]
pub use executor::execute_command_hook;
pub use matcher::CompiledMatcher;
pub use types::*;
