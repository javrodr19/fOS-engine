//! JavaScript Built-in Objects
//!
//! Promise, Map, Set, Symbol, Proxy, BigInt, WeakRef, Atomics.

pub mod promise;
pub mod collections;
pub mod symbol;
pub mod proxy;
pub mod bigint;
pub mod weakref;
pub mod atomics;
pub mod top_level_await;

pub use promise::{JsPromise, PromiseState};
pub use collections::{JsMap, JsSet, JsWeakMap, JsWeakSet};
pub use symbol::{JsSymbol, WellKnownSymbol};
pub use proxy::{JsProxy, ProxyHandler};
pub use bigint::JsBigInt;
pub use weakref::{JsWeakRef, FinalizationRegistry};
pub use atomics::{SharedArrayBuffer, Atomics};
pub use top_level_await::{AsyncModule, TlaModuleGraph, TlaEvaluationHandle};
