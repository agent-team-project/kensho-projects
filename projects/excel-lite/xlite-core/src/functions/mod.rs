pub mod prelude;
pub mod registry;

pub use registry::{build_registry, Function, FunctionArgMode, FunctionEntry};

include!(concat!(env!("OUT_DIR"), "/function_modules.rs"));
