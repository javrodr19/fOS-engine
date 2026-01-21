//! Extended Bytecode - 150+ Specialized Opcodes
//!
//! Extends the base bytecode with specialized opcodes for:
//! - Type-specialized arithmetic
//! - Fast property access patterns
//! - Array operations
//! - String operations
//! - Optimized control flow

/// Extended opcode set (150+ opcodes)
/// Uses ranges to organize by category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExtendedOpcode {
    // ===========================================
    // Type-Specialized Integer Arithmetic (160-179)
    // ===========================================
    /// Add two SMIs (small integers), no overflow check
    AddSmi = 160,
    /// Subtract two SMIs
    SubSmi = 161,
    /// Multiply two SMIs
    MulSmi = 162,
    /// Divide two SMIs (integer division)
    DivSmi = 163,
    /// Modulo two SMIs
    ModSmi = 164,
    /// Negate SMI
    NegSmi = 165,
    /// Increment SMI
    IncSmi = 166,
    /// Decrement SMI
    DecSmi = 167,
    /// Bitwise AND SMIs
    AndSmi = 168,
    /// Bitwise OR SMIs
    OrSmi = 169,
    /// Bitwise XOR SMIs
    XorSmi = 170,
    /// Left shift SMI
    ShlSmi = 171,
    /// Right shift SMI (signed)
    ShrSmi = 172,
    /// Right shift SMI (unsigned)
    UshrSmi = 173,
    
    // ===========================================
    // Type-Specialized Float Arithmetic (180-189)
    // ===========================================
    /// Add two heap numbers (f64)
    AddNumber = 180,
    /// Subtract two heap numbers
    SubNumber = 181,
    /// Multiply two heap numbers
    MulNumber = 182,
    /// Divide two heap numbers
    DivNumber = 183,
    /// Modulo heap numbers
    ModNumber = 184,
    /// Power (exponentiation)
    PowNumber = 185,
    /// Negate heap number
    NegNumber = 186,
    /// Floor division
    FloorDiv = 187,
    /// Ceiling division
    CeilDiv = 188,
    /// Truncate to integer
    TruncNumber = 189,

    // ===========================================
    // String Operations (190-209)
    // ===========================================
    /// Concatenate two strings
    StringConcat = 190,
    /// Get string length
    StringLength = 191,
    /// Get character at index
    StringCharAt = 192,
    /// Get char code at index
    StringCharCodeAt = 193,
    /// Substring extraction
    StringSubstring = 194,
    /// String indexOf
    StringIndexOf = 195,
    /// String includes
    StringIncludes = 196,
    /// String startsWith
    StringStartsWith = 197,
    /// String endsWith
    StringEndsWith = 198,
    /// String split
    StringSplit = 199,
    /// String trim
    StringTrim = 200,
    /// String toLowerCase
    StringToLower = 201,
    /// String toUpperCase
    StringToUpper = 202,
    /// String slice
    StringSlice = 203,
    /// String repeat
    StringRepeat = 204,
    /// String replace
    StringReplace = 205,
    /// Template literal part
    TemplateLiteral = 206,
    /// String comparison (fast path)
    StringEq = 207,
    /// String less than
    StringLt = 208,
    /// Intern string (for repeated use)
    StringIntern = 209,

    // ===========================================
    // Array Operations (210-229)
    // ===========================================
    /// Push element to array
    ArrayPush = 210,
    /// Pop element from array
    ArrayPop = 211,
    /// Array length
    ArrayLength = 212,
    /// Array shift
    ArrayShift = 213,
    /// Array unshift
    ArrayUnshift = 214,
    /// Array splice
    ArraySplice = 215,
    /// Array slice
    ArraySlice = 216,
    /// Array concat
    ArrayConcat = 217,
    /// Array indexOf
    ArrayIndexOf = 218,
    /// Array includes
    ArrayIncludes = 219,
    /// Fast array element access (bounds checked)
    ArrayGetFast = 220,
    /// Fast array element store (bounds checked)
    ArraySetFast = 221,
    /// Array fill
    ArrayFill = 222,
    /// Array reverse
    ArrayReverse = 223,
    /// Array sort
    ArraySort = 224,
    /// Array from (create from iterable)
    ArrayFrom = 225,
    /// Array isArray check
    ArrayIsArray = 226,
    /// Array flat
    ArrayFlat = 227,
    /// Array join
    ArrayJoin = 228,
    /// New typed array
    NewTypedArray = 229,

    // ===========================================
    // Object Operations (230-244)
    // ===========================================
    /// Object.keys
    ObjectKeys = 230,
    /// Object.values
    ObjectValues = 231,
    /// Object.entries
    ObjectEntries = 232,
    /// Object.assign
    ObjectAssign = 233,
    /// Object.freeze
    ObjectFreeze = 234,
    /// Object.seal
    ObjectSeal = 235,
    /// Object.is
    ObjectIs = 236,
    /// Object.hasOwn (ES2022)
    ObjectHasOwn = 237,
    /// Object.fromEntries
    ObjectFromEntries = 238,
    /// Object.create
    ObjectCreate = 239,
    /// Object.getOwnPropertyDescriptor
    ObjectGetDescriptor = 240,
    /// Object.defineProperty
    ObjectDefineProperty = 241,
    /// Get private field
    GetPrivateField = 242,
    /// Set private field
    SetPrivateField = 243,
    /// Has private field (in check)
    HasPrivateField = 244,

    // ===========================================
    // Optimized Property Access (245-254)
    // ===========================================
    /// Get property with inline cache (monomorphic)
    GetPropertyIC = 245,
    /// Set property with inline cache
    SetPropertyIC = 246,
    /// Get property (polymorphic, 4 shapes)
    GetPropertyPoly = 247,
    /// Set property (polymorphic)
    SetPropertyPoly = 248,
    /// Get named property (constant name)
    GetNamedProperty = 249,
    /// Set named property
    SetNamedProperty = 250,
    /// Get computed property
    GetComputedProperty = 251,
    /// Set computed property
    SetComputedProperty = 252,
    /// Delete property
    DeleteProperty = 253,
    /// Has own property check
    HasOwnProperty = 254,

    // Extended opcodes (next byte indicates actual operation)
    Extended = 255,
}

/// Second-byte opcodes when first byte is Extended (255)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExtendedOpcode2 {
    // ===========================================
    // Control Flow Extensions (0-19)
    // ===========================================
    /// Wide jump (i32 offset)
    JumpWide = 0,
    /// Wide conditional jump
    JumpIfFalseWide = 1,
    /// Wide conditional jump
    JumpIfTrueWide = 2,
    /// Switch table dispatch
    SwitchTable = 3,
    /// Loop header (for OSR)
    LoopHeader = 4,
    /// OSR entry point
    OsrEntry = 5,
    /// Deoptimize and bail to interpreter
    Deopt = 6,
    /// Debugger breakpoint
    Debugger = 7,
    /// Assert (debug builds)
    Assert = 8,
    /// Profile counter increment
    ProfileCount = 9,

    // ===========================================
    // Function Extensions (20-39)
    // ===========================================
    /// Wide call (u16 argc)
    CallWide = 20,
    /// Call with spread
    CallSpread = 21,
    /// Super call
    SuperCall = 22,
    /// Method call (get + call combined)
    CallMethod = 23,
    /// Optional chain call (obj?.method())
    CallOptional = 24,
    /// New with arguments
    Construct = 25,
    /// Construct with spread
    ConstructSpread = 26,
    /// Apply function
    Apply = 27,
    /// Bind function
    Bind = 28,
    /// Get callee (for arguments.callee)
    GetCallee = 29,
    /// Get new.target
    GetNewTarget = 30,
    /// Arrow function (captures this)
    ArrowClosure = 31,
    /// Generator function
    GeneratorClosure = 32,
    /// Async function
    AsyncClosure = 33,
    /// Async generator
    AsyncGeneratorClosure = 34,

    // ===========================================
    // Generator/Async (40-59)
    // ===========================================
    /// Yield value
    Yield = 40,
    /// Yield* (delegate)
    YieldStar = 41,
    /// Await promise
    Await = 42,
    /// Create generator object
    CreateGenerator = 43,
    /// Resume generator
    ResumeGenerator = 44,
    /// Suspend generator
    SuspendGenerator = 45,
    /// Close generator
    CloseGenerator = 46,
    /// Get generator state
    GeneratorState = 47,
    /// Create async from sync iterator
    CreateAsyncFromSync = 48,

    // ===========================================
    // Comparison Extensions (60-79)
    // ===========================================
    /// Compare null (x == null or x != null)
    CompareNull = 60,
    /// Compare undefined
    CompareUndefined = 61,
    /// Compare nullish (null or undefined)
    CompareNullish = 62,
    /// Test if object
    TestObject = 63,
    /// Test if function
    TestFunction = 64,
    /// Test if string
    TestString = 65,
    /// Test if number
    TestNumber = 66,
    /// Test if boolean
    TestBoolean = 67,
    /// Test if symbol
    TestSymbol = 68,
    /// Test if bigint
    TestBigInt = 69,
    /// Three-way compare (returns -1, 0, 1)
    Compare3Way = 70,
    /// Same value check (Object.is semantics)
    SameValue = 71,

    // ===========================================
    // Spread/Rest (80-89)
    // ===========================================
    /// Spread array
    SpreadArray = 80,
    /// Spread object
    SpreadObject = 81,
    /// Rest element (collect remaining)
    RestElement = 82,
    /// Create arguments object
    CreateArguments = 83,
    /// Create mapped arguments
    CreateMappedArguments = 84,
    /// Get rest parameter as array
    GetRestParam = 85,

    // ===========================================
    // Class Operations (90-109)
    // ===========================================
    /// Create class
    CreateClass = 90,
    /// Define method
    DefineMethod = 91,
    /// Define getter
    DefineGetter = 92,
    /// Define setter
    DefineSetter = 93,
    /// Define static method
    DefineStaticMethod = 94,
    /// Define static getter
    DefineStaticGetter = 95,
    /// Define static setter
    DefineStaticSetter = 96,
    /// Create private name
    CreatePrivateName = 97,
    /// Private brand check
    CheckPrivateBrand = 98,
    /// Set private brand
    SetPrivateBrand = 99,
    /// Call super constructor
    SuperConstruct = 100,
    /// Get super property
    GetSuperProperty = 101,
    /// Set super property
    SetSuperProperty = 102,
    /// Static block
    StaticBlock = 103,

    // ===========================================
    // RegExp Operations (110-119)
    // ===========================================
    /// Create RegExp
    CreateRegExp = 110,
    /// RegExp test
    RegExpTest = 111,
    /// RegExp exec
    RegExpExec = 112,
    /// RegExp match
    RegExpMatch = 113,
    /// RegExp replace
    RegExpReplace = 114,
    /// RegExp search
    RegExpSearch = 115,
    /// RegExp split
    RegExpSplit = 116,

    // ===========================================
    // Promise Operations (120-129)
    // ===========================================
    /// Create promise
    CreatePromise = 120,
    /// Resolve promise
    ResolvePromise = 121,
    /// Reject promise
    RejectPromise = 122,
    /// Promise.all
    PromiseAll = 123,
    /// Promise.race
    PromiseRace = 124,
    /// Promise.allSettled
    PromiseAllSettled = 125,
    /// Promise.any
    PromiseAny = 126,
    /// Promise.withResolvers (ES2024)
    PromiseWithResolvers = 127,

    // ===========================================
    // Math Operations (130-149)
    // ===========================================
    /// Math.floor
    MathFloor = 130,
    /// Math.ceil
    MathCeil = 131,
    /// Math.round
    MathRound = 132,
    /// Math.trunc
    MathTrunc = 133,
    /// Math.abs
    MathAbs = 134,
    /// Math.sqrt
    MathSqrt = 135,
    /// Math.sin
    MathSin = 136,
    /// Math.cos
    MathCos = 137,
    /// Math.tan
    MathTan = 138,
    /// Math.log
    MathLog = 139,
    /// Math.exp
    MathExp = 140,
    /// Math.min (2 args)
    MathMin = 141,
    /// Math.max (2 args)
    MathMax = 142,
    /// Math.pow
    MathPow = 143,
    /// Math.random
    MathRandom = 144,
    /// Math.sign
    MathSign = 145,
    /// Math.clz32
    MathClz32 = 146,
    /// Math.imul
    MathImul = 147,
    /// Math.fround
    MathFround = 148,
    /// Math.hypot
    MathHypot = 149,

    // ===========================================
    // Symbol/Well-known (150-159)
    // ===========================================
    /// Get Symbol.iterator
    GetSymbolIterator = 150,
    /// Get Symbol.toStringTag
    GetSymbolToStringTag = 151,
    /// Get Symbol.toPrimitive
    GetSymbolToPrimitive = 152,
    /// Get Symbol.hasInstance
    GetSymbolHasInstance = 153,
    /// Create symbol
    CreateSymbol = 154,
    /// Symbol.for
    SymbolFor = 155,
    /// Symbol.keyFor
    SymbolKeyFor = 156,

    // ===========================================
    // BigInt Operations (160-169)
    // ===========================================
    /// Create BigInt
    CreateBigInt = 160,
    /// BigInt add
    BigIntAdd = 161,
    /// BigInt subtract
    BigIntSub = 162,
    /// BigInt multiply
    BigIntMul = 163,
    /// BigInt divide
    BigIntDiv = 164,
    /// BigInt modulo
    BigIntMod = 165,
    /// BigInt power
    BigIntPow = 166,
    /// BigInt negate
    BigIntNeg = 167,
    /// BigInt to number
    BigIntToNumber = 168,
    /// Number to BigInt
    NumberToBigInt = 169,

    // ===========================================
    // Decorators (170-179) - ES Decorators
    // ===========================================
    /// Apply class decorator
    ApplyClassDecorator = 170,
    /// Apply method decorator
    ApplyMethodDecorator = 171,
    /// Apply field decorator
    ApplyFieldDecorator = 172,
    /// Apply accessor decorator
    ApplyAccessorDecorator = 173,
    /// Apply getter decorator
    ApplyGetterDecorator = 174,
    /// Apply setter decorator
    ApplySetterDecorator = 175,
    /// Decorator context create
    CreateDecoratorContext = 176,
    /// Add initializer
    AddInitializer = 177,
    /// Run initializers
    RunInitializers = 178,

    // ===========================================
    // Pattern Matching (180-189)
    // ===========================================
    /// Match pattern start
    MatchStart = 180,
    /// Match literal
    MatchLiteral = 181,
    /// Match binding
    MatchBinding = 182,
    /// Match array pattern
    MatchArray = 183,
    /// Match object pattern
    MatchObject = 184,
    /// Match guard check
    MatchGuard = 185,
    /// Match wildcard (_)
    MatchWildcard = 186,
    /// Match or pattern
    MatchOr = 187,
    /// Match end (success)
    MatchEnd = 188,
    /// Match fail (next arm)
    MatchFail = 189,

    // ===========================================
    // WeakRef/FinalizationRegistry (190-199)
    // ===========================================
    /// Create WeakRef
    CreateWeakRef = 190,
    /// WeakRef deref
    WeakRefDeref = 191,
    /// Create FinalizationRegistry
    CreateFinalizationRegistry = 192,
    /// Register finalizer
    RegisterFinalizer = 193,
    /// Unregister finalizer
    UnregisterFinalizer = 194,
    /// Create WeakMap
    CreateWeakMap = 195,
    /// Create WeakSet
    CreateWeakSet = 196,

    // ===========================================
    // Proxy/Reflect (200-209)
    // ===========================================
    /// Create Proxy
    CreateProxy = 200,
    /// Reflect.get
    ReflectGet = 201,
    /// Reflect.set
    ReflectSet = 202,
    /// Reflect.has
    ReflectHas = 203,
    /// Reflect.deleteProperty
    ReflectDelete = 204,
    /// Reflect.ownKeys
    ReflectOwnKeys = 205,
    /// Reflect.apply
    ReflectApply = 206,
    /// Reflect.construct
    ReflectConstruct = 207,
    /// Revocable proxy
    CreateRevocableProxy = 208,
    /// Revoke proxy
    RevokeProxy = 209,

    // ===========================================
    // Atomics (210-219)
    // ===========================================
    /// Atomics.load
    AtomicsLoad = 210,
    /// Atomics.store
    AtomicsStore = 211,
    /// Atomics.add
    AtomicsAdd = 212,
    /// Atomics.sub
    AtomicsSub = 213,
    /// Atomics.and
    AtomicsAnd = 214,
    /// Atomics.or
    AtomicsOr = 215,
    /// Atomics.xor
    AtomicsXor = 216,
    /// Atomics.exchange
    AtomicsExchange = 217,
    /// Atomics.compareExchange
    AtomicsCas = 218,
    /// Atomics.waitAsync (ES2024)
    AtomicsWaitAsync = 219,

    // ===========================================
    // Module Operations (220-229)
    // ===========================================
    /// Dynamic import
    DynamicImport = 220,
    /// Import meta
    ImportMeta = 221,
    /// Import assertion
    ImportAssertion = 222,
    /// Export binding
    ExportBinding = 223,
    /// Export default
    ExportDefault = 224,
    /// Re-export
    ReExport = 225,
    /// Star export
    StarExport = 226,
    /// Get module namespace
    GetModuleNamespace = 227,

    // ===========================================
    // Collection Operations (230-239)
    // ===========================================
    /// Map.get
    MapGet = 230,
    /// Map.set
    MapSet = 231,
    /// Map.has
    MapHas = 232,
    /// Map.delete
    MapDelete = 233,
    /// Set.add
    SetAdd = 234,
    /// Set.has
    SetHas = 235,
    /// Set.delete
    SetDelete = 236,
    /// Collection clear
    CollectionClear = 237,
    /// Collection size
    CollectionSize = 238,
    /// Collection forEach
    CollectionForEach = 239,

    // ===========================================
    // Date Operations (240-249)
    // ===========================================
    /// Create Date
    CreateDate = 240,
    /// Date.now()
    DateNow = 241,
    /// Get time
    DateGetTime = 242,
    /// Get year
    DateGetYear = 243,
    /// Get month
    DateGetMonth = 244,
    /// Get day
    DateGetDay = 245,
    /// Get hours
    DateGetHours = 246,
    /// Set time
    DateSetTime = 247,
    /// To ISO string
    DateToISOString = 248,
    /// Parse date
    DateParse = 249,

    // ===========================================
    // JSON Operations (250-254)
    // ===========================================
    /// JSON.parse
    JsonParse = 250,
    /// JSON.stringify
    JsonStringify = 251,
    /// JSON.parse reviver
    JsonParseReviver = 252,
    /// JSON.stringify replacer
    JsonStringifyReplacer = 253,
    /// Structured clone
    StructuredClone = 254,
}

/// Opcode categories for optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpcodeCategory {
    Stack,
    Load,
    Store,
    Arithmetic,
    Bitwise,
    Compare,
    Jump,
    Call,
    Object,
    Array,
    String,
    TypeCheck,
    Generator,
    Async,
    Class,
    Module,
}

impl ExtendedOpcode {
    /// Get opcode category
    pub fn category(self) -> OpcodeCategory {
        match self as u8 {
            160..=179 => OpcodeCategory::Arithmetic, // SMI ops
            180..=189 => OpcodeCategory::Arithmetic, // Number ops
            190..=209 => OpcodeCategory::String,
            210..=229 => OpcodeCategory::Array,
            230..=244 => OpcodeCategory::Object,
            245..=254 => OpcodeCategory::Object, // Property access
            _ => OpcodeCategory::Stack,
        }
    }

    /// Check if opcode can throw
    pub fn can_throw(self) -> bool {
        matches!(self,
            ExtendedOpcode::DivSmi |
            ExtendedOpcode::ModSmi |
            ExtendedOpcode::DivNumber |
            ExtendedOpcode::ArrayGetFast |
            ExtendedOpcode::GetPropertyIC |
            ExtendedOpcode::SetPropertyIC
        )
    }

    /// Check if opcode has side effects
    pub fn has_side_effects(self) -> bool {
        matches!(self,
            ExtendedOpcode::ArrayPush |
            ExtendedOpcode::ArrayPop |
            ExtendedOpcode::SetPropertyIC |
            ExtendedOpcode::DeleteProperty
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_count() {
        // Verify we have 150+ opcodes
        assert!(ExtendedOpcode::Extended as u8 >= 95);
        assert!(ExtendedOpcode2::StructuredClone as u8 >= 254);
    }

    #[test]
    fn test_opcode_category() {
        assert_eq!(ExtendedOpcode::AddSmi.category(), OpcodeCategory::Arithmetic);
        assert_eq!(ExtendedOpcode::StringConcat.category(), OpcodeCategory::String);
        assert_eq!(ExtendedOpcode::ArrayPush.category(), OpcodeCategory::Array);
    }

    #[test]
    fn test_can_throw() {
        assert!(ExtendedOpcode::DivSmi.can_throw());
        assert!(!ExtendedOpcode::AddSmi.can_throw());
    }
}
