//! Cold path utilities for binary size optimization.
//!
//! Use these to mark rarely-executed code paths, allowing the compiler
//! to optimize them differently and reduce binary size.
//!
//! # Example
//! ```rust
//! use fos_engine::cold_path;
//!
//! fn process_data(data: &[u8]) -> Result<(), String> {
//!     if data.is_empty() {
//!         return cold_path(|| Err("Empty data".to_string()));
//!     }
//!     Ok(())
//! }
//! ```

/// Mark a closure as cold (rarely executed).
/// 
/// This helps the compiler optimize for the hot path by moving
/// cold code out of line and reducing instruction cache pressure.
///
/// # Example
/// ```rust
/// use fos_engine::cold_path;
///
/// // Use cold_path for error handling or rare conditions
/// fn validate(value: i32) -> Result<i32, &'static str> {
///     if value < 0 {
///         cold_path(|| Err("negative value"))
///     } else {
///         Ok(value)
///     }
/// }
/// ```
#[inline(always)]
#[cold]
pub fn cold_path<F: FnOnce() -> R, R>(f: F) -> R {
    f()
}

/// Format an error message without bloating binary size.
///
/// Uses a single format point instead of spreading format calls
/// throughout the codebase, which reduces binary size by avoiding
/// format string duplication.
///
/// # Example
/// ```rust
/// use fos_engine::format_error;
///
/// let msg = format_error("connection failed", "NetworkModule");
/// // Returns: "NetworkModule: connection failed"
/// ```
#[inline(never)]
#[cold]
pub fn format_error(msg: &str, context: &str) -> String {
    format!("{}: {}", context, msg)
}

/// Panic with minimal binary bloat.
///
/// This function is marked `#[cold]` and `#[inline(never)]` to ensure
/// panic paths don't bloat the binary with inlined panic machinery.
///
/// # Panics
/// Always panics with the provided message.
#[inline(never)]
#[cold]
pub fn cold_panic(msg: &str) -> ! {
    panic!("{}", msg)
}

/// Unreachable with minimal binary bloat.
///
/// Use this instead of `unreachable!()` for code paths that should
/// never be reached to minimize binary size impact.
///
/// # Safety
/// This function will panic if ever called. Only use in truly
/// unreachable code paths.
#[inline(never)]
#[cold]
pub fn cold_unreachable() -> ! {
    panic!("entered unreachable code")
}

/// Debug assertion that compiles to nothing in release builds.
///
/// Unlike `debug_assert!`, this doesn't leave any format string
/// residue in release builds.
#[inline(always)]
pub fn debug_check<F: FnOnce() -> bool>(_check: F, _msg: &str) {
    #[cfg(debug_assertions)]
    {
        if !_check() {
            cold_panic(_msg);
        }
    }
}

// ============================================================================
// Static String Utilities (Avoid Format String Bloat)
// ============================================================================

/// Static error message - avoids format string allocation.
///
/// Use this for common error messages that don't need dynamic formatting.
/// This reduces binary size by avoiding format machinery.
#[derive(Debug, Clone, Copy)]
pub struct StaticError {
    pub message: &'static str,
    pub code: u32,
}

impl StaticError {
    /// Create a new static error.
    pub const fn new(message: &'static str, code: u32) -> Self {
        Self { message, code }
    }
}

impl std::fmt::Display for StaticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for StaticError {}

/// Common engine errors as static constants.
/// Using static strings avoids format string bloat.
pub mod errors {
    use super::StaticError;
    
    pub const OUT_OF_MEMORY: StaticError = StaticError::new("out of memory", 1);
    pub const INVALID_STATE: StaticError = StaticError::new("invalid state", 2);
    pub const NOT_FOUND: StaticError = StaticError::new("not found", 3);
    pub const PARSE_ERROR: StaticError = StaticError::new("parse error", 4);
    pub const NETWORK_ERROR: StaticError = StaticError::new("network error", 5);
    pub const TIMEOUT: StaticError = StaticError::new("timeout", 6);
    pub const PERMISSION_DENIED: StaticError = StaticError::new("permission denied", 7);
    pub const UNSUPPORTED: StaticError = StaticError::new("unsupported operation", 8);
    pub const INVALID_INPUT: StaticError = StaticError::new("invalid input", 9);
    pub const IO_ERROR: StaticError = StaticError::new("I/O error", 10);
}

/// Concatenate static strings at compile time (when possible).
/// This macro avoids runtime format! calls.
#[macro_export]
macro_rules! static_concat {
    ($($s:expr),+ $(,)?) => {
        concat!($($s),+)
    };
}

/// Create an error result with a static message.
/// Avoids format! string bloat.
#[macro_export]
macro_rules! static_err {
    ($msg:literal) => {
        Err($crate::cold::StaticError::new($msg, 0))
    };
    ($msg:literal, $code:expr) => {
        Err($crate::cold::StaticError::new($msg, $code))
    };
}

/// Bail with a static error message.
/// Use instead of bail! or return Err(format!(...)).
#[macro_export]
macro_rules! static_bail {
    ($msg:literal) => {
        return Err($crate::cold::StaticError::new($msg, 0).into())
    };
    ($msg:literal, $code:expr) => {
        return Err($crate::cold::StaticError::new($msg, $code).into())
    };
}

// ============================================================================
// Monomorphization Control
// ============================================================================

/// Trait for types that should use dynamic dispatch to avoid monomorphization bloat.
///
/// When implementing generic code that could cause excessive monomorphization,
/// consider using `&dyn DynDispatch` instead of `impl Trait` or `T: Trait`.
///
/// # Guidelines
/// 
/// Use `dyn Trait` when:
/// - The trait has many implementors
/// - Generic functions are called with many different types
/// - The function body is large (>50 lines)
/// - Performance is not critical for this code path
///
/// Use generics when:
/// - Performance is critical (hot paths)
/// - There are few concrete types
/// - The function is small and benefits from inlining
pub trait DynDispatch: Send + Sync {
    /// Type name for debugging
    fn type_name(&self) -> &'static str;
}

/// Wrapper to force dynamic dispatch for a generic type.
///
/// This can help reduce binary bloat when a generic function
/// is being instantiated for many types.
pub struct DynWrapper<T>(pub T);

impl<T: Send + Sync + 'static> DynDispatch for DynWrapper<T> {
    fn type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cold_path() {
        let result = cold_path(|| 42);
        assert_eq!(result, 42);
    }

    #[test]
    fn test_format_error() {
        let msg = format_error("test error", "TestContext");
        assert_eq!(msg, "TestContext: test error");
    }

    #[test]
    #[should_panic(expected = "test panic")]
    fn test_cold_panic() {
        cold_panic("test panic");
    }

    #[test]
    fn test_debug_check_passes() {
        debug_check(|| true, "should pass");
    }
}
