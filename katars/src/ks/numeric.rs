//! Fixed-width numeric primitives (U8..U128, I8..I128, Usz, Isz, F16, F32, F64).
//!
//! A single `define_numeric_prims!` macro generates all repetitive code for 15
//! types. Existing files delegate to the helpers here via fallthrough arms.
//!
//! Three "kinds" determine behavior:
//!   - `unsigned` — checked integer arithmetic, bitwise ops, no negation
//!   - `signed`   — checked integer arithmetic, bitwise ops, checked negation
//!   - `float`    — IEEE float arithmetic, no bitwise ops, negation

use half::f16;
use num_bigint::BigInt;
use num_traits::ToPrimitive;

use super::error::ErrorKind;
use super::native::{NativeFnRegistry, PrimMethods};
use super::types::{prim, TypeId, TypeRegistry};
use super::value::Value;
use crate::ks::ast::{BinOp, UnaryOp};
use crate::ks::native::NativeCtx;

// ═══════════════════════════════════════════════════════════════════
// Float helper trait
// ═══════════════════════════════════════════════════════════════════
//
// Rust macros can't pattern-match on type paths (e.g., `half::f16`),
// so we use a trait to handle f16/f32/f64 uniformly.

/// Operations needed on float prim types that vary by width.
trait NumericFloat:
    Sized
    + Copy
    + std::fmt::Display
    + PartialEq
    + PartialOrd
    + std::ops::Add<Output = Self>
    + std::ops::Sub<Output = Self>
    + std::ops::Mul<Output = Self>
    + std::ops::Div<Output = Self>
    + std::ops::Neg<Output = Self>
{
    fn is_zero(&self) -> bool;
    fn from_f64(f: f64) -> Self;
    fn to_f64(self) -> f64;
}

impl NumericFloat for f16 {
    fn is_zero(&self) -> bool {
        *self == f16::ZERO
    }
    fn from_f64(f: f64) -> Self {
        f16::from_f64(f)
    }
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
}

impl NumericFloat for f32 {
    fn is_zero(&self) -> bool {
        *self == 0.0
    }
    fn from_f64(f: f64) -> Self {
        f as f32
    }
    fn to_f64(self) -> f64 {
        self as f64
    }
}

impl NumericFloat for f64 {
    fn is_zero(&self) -> bool {
        *self == 0.0
    }
    fn from_f64(f: f64) -> Self {
        f
    }
    fn to_f64(self) -> f64 {
        self
    }
}

// ═══════════════════════════════════════════════════════════════════
// Kind-specific helper macros
// ═══════════════════════════════════════════════════════════════════

macro_rules! numeric_nonzero {
    (unsigned, $n:expr) => {
        *$n != 0
    };
    (signed,  $n:expr) => {
        *$n != 0
    };
    (float,   $n:expr) => {
        !NumericFloat::is_zero($n)
    };
}

macro_rules! numeric_add {
    (unsigned, $V:ident, $a:expr, $b:expr) => {
        $a.checked_add(*$b)
            .map(Value::$V)
            .ok_or(ErrorKind::IntegerOverflow)
    };
    (signed, $V:ident, $a:expr, $b:expr) => {
        $a.checked_add(*$b)
            .map(Value::$V)
            .ok_or(ErrorKind::IntegerOverflow)
    };
    (float, $V:ident, $a:expr, $b:expr) => {
        Ok(Value::$V(*$a + *$b))
    };
}

macro_rules! numeric_sub {
    (unsigned, $V:ident, $a:expr, $b:expr) => {
        $a.checked_sub(*$b)
            .map(Value::$V)
            .ok_or(ErrorKind::IntegerOverflow)
    };
    (signed, $V:ident, $a:expr, $b:expr) => {
        $a.checked_sub(*$b)
            .map(Value::$V)
            .ok_or(ErrorKind::IntegerOverflow)
    };
    (float, $V:ident, $a:expr, $b:expr) => {
        Ok(Value::$V(*$a - *$b))
    };
}

macro_rules! numeric_mul {
    (unsigned, $V:ident, $a:expr, $b:expr) => {
        $a.checked_mul(*$b)
            .map(Value::$V)
            .ok_or(ErrorKind::IntegerOverflow)
    };
    (signed, $V:ident, $a:expr, $b:expr) => {
        $a.checked_mul(*$b)
            .map(Value::$V)
            .ok_or(ErrorKind::IntegerOverflow)
    };
    (float, $V:ident, $a:expr, $b:expr) => {
        Ok(Value::$V(*$a * *$b))
    };
}

macro_rules! numeric_div {
    (unsigned, $V:ident, $a:expr, $b:expr) => {
        if *$b == 0 {
            Err(ErrorKind::DivisionByZero)
        } else {
            $a.checked_div(*$b)
                .map(Value::$V)
                .ok_or(ErrorKind::IntegerOverflow)
        }
    };
    (signed, $V:ident, $a:expr, $b:expr) => {
        if *$b == 0 {
            Err(ErrorKind::DivisionByZero)
        } else {
            $a.checked_div(*$b)
                .map(Value::$V)
                .ok_or(ErrorKind::IntegerOverflow)
        }
    };
    (float, $V:ident, $a:expr, $b:expr) => {
        if NumericFloat::is_zero($b) {
            Err(ErrorKind::DivisionByZero)
        } else {
            Ok(Value::$V(*$a / *$b))
        }
    };
}

macro_rules! numeric_neg {
    (unsigned, $V:ident, $a:expr) => {
        None
    };
    (signed,  $V:ident, $a:expr) => {
        Some(
            $a.checked_neg()
                .map(Value::$V)
                .ok_or(ErrorKind::IntegerOverflow),
        )
    };
    (float, $V:ident, $a:expr) => {
        Some(Ok(Value::$V(-*$a)))
    };
}

macro_rules! numeric_cmp {
    (unsigned, $a:expr, $b:expr) => {
        Some($a.cmp($b))
    };
    (signed,  $a:expr, $b:expr) => {
        Some($a.cmp($b))
    };
    (float,   $a:expr, $b:expr) => {
        $a.partial_cmp($b)
    };
}

macro_rules! construct_dispatch {
    (unsigned, $name:literal, $V:ident, $T:ty, $arg:expr) => {
        construct_int::<$T>($name, $arg, Value::$V)
    };
    (signed, $name:literal, $V:ident, $T:ty, $arg:expr) => {
        construct_int::<$T>($name, $arg, Value::$V)
    };
    (float, $name:literal, $V:ident, $T:ty, $arg:expr) => {
        construct_float::<$T>($name, $arg, Value::$V)
    };
}

macro_rules! bootstrap_dispatch {
    (unsigned, $reg:expr, $result:expr, $prim:expr, $V:ident) => {
        register_int_methods!($reg, $result, $prim, $V)
    };
    (signed, $reg:expr, $result:expr, $prim:expr, $V:ident) => {
        register_int_methods!($reg, $result, $prim, $V)
    };
    (float, $reg:expr, $result:expr, $prim:expr, $V:ident) => {
        register_float_methods!($reg, $result, $prim, $V)
    };
}

// ═══════════════════════════════════════════════════════════════════
// The master macro
// ═══════════════════════════════════════════════════════════════════

macro_rules! define_numeric_prims {
    (
        $( $name:literal, $V:ident, $T:ty, $C:ident, $id:literal, $kind:ident );* $(;)?
    ) => {

        // ── Type registration ─────────────────────────────────────

        /// Register all numeric prims in TypeRegistry (called after Nil..Char).
        pub fn register_all(reg: &mut TypeRegistry) {
            $( reg.register_prim($name); )*
        }

        /// TypeId → display name for numeric types.
        pub fn display_static(id: TypeId) -> Option<&'static str> {
            match id {
                $( prim::$C => Some($name), )*
                _ => None,
            }
        }

        /// (name, TypeId) pairs for scope registration.
        pub fn type_entries() -> &'static [(&'static str, TypeId)] {
            &[ $( ($name, prim::$C), )* ]
        }

        // ── Value dispatch ────────────────────────────────────────

        pub fn type_id_of(v: &Value) -> TypeId {
            match v {
                $( Value::$V(_) => prim::$C, )*
                _ => panic!("numeric::type_id_of called on non-numeric"),
            }
        }

        pub fn display_numeric(v: &Value) -> Option<String> {
            match v {
                $( Value::$V(n) => Some(format!("{n}")), )*
                _ => None,
            }
        }

        pub fn eq_numeric(a: &Value, b: &Value) -> bool {
            match (a, b) {
                $( (Value::$V(x), Value::$V(y)) => x == y, )*
                _ => false,
            }
        }

        pub fn try_truth(v: &Value) -> Option<bool> {
            match v {
                $( Value::$V(n) => Some(numeric_nonzero!($kind, n)), )*
                _ => None,
            }
        }

        // ── Operators ─────────────────────────────────────────────

        pub fn try_binop(op: BinOp, left: &Value, right: &Value) -> Option<Result<Value, ErrorKind>> {
            match (left, right) {
                $(
                    (Value::$V(a), Value::$V(b)) => Some(match op {
                        BinOp::Add => numeric_add!($kind, $V, a, b),
                        BinOp::Sub => numeric_sub!($kind, $V, a, b),
                        BinOp::Mul => numeric_mul!($kind, $V, a, b),
                        BinOp::Div => numeric_div!($kind, $V, a, b),
                        BinOp::Eq  => Ok(Value::Bool(a == b)),
                        BinOp::Ne  => Ok(Value::Bool(a != b)),
                        BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                            match numeric_cmp!($kind, a, b) {
                                Some(ord) => Ok(Value::Bool(match op {
                                    BinOp::Lt => ord == std::cmp::Ordering::Less,
                                    BinOp::Gt => ord == std::cmp::Ordering::Greater,
                                    BinOp::Le => ord != std::cmp::Ordering::Greater,
                                    BinOp::Ge => ord != std::cmp::Ordering::Less,
                                    _ => unreachable!(),
                                })),
                                None => Err(ErrorKind::NanComparison),
                            }
                        }
                    }),
                )*
                _ => None,
            }
        }

        pub fn try_unaryop(op: UnaryOp, operand: &Value) -> Option<Result<Value, ErrorKind>> {
            match op {
                UnaryOp::Neg => match operand {
                    $( Value::$V(a) => numeric_neg!($kind, $V, a), )*
                    _ => None,
                },
                UnaryOp::Not => None,
            }
        }

        // ── Construction ──────────────────────────────────────────

        pub fn try_construct(type_id: TypeId, arg: &Value) -> Option<Result<Value, ErrorKind>> {
            match type_id {
                $( prim::$C => Some(construct_dispatch!($kind, $name, $V, $T, arg)), )*
                _ => None,
            }
        }

        // ── Native methods ────────────────────────────────────────

        pub fn bootstrap_methods_impl(reg: &mut NativeFnRegistry) -> Vec<(TypeId, PrimMethods)> {
            let mut result = Vec::new();
            $( bootstrap_dispatch!($kind, reg, result, prim::$C, $V); )*
            result
        }
    };
}

// ═══════════════════════════════════════════════════════════════════
// Construction
// ═══════════════════════════════════════════════════════════════════

/// Convert a BigInt to a fixed-width integer with range checking.
fn bigint_to_prim<T>(n: &BigInt, type_name: &'static str) -> Result<T, ErrorKind>
where
    T: TryFrom<i128> + TryFrom<u128>,
{
    if let Some(i) = n.to_i128() {
        if let Ok(val) = T::try_from(i) {
            return Ok(val);
        }
    }
    if let Some(u) = n.to_u128() {
        if let Ok(val) = T::try_from(u) {
            return Ok(val);
        }
    }
    Err(ErrorKind::PrimOutOfRange {
        type_name,
        detail: format!("{n}"),
    })
}

/// Construct an integer from a BigInt.
fn construct_int<T: TryFrom<i128> + TryFrom<u128>>(
    name: &'static str,
    arg: &Value,
    wrap: fn(T) -> Value,
) -> Result<Value, ErrorKind> {
    match arg {
        Value::Int(n) => Ok(wrap(bigint_to_prim(n, name)?)),
        _ => Err(ErrorKind::TypeMismatch {
            expected: prim::INT,
            actual: arg.type_id(),
        }),
    }
}

/// Construct a float from a Float or Int.
fn construct_float<T: NumericFloat>(
    name: &'static str,
    arg: &Value,
    wrap: fn(T) -> Value,
) -> Result<Value, ErrorKind> {
    match arg {
        Value::Float(f) => Ok(wrap(T::from_f64(*f))),
        Value::Int(n) => {
            let f = n.to_f64().ok_or(ErrorKind::IntegerOverflow)?;
            Ok(wrap(T::from_f64(f)))
        }
        _ => Err(ErrorKind::TypeMismatch {
            expected: prim::FLOAT,
            actual: arg.type_id(),
        }),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Native method registration
// ═══════════════════════════════════════════════════════════════════

macro_rules! register_int_methods {
    ($reg:expr, $result:expr, $prim:expr, $V:ident) => {{
        let mut m = Vec::new();

        m.push((
            "to_int",
            $reg.register(
                "to_int",
                false,
                |_: &mut NativeCtx, args: &[Value]| -> Result<Value, super::error::RuntimeError> {
                    let Value::$V(n) = &args[0] else {
                        return Err(ErrorKind::TypeMismatch {
                            expected: $prim,
                            actual: args[0].type_id(),
                        }
                        .into());
                    };
                    Ok(Value::Int(BigInt::from(*n)))
                },
            ),
        ));

        m.push((
            "band",
            $reg.register(
                "band",
                false,
                |_: &mut NativeCtx, args: &[Value]| -> Result<Value, super::error::RuntimeError> {
                    let (Value::$V(a), Value::$V(b)) = (&args[0], &args[1]) else {
                        return Err(ErrorKind::TypeMismatch {
                            expected: $prim,
                            actual: args[1].type_id(),
                        }
                        .into());
                    };
                    Ok(Value::$V(a & b))
                },
            ),
        ));

        m.push((
            "ior",
            $reg.register(
                "ior",
                false,
                |_: &mut NativeCtx, args: &[Value]| -> Result<Value, super::error::RuntimeError> {
                    let (Value::$V(a), Value::$V(b)) = (&args[0], &args[1]) else {
                        return Err(ErrorKind::TypeMismatch {
                            expected: $prim,
                            actual: args[1].type_id(),
                        }
                        .into());
                    };
                    Ok(Value::$V(a | b))
                },
            ),
        ));

        m.push((
            "xor",
            $reg.register(
                "xor",
                false,
                |_: &mut NativeCtx, args: &[Value]| -> Result<Value, super::error::RuntimeError> {
                    let (Value::$V(a), Value::$V(b)) = (&args[0], &args[1]) else {
                        return Err(ErrorKind::TypeMismatch {
                            expected: $prim,
                            actual: args[1].type_id(),
                        }
                        .into());
                    };
                    Ok(Value::$V(a ^ b))
                },
            ),
        ));

        m.push((
            "inv",
            $reg.register(
                "inv",
                false,
                |_: &mut NativeCtx, args: &[Value]| -> Result<Value, super::error::RuntimeError> {
                    let Value::$V(a) = &args[0] else {
                        return Err(ErrorKind::TypeMismatch {
                            expected: $prim,
                            actual: args[0].type_id(),
                        }
                        .into());
                    };
                    Ok(Value::$V(!a))
                },
            ),
        ));

        m.push((
            "shl",
            $reg.register(
                "shl",
                false,
                |_: &mut NativeCtx, args: &[Value]| -> Result<Value, super::error::RuntimeError> {
                    let (Value::$V(a), Value::Int(b)) = (&args[0], &args[1]) else {
                        return Err(ErrorKind::TypeMismatch {
                            expected: prim::INT,
                            actual: args[1].type_id(),
                        }
                        .into());
                    };
                    let shift = b.to_u32().ok_or(ErrorKind::IntegerOverflow)?;
                    Ok(Value::$V(a.wrapping_shl(shift)))
                },
            ),
        ));

        m.push((
            "shr",
            $reg.register(
                "shr",
                false,
                |_: &mut NativeCtx, args: &[Value]| -> Result<Value, super::error::RuntimeError> {
                    let (Value::$V(a), Value::Int(b)) = (&args[0], &args[1]) else {
                        return Err(ErrorKind::TypeMismatch {
                            expected: prim::INT,
                            actual: args[1].type_id(),
                        }
                        .into());
                    };
                    let shift = b.to_u32().ok_or(ErrorKind::IntegerOverflow)?;
                    Ok(Value::$V(a.wrapping_shr(shift)))
                },
            ),
        ));

        $result.push(($prim, PrimMethods { methods: m }));
    }};
}

macro_rules! register_float_methods {
    ($reg:expr, $result:expr, $prim:expr, $V:ident) => {{
        let mut m = Vec::new();

        m.push((
            "to_int",
            $reg.register(
                "to_int",
                false,
                |_: &mut NativeCtx, args: &[Value]| -> Result<Value, super::error::RuntimeError> {
                    let Value::$V(n) = &args[0] else {
                        return Err(ErrorKind::TypeMismatch {
                            expected: $prim,
                            actual: args[0].type_id(),
                        }
                        .into());
                    };
                    let f = NumericFloat::to_f64(*n);
                    Ok(Value::Int(BigInt::from(f as i128)))
                },
            ),
        ));

        m.push((
            "to_float",
            $reg.register(
                "to_float",
                false,
                |_: &mut NativeCtx, args: &[Value]| -> Result<Value, super::error::RuntimeError> {
                    let Value::$V(n) = &args[0] else {
                        return Err(ErrorKind::TypeMismatch {
                            expected: $prim,
                            actual: args[0].type_id(),
                        }
                        .into());
                    };
                    Ok(Value::Float(NumericFloat::to_f64(*n)))
                },
            ),
        ));

        $result.push(($prim, PrimMethods { methods: m }));
    }};
}

// ═══════════════════════════════════════════════════════════════════
// Macro invocation — the single source of truth for all 15 types
// ═══════════════════════════════════════════════════════════════════

define_numeric_prims! {
    "U8",   U8,   u8,        U8,   11, unsigned;
    "U16",  U16,  u16,       U16,  12, unsigned;
    "U32",  U32,  u32,       U32,  13, unsigned;
    "U64",  U64,  u64,       U64,  14, unsigned;
    "U128", U128, u128,      U128, 15, unsigned;
    "I8",   I8,   i8,        I8,   16, signed;
    "I16",  I16,  i16,       I16,  17, signed;
    "I32",  I32,  i32,       I32,  18, signed;
    "I64",  I64,  i64,       I64,  19, signed;
    "I128", I128, i128,      I128, 20, signed;
    "Usz",  Usz,  usize,     USZ,  21, unsigned;
    "Isz",  Isz,  isize,     ISZ,  22, signed;
    "F16",  F16,  half::f16, F16,  23, float;
    "F32",  F32,  f32,       F32,  24, float;
    "F64",  F64,  f64,       F64,  25, float;
}
