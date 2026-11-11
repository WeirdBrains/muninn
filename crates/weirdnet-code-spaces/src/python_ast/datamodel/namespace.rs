//! Rust traits used to abstract Python data objects that are namespaces and contain symbols.
//! This includes Modules, Functions, Classes, and other objects.

use crate::python_ast::{Name, Object};
use std::collections::HashMap;

/*
pub enum PyPath {
    Name(Name),
    SubModule(Vec<PyPath>),
    Super,
}*/

pub trait NameSpace: Object {
    /// Returns the name of the object
    fn name(&self) -> Name;

    /// Returns the docstring, if any.
    fn doc(&self) -> Option<String>;

    /// Returns the namespace of the Object.
    fn dict<K, V>(&self) -> HashMap<K, V>;
}

#[cfg(python_ast_upstream_tests)]
mod tests {
    //use super::*;
}
