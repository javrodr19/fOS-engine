//! Intl
//!
//! Written in JavaScript (intl.js) and built the first time a script reads
//! the `Intl` global, so pages that never format anything pay nothing.

use crate::gc::Gc;
use crate::object::*;
use crate::value::Value;
use crate::vm::{JsResult, Vm};

use super::arg;

const SOURCE: &str = include_str!("intl.js");

pub(super) fn init(vm: &mut Vm) {
    let g = vm.global;
    vm.def_accessor(g, "Intl", get_intl, Some(set_intl));
}

/// The first read builds the object and replaces the accessor with it
fn get_intl(vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let intl = vm.eval(SOURCE)?;
    let g = vm.global;
    vm.def_value(g, "Intl", intl, PropFlags::HIDDEN);
    Ok(intl)
}

/// Scripts may replace it before reading it
fn set_intl(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let g = vm.global;
    vm.def_value(g, "Intl", arg(args, 0), PropFlags::HIDDEN);
    Ok(Value::UNDEFINED)
}
