//! Guard Insertion for Speculation
//!
//! Inserts type guards in JIT-compiled code for speculative optimizations.
//! Guards check assumptions and trigger deoptimization if violated.

use super::value::JsVal;

/// Type of guard check
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardType {
    /// Value must be a number
    IsNumber,
    /// Value must be an integer (no fractional part)
    IsInteger,
    /// Value must be a string
    IsString,
    /// Value must be an object
    IsObject,
    /// Value must be an array
    IsArray,
    /// Value must be truthy
    IsTruthy,
    /// Value must equal specific constant
    EqualsConstant,
    /// Object has expected shape
    HasShape(u32),
}

/// A guard check to be compiled
#[derive(Debug, Clone)]
pub struct Guard {
    pub guard_type: GuardType,
    pub register: u8,           // Virtual register to check
    pub deopt_id: u32,          // Deopt point ID if guard fails
}

impl Guard {
    pub fn new(guard_type: GuardType, register: u8, deopt_id: u32) -> Self {
        Self { guard_type, register, deopt_id }
    }
    
    /// Check if a value passes this guard
    pub fn check(&self, val: &JsVal) -> bool {
        match self.guard_type {
            GuardType::IsNumber => val.as_number().is_some(),
            GuardType::IsInteger => {
                val.as_number().map(|n| n.fract() == 0.0).unwrap_or(false)
            }
            GuardType::IsString => val.as_string().is_some(),
            GuardType::IsObject => val.as_object_id().is_some(),
            GuardType::IsArray => val.as_array_id().is_some(),
            GuardType::IsTruthy => val.is_truthy(),
            GuardType::EqualsConstant => false, // Requires additional data
            GuardType::HasShape(_) => true, // Requires shape system
        }
    }
}

/// Guard compiler - generates guard checks for JIT code
pub struct GuardCompiler {
    guards: Vec<Guard>,
    next_deopt_id: u32,
}

impl Default for GuardCompiler {
    fn default() -> Self { Self::new() }
}

impl GuardCompiler {
    pub fn new() -> Self {
        Self {
            guards: Vec::new(),
            next_deopt_id: 0,
        }
    }
    
    /// Add a guard
    pub fn add_guard(&mut self, guard_type: GuardType, register: u8) -> u32 {
        let deopt_id = self.next_deopt_id;
        self.next_deopt_id += 1;
        self.guards.push(Guard::new(guard_type, register, deopt_id));
        deopt_id
    }
    
    /// Generate x86_64 code for guard check
    pub fn emit_guard(&self, guard: &Guard, codegen: &mut super::x64_codegen::X64Codegen) {
        use super::x64_codegen::X64Reg;
        
        match guard.guard_type {
            GuardType::IsNumber => {
                // Check NaN-boxing tag
                // Load value, check if it's a valid number
                // If not, jump to deopt
                codegen.test_reg_reg(X64Reg::Rax, X64Reg::Rax);
                codegen.jne_label(guard.deopt_id);
            }
            GuardType::IsInteger => {
                // Convert to int and back, compare
                codegen.cvttsd2si_reg_xmm(X64Reg::Rax, 0);
                codegen.cvtsi2sd_xmm_reg(1, X64Reg::Rax);
                codegen.ucomisd_xmm_xmm(0, 1);
                codegen.jne_label(guard.deopt_id);
            }
            _ => {
                // Generic guard - check and branch
                codegen.test_reg_reg(X64Reg::Rax, X64Reg::Rax);
                codegen.je_label(guard.deopt_id);
            }
        }
    }
    
    /// Get all guards
    pub fn guards(&self) -> &[Guard] {
        &self.guards
    }
    
    /// Clear guards
    pub fn clear(&mut self) {
        self.guards.clear();
        self.next_deopt_id = 0;
    }
}

/// Runtime guard tracker
#[derive(Debug, Default)]
pub struct GuardStats {
    pub checks_passed: u64,
    pub checks_failed: u64,
}

impl GuardStats {
    pub fn new() -> Self { Self::default() }
    
    pub fn record_pass(&mut self) { self.checks_passed += 1; }
    pub fn record_fail(&mut self) { self.checks_failed += 1; }
    
    pub fn success_rate(&self) -> f64 {
        let total = self.checks_passed + self.checks_failed;
        if total == 0 { 1.0 } else { self.checks_passed as f64 / total as f64 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_guard_check() {
        let guard = Guard::new(GuardType::IsNumber, 0, 0);
        
        assert!(guard.check(&JsVal::Number(42.0)));
        assert!(!guard.check(&JsVal::String("hello".into())));
    }
    
    #[test]
    fn test_integer_guard() {
        let guard = Guard::new(GuardType::IsInteger, 0, 0);
        
        assert!(guard.check(&JsVal::Number(42.0)));
        assert!(!guard.check(&JsVal::Number(3.14)));
    }
}
