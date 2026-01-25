
pub mod viaems {
    pub mod console {
        include!(concat!(env!("OUT_DIR"), "/viaems.console.rs"));
    }
}

pub use viaems::console::*;




