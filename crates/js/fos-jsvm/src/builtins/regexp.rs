//! RegExp (engine pending)

use crate::gc::Gc;
use crate::object::*;
use crate::value::Value;
use crate::vm::{JsResult, Vm};

pub(super) fn init(_vm: &mut Vm) {}

pub(crate) fn create(vm: &mut Vm, _pattern: &str, _flags: &str) -> JsResult<Value> {
    Err(vm.make_error(crate::vm::ErrorKind::Syntax, "regular expressions are not supported yet"))
}

pub(crate) fn is_regexp(_o: Gc<JsObject>) -> bool {
    false
}

pub(crate) fn is_global(_o: Gc<JsObject>) -> bool {
    false
}

pub(crate) fn regexp_split(vm: &mut Vm, _re: Gc<JsObject>, _s: Value, _limit: Value) -> JsResult<Value> {
    Err(vm.type_error("regular expressions are not supported yet"))
}

pub(crate) fn regexp_replace(vm: &mut Vm, _re: Gc<JsObject>, _s: Value, _r: Value) -> JsResult<Value> {
    Err(vm.type_error("regular expressions are not supported yet"))
}

pub(crate) fn string_match(vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Err(vm.type_error("regular expressions are not supported yet"))
}

pub(crate) fn string_match_all(vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Err(vm.type_error("regular expressions are not supported yet"))
}

pub(crate) fn string_search(vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Err(vm.type_error("regular expressions are not supported yet"))
}
