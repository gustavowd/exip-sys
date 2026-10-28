#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals, dead_code)]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

mod decode;
mod encode;
pub use decode::{exi_to_xml, ExiError};
pub use encode::{xml_to_exi, EncodeError};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_loads() {
        unsafe { assert!(!sep_get_schema().is_null()); }
    }
}
