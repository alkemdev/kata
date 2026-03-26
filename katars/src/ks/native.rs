//! Native function registry, module tree, and all builtin implementations.
//!
//! Native functions are Rust functions with the signature:
//!     `fn(&mut NativeCtx, &[Value]) -> Result<Value, RuntimeError>`
//!
//! They are registered at boot time and organized into a module tree.
//! At call time, `Value::NativeFn(NativeFnId)` is looked up by ID
//! and dispatched — no string matching.

use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;

use indexmap::IndexMap;
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use serde::{Deserialize, Serialize};

use super::ast::{BinOp, UnaryOp};
use super::error::{ArityTarget, ErrorKind, RuntimeError};
use super::types::{prim, TypeRegistry};
use super::value::Value;

// ── IDs ──────────────────────────────────────────────────────────────────────

/// Opaque handle to a registered native function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NativeFnId(pub u32);

/// Opaque handle to a module in the module tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModuleId(pub u32);

// ── Module tree ──────────────────────────────────────────────────────────────

/// A node in the module tree. Entries are Values — sub-modules are
/// `Value::Module(id)`, native fns are `Value::NativeFn(id)`, and
/// KS-defined exports (types, functions) are any Value.
#[derive(Debug)]
pub struct Module {
    pub name: String,
    pub entries: IndexMap<String, Value>,
}

impl Module {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entries: IndexMap::new(),
        }
    }
}

// ── Native function metadata ────────────────────────────────────────────────

/// Type alias for native function handlers.
pub type NativeHandler = fn(&mut NativeCtx, &[Value]) -> Result<Value, RuntimeError>;

/// A registered native function: metadata + handler.
pub struct NativeFnEntry {
    pub name: &'static str,
    pub requires_unsafe: bool,
    pub handler: NativeHandler,
}

// ── NativeCtx ───────────────────────────────────────────────────────────────

/// Runtime context passed to native functions. Provides controlled access
/// to interpreter internals without exposing the full Interpreter.
pub struct NativeCtx<'a> {
    pub types: &'a TypeRegistry,
    pub allocations: &'a mut Vec<Option<Vec<Value>>>,
    pub bin_intern: &'a mut HashSet<Arc<[u8]>>,
    pub out: &'a mut dyn Write,
    /// Available for native fns that need to know if they're in an unsafe context.
    /// Currently checked at the dispatch level before the handler is called.
    #[allow(dead_code)]
    pub in_unsafe: bool,
}

impl<'a> NativeCtx<'a> {
    /// Extract a RawPtr id from args at the given position.
    pub fn expect_rawptr(&self, args: &[Value], pos: usize) -> Result<u32, RuntimeError> {
        match args.get(pos) {
            Some(Value::RawPtr(id)) => Ok(*id),
            Some(other) => Err(ErrorKind::TypeMismatch {
                expected: prim::RAW_PTR,
                actual: other.type_id(),
            }
            .into()),
            None => Err(ErrorKind::InternalError("missing argument to native function").into()),
        }
    }

    /// Extract a usize from args at the given position, expecting an Int.
    pub fn expect_int(&self, args: &[Value], pos: usize) -> Result<usize, RuntimeError> {
        match args.get(pos) {
            Some(Value::Int(n)) => n
                .to_usize()
                .ok_or_else(|| RuntimeError::from(ErrorKind::IntegerOverflow)),
            Some(other) => Err(ErrorKind::TypeMismatch {
                expected: prim::INT,
                actual: other.type_id(),
            }
            .into()),
            None => Err(ErrorKind::InternalError("missing argument to native function").into()),
        }
    }

    /// Intern a byte vector and return a Bin value.
    pub fn intern_bin(&mut self, bytes: Vec<u8>) -> Value {
        if let Some(existing) = self.bin_intern.get(bytes.as_slice()) {
            Value::Bin(Arc::clone(existing))
        } else {
            let arc: Arc<[u8]> = bytes.into();
            self.bin_intern.insert(Arc::clone(&arc));
            Value::Bin(arc)
        }
    }
}

// ── NativeFnRegistry ────────────────────────────────────────────────────────

/// Owns all native function entries and the module tree.
#[derive(Debug)]
pub struct NativeFnRegistry {
    pub fns: Vec<NativeFnEntry>,
    pub modules: Vec<Module>,
}

impl NativeFnRegistry {
    pub fn new() -> Self {
        Self {
            fns: Vec::new(),
            modules: Vec::new(),
        }
    }

    /// Register a native function, returning its ID.
    pub fn register(
        &mut self,
        name: &'static str,
        requires_unsafe: bool,
        handler: NativeHandler,
    ) -> NativeFnId {
        let id = NativeFnId(self.fns.len() as u32);
        self.fns.push(NativeFnEntry {
            name,
            requires_unsafe,
            handler,
        });
        id
    }

    /// Create a new module, returning its ID.
    pub fn create_module(&mut self, name: impl Into<String>) -> ModuleId {
        let id = ModuleId(self.modules.len() as u32);
        self.modules.push(Module::new(name));
        id
    }

    /// Add a sub-module to a parent module.
    pub fn add_submodule(&mut self, parent: ModuleId, name: impl Into<String>, child: ModuleId) {
        self.modules[parent.0 as usize]
            .entries
            .insert(name.into(), Value::Module(child));
    }

    /// Add a native function to a module.
    pub fn add_fn(&mut self, module: ModuleId, fn_id: NativeFnId) {
        let name = self.fns[fn_id.0 as usize].name.to_string();
        self.modules[module.0 as usize]
            .entries
            .insert(name, Value::NativeFn(fn_id));
    }

    /// Add a KS-defined value to a module.
    pub fn add_value(&mut self, module: ModuleId, name: impl Into<String>, value: Value) {
        self.modules[module.0 as usize]
            .entries
            .insert(name.into(), value);
    }

    /// Look up a native function entry by ID.
    pub fn get(&self, id: NativeFnId) -> &NativeFnEntry {
        &self.fns[id.0 as usize]
    }

    /// Look up a module by ID.
    pub fn get_module(&self, id: ModuleId) -> &Module {
        &self.modules[id.0 as usize]
    }

    /// Get the name of a native function by ID.
    pub fn fn_name(&self, id: NativeFnId) -> &str {
        self.fns[id.0 as usize].name
    }

    /// Find a submodule by name within a parent module.
    pub fn find_submodule(&self, parent: ModuleId, name: &str) -> Option<ModuleId> {
        match self.modules[parent.0 as usize].entries.get(name) {
            Some(Value::Module(mid)) => Some(*mid),
            _ => None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Native function implementations
// ═══════════════════════════════════════════════════════════════════

// ── Top-level builtins ──────────────────────────────────────────────

pub fn native_print(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let parts: Vec<String> = args.iter().map(|v| v.display(ctx.types)).collect();
    writeln!(ctx.out, "{}", parts.join(" ")).map_err(|e| ErrorKind::Other(e.to_string()))?;
    Ok(Value::Nil)
}

pub fn native_panic(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let msg = if args.is_empty() {
        "panic".to_string()
    } else {
        args.iter()
            .map(|v| v.display(ctx.types))
            .collect::<Vec<_>>()
            .join(" ")
    };
    Err(ErrorKind::Panic { message: msg }.into())
}

pub fn native_typeof(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(ErrorKind::ArityMismatch {
            target: ArityTarget::Builtin {
                name: "typeof".into(),
            },
            expected: 1,
            actual: args.len(),
        }
        .into());
    }
    Ok(Value::Type(args[0].type_id()))
}

// ── std.ops — binary operators ──────────────────────────────────────

fn binop_native(_ctx: &mut NativeCtx, args: &[Value], op: BinOp) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(ErrorKind::ArityMismatch {
            target: ArityTarget::Builtin {
                name: format!("std.ops.{}", op.method_name()),
            },
            expected: 2,
            actual: args.len(),
        }
        .into());
    }
    eval_binop(op, &args[0], &args[1]).map_err(RuntimeError::from)
}

pub fn native_ops_add(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    binop_native(ctx, args, BinOp::Add)
}
pub fn native_ops_sub(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    binop_native(ctx, args, BinOp::Sub)
}
pub fn native_ops_mul(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    binop_native(ctx, args, BinOp::Mul)
}
pub fn native_ops_div(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    binop_native(ctx, args, BinOp::Div)
}
pub fn native_ops_eq(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    binop_native(ctx, args, BinOp::Eq)
}
pub fn native_ops_ne(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    binop_native(ctx, args, BinOp::Ne)
}
pub fn native_ops_lt(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    binop_native(ctx, args, BinOp::Lt)
}
pub fn native_ops_gt(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    binop_native(ctx, args, BinOp::Gt)
}
pub fn native_ops_le(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    binop_native(ctx, args, BinOp::Le)
}
pub fn native_ops_ge(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    binop_native(ctx, args, BinOp::Ge)
}

// ── std.ops — unary operators ───────────────────────────────────────

fn unaryop_native(
    _ctx: &mut NativeCtx,
    args: &[Value],
    op: UnaryOp,
) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(ErrorKind::ArityMismatch {
            target: ArityTarget::Builtin {
                name: format!("std.ops.{}", op.method_name()),
            },
            expected: 1,
            actual: args.len(),
        }
        .into());
    }
    eval_unaryop(op, &args[0]).map_err(RuntimeError::from)
}

pub fn native_ops_neg(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    unaryop_native(ctx, args, UnaryOp::Neg)
}
pub fn native_ops_not(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    unaryop_native(ctx, args, UnaryOp::Not)
}

pub fn native_ops_truth(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(ErrorKind::ArityMismatch {
            target: ArityTarget::Builtin {
                name: "std.ops.truth".into(),
            },
            expected: 1,
            actual: args.len(),
        }
        .into());
    }
    Ok(Value::Bool(truth(&args[0])))
}

// ── std.mem — allocation intrinsics ─────────────────────────────────
//
// Minimal runtime escape hatch for memory management. All take/return
// RawPtr (opaque handle). Only callable inside `unsafe` blocks.

pub fn native_mem_alloc(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let cap = ctx.expect_int(args, 0)?;
    let id = ctx.allocations.len() as u32;
    ctx.allocations.push(Some(Vec::with_capacity(cap)));
    Ok(Value::RawPtr(id))
}

pub fn native_mem_free(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let id = ctx.expect_rawptr(args, 0)?;
    let slot = ctx
        .allocations
        .get_mut(id as usize)
        .ok_or(ErrorKind::UseAfterFree)?;
    if slot.is_none() {
        return Err(ErrorKind::UseAfterFree.into());
    }
    *slot = None;
    Ok(Value::Nil)
}

pub fn native_mem_read(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let id = ctx.expect_rawptr(args, 0)?;
    let idx = ctx.expect_int(args, 1)?;
    let alloc = ctx
        .allocations
        .get(id as usize)
        .and_then(|s| s.as_ref())
        .ok_or(ErrorKind::UseAfterFree)?;
    Ok(alloc.get(idx).cloned().unwrap_or(Value::Nil))
}

pub fn native_mem_write(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let id = ctx.expect_rawptr(args, 0)?;
    let idx = ctx.expect_int(args, 1)?;
    let val = args.get(2).cloned().unwrap_or(Value::Nil);
    let alloc = ctx
        .allocations
        .get_mut(id as usize)
        .and_then(|s| s.as_mut())
        .ok_or(ErrorKind::UseAfterFree)?;
    while alloc.len() <= idx {
        alloc.push(Value::Nil);
    }
    alloc[idx] = val;
    Ok(Value::Nil)
}

pub fn native_mem_capacity(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let id = ctx.expect_rawptr(args, 0)?;
    let alloc = ctx
        .allocations
        .get(id as usize)
        .and_then(|s| s.as_ref())
        .ok_or(ErrorKind::UseAfterFree)?;
    Ok(Value::Int(BigInt::from(alloc.capacity())))
}

pub fn native_mem_len(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let id = ctx.expect_rawptr(args, 0)?;
    let alloc = ctx
        .allocations
        .get(id as usize)
        .and_then(|s| s.as_ref())
        .ok_or(ErrorKind::UseAfterFree)?;
    Ok(Value::Int(BigInt::from(alloc.len())))
}

// ═══════════════════════════════════════════════════════════════════
// Operator logic (used by both native fns and direct eval)
// ═══════════════════════════════════════════════════════════════════

/// Truthiness: nil, false, 0, 0.0, "" are falsy; everything else truthy.
pub fn truth(v: &Value) -> bool {
    match v {
        Value::Nil => false,
        Value::Bool(b) => *b,
        Value::Int(n) => !n.is_zero(),
        Value::Float(f) => *f != 0.0,
        Value::Str(s) => !s.is_empty(),
        _ => true,
    }
}

/// Evaluate a binary operator on two values.
pub fn eval_binop(op: BinOp, left: &Value, right: &Value) -> Result<Value, ErrorKind> {
    match op {
        BinOp::Add => op_add(left, right),
        BinOp::Sub | BinOp::Mul => op_arith(op, left, right),
        BinOp::Div => op_div(left, right),
        BinOp::Eq => Ok(Value::Bool(left == right || cross_eq(left, right))),
        BinOp::Ne => Ok(Value::Bool(left != right && !cross_eq(left, right))),
        BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => op_cmp(op, left, right),
    }
}

/// Evaluate a unary operator.
pub fn eval_unaryop(op: UnaryOp, operand: &Value) -> Result<Value, ErrorKind> {
    match op {
        UnaryOp::Neg => match operand {
            Value::Int(n) => Ok(Value::Int(-n)),
            Value::Float(f) => Ok(Value::Float(-f)),
            _ => Err(ErrorKind::UnaryOpType {
                op,
                operand: operand.type_id(),
            }),
        },
        UnaryOp::Not => Ok(Value::Bool(!truth(operand))),
    }
}

// ── Operator helpers ────────────────────────────────────────────────

/// Convert a BigInt to f64 for mixed-type arithmetic. Errors if the
/// integer is too large to represent as a float (avoids silent data loss).
fn int_to_f64(n: &BigInt) -> Result<f64, ErrorKind> {
    n.to_f64().ok_or(ErrorKind::IntegerOverflow)
}

fn op_add(left: &Value, right: &Value) -> Result<Value, ErrorKind> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(int_to_f64(a)? + b)),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + int_to_f64(b)?)),
        (Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{a}{b}"))),
        _ => Err(ErrorKind::BinOpType {
            op: BinOp::Add,
            left: left.type_id(),
            right: right.type_id(),
        }),
    }
}

fn op_arith(op: BinOp, left: &Value, right: &Value) -> Result<Value, ErrorKind> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => match op {
            BinOp::Sub => Ok(Value::Int(a - b)),
            BinOp::Mul => Ok(Value::Int(a * b)),
            _ => unreachable!(),
        },
        (Value::Float(a), Value::Float(b)) => {
            let r = match op {
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                _ => unreachable!(),
            };
            Ok(Value::Float(r))
        }
        (Value::Int(a), Value::Float(b)) => {
            let a = int_to_f64(a)?;
            let r = match op {
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                _ => unreachable!(),
            };
            Ok(Value::Float(r))
        }
        (Value::Float(a), Value::Int(b)) => {
            let b = int_to_f64(b)?;
            let r = match op {
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                _ => unreachable!(),
            };
            Ok(Value::Float(r))
        }
        _ => Err(ErrorKind::BinOpType {
            op,
            left: left.type_id(),
            right: right.type_id(),
        }),
    }
}

fn op_div(left: &Value, right: &Value) -> Result<Value, ErrorKind> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => {
            if b.is_zero() {
                return Err(ErrorKind::DivisionByZero);
            }
            Ok(Value::Int(a / b))
        }
        (Value::Float(a), Value::Float(b)) => {
            if *b == 0.0 {
                return Err(ErrorKind::DivisionByZero);
            }
            Ok(Value::Float(a / b))
        }
        (Value::Int(a), Value::Float(b)) => {
            if *b == 0.0 {
                return Err(ErrorKind::DivisionByZero);
            }
            Ok(Value::Float(int_to_f64(a)? / b))
        }
        (Value::Float(a), Value::Int(b)) => {
            if b.is_zero() {
                return Err(ErrorKind::DivisionByZero);
            }
            Ok(Value::Float(a / int_to_f64(b)?))
        }
        _ => Err(ErrorKind::BinOpType {
            op: BinOp::Div,
            left: left.type_id(),
            right: right.type_id(),
        }),
    }
}

fn cross_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Int(a), Value::Float(b)) => a.to_f64().map_or(false, |a| a == *b),
        (Value::Float(a), Value::Int(b)) => b.to_f64().map_or(false, |b| *a == b),
        _ => false,
    }
}

fn op_cmp(op: BinOp, left: &Value, right: &Value) -> Result<Value, ErrorKind> {
    let ord = match (left, right) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b).ok_or(ErrorKind::NanComparison)?,
        (Value::Int(a), Value::Float(b)) => {
            let a = int_to_f64(a)?;
            a.partial_cmp(b).ok_or(ErrorKind::NanComparison)?
        }
        (Value::Float(a), Value::Int(b)) => {
            let b = int_to_f64(b)?;
            a.partial_cmp(&b).ok_or(ErrorKind::NanComparison)?
        }
        (Value::Str(a), Value::Str(b)) => a.cmp(b),
        (Value::Byte(a), Value::Byte(b)) => a.cmp(b),
        (Value::Char(a), Value::Char(b)) => a.cmp(b),
        _ => {
            return Err(ErrorKind::BinOpType {
                op,
                left: left.type_id(),
                right: right.type_id(),
            })
        }
    };
    let result = match op {
        BinOp::Lt => ord.is_lt(),
        BinOp::Gt => ord.is_gt(),
        BinOp::Le => ord.is_le(),
        BinOp::Ge => ord.is_ge(),
        _ => unreachable!(),
    };
    Ok(Value::Bool(result))
}

// ═══════════════════════════════════════════════════════════════════
// Native methods for Byte and Char
// ═══════════════════════════════════════════════════════════════════

// ── Byte methods ─────────────────────────────────────────────────

fn byte_to_int(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Byte(b) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::BYTE,
            actual: args[0].type_id(),
        }
        .into());
    };
    Ok(Value::Int(BigInt::from(*b)))
}

fn byte_and(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let (Value::Byte(a), Value::Byte(b)) = (&args[0], &args[1]) else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::BYTE,
            actual: args[1].type_id(),
        }
        .into());
    };
    Ok(Value::Byte(a & b))
}

fn byte_or(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let (Value::Byte(a), Value::Byte(b)) = (&args[0], &args[1]) else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::BYTE,
            actual: args[1].type_id(),
        }
        .into());
    };
    Ok(Value::Byte(a | b))
}

fn byte_xor(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let (Value::Byte(a), Value::Byte(b)) = (&args[0], &args[1]) else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::BYTE,
            actual: args[1].type_id(),
        }
        .into());
    };
    Ok(Value::Byte(a ^ b))
}

fn byte_not(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Byte(b) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::BYTE,
            actual: args[0].type_id(),
        }
        .into());
    };
    Ok(Value::Byte(!b))
}

fn byte_shl(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Byte(b) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::BYTE,
            actual: args[0].type_id(),
        }
        .into());
    };
    let Value::Int(n) = &args[1] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::INT,
            actual: args[1].type_id(),
        }
        .into());
    };
    let shift: u32 = n.try_into().map_err(|_| ErrorKind::IntegerOverflow)?;
    Ok(Value::Byte(b.wrapping_shl(shift)))
}

fn byte_shr(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Byte(b) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::BYTE,
            actual: args[0].type_id(),
        }
        .into());
    };
    let Value::Int(n) = &args[1] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::INT,
            actual: args[1].type_id(),
        }
        .into());
    };
    let shift: u32 = n.try_into().map_err(|_| ErrorKind::IntegerOverflow)?;
    Ok(Value::Byte(b.wrapping_shr(shift)))
}

// ── Char methods ─────────────────────────────────────────────────

fn char_to_int(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Char(c) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::CHAR,
            actual: args[0].type_id(),
        }
        .into());
    };
    Ok(Value::Int(BigInt::from(*c as u32)))
}

fn char_is_alpha(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Char(c) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::CHAR,
            actual: args[0].type_id(),
        }
        .into());
    };
    Ok(Value::Bool(c.is_alphabetic()))
}

fn char_is_digit(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Char(c) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::CHAR,
            actual: args[0].type_id(),
        }
        .into());
    };
    Ok(Value::Bool(c.is_ascii_digit()))
}

fn char_is_upper(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Char(c) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::CHAR,
            actual: args[0].type_id(),
        }
        .into());
    };
    Ok(Value::Bool(c.is_uppercase()))
}

fn char_is_lower(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Char(c) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::CHAR,
            actual: args[0].type_id(),
        }
        .into());
    };
    Ok(Value::Bool(c.is_lowercase()))
}

fn char_to_upper(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Char(c) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::CHAR,
            actual: args[0].type_id(),
        }
        .into());
    };
    // to_uppercase can produce multiple chars; take the first.
    Ok(Value::Char(c.to_uppercase().next().unwrap_or(*c)))
}

fn char_to_lower(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Char(c) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::CHAR,
            actual: args[0].type_id(),
        }
        .into());
    };
    Ok(Value::Char(c.to_lowercase().next().unwrap_or(*c)))
}

// ── Str methods ─────────────────────────────────────────────────────

/// Helper: extract &str from args[0], or return TypeMismatch.
fn expect_str(args: &[Value], pos: usize) -> Result<&str, RuntimeError> {
    match args.get(pos) {
        Some(Value::Str(s)) => Ok(s.as_str()),
        Some(other) => Err(ErrorKind::TypeMismatch {
            expected: prim::STR,
            actual: other.type_id(),
        }
        .into()),
        None => Err(ErrorKind::InternalError("missing argument to native function").into()),
    }
}

fn str_len(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    Ok(Value::Int(BigInt::from(s.len())))
}

fn str_char_len(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    Ok(Value::Int(BigInt::from(s.chars().count())))
}

fn str_contains(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    let needle = expect_str(args, 1)?;
    Ok(Value::Bool(s.contains(needle)))
}

fn str_starts_with(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    let prefix = expect_str(args, 1)?;
    Ok(Value::Bool(s.starts_with(prefix)))
}

fn str_ends_with(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    let suffix = expect_str(args, 1)?;
    Ok(Value::Bool(s.ends_with(suffix)))
}

fn str_trim(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    Ok(Value::Str(s.trim().to_string()))
}

fn str_trim_start(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    Ok(Value::Str(s.trim_start().to_string()))
}

fn str_trim_end(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    Ok(Value::Str(s.trim_end().to_string()))
}

fn str_to_upper(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    Ok(Value::Str(s.to_uppercase()))
}

fn str_to_lower(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    Ok(Value::Str(s.to_lowercase()))
}

fn str_replace(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    let from = expect_str(args, 1)?;
    let to = expect_str(args, 2)?;
    Ok(Value::Str(s.replace(from, to)))
}

fn str_substr(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    let start = match args.get(1) {
        Some(Value::Int(n)) => n.to_usize().ok_or(ErrorKind::IntegerOverflow)?,
        Some(other) => {
            return Err(ErrorKind::TypeMismatch {
                expected: prim::INT,
                actual: other.type_id(),
            }
            .into())
        }
        None => return Err(ErrorKind::InternalError("missing argument").into()),
    };
    let len = match args.get(2) {
        Some(Value::Int(n)) => n.to_usize().ok_or(ErrorKind::IntegerOverflow)?,
        Some(other) => {
            return Err(ErrorKind::TypeMismatch {
                expected: prim::INT,
                actual: other.type_id(),
            }
            .into())
        }
        None => return Err(ErrorKind::InternalError("missing argument").into()),
    };
    // Byte-index substr. Clamp to string bounds.
    let end = (start + len).min(s.len());
    let start = start.min(s.len());
    // Ensure we don't split a UTF-8 char.
    if !s.is_char_boundary(start) || !s.is_char_boundary(end) {
        return Err(ErrorKind::Other("substr: index splits a UTF-8 character".to_string()).into());
    }
    Ok(Value::Str(s[start..end].to_string()))
}

fn str_split(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    let delim = expect_str(args, 1)?;
    // Returns an Arr-style display for now — actually we need to return a proper array.
    // For now, return a comma-joined string of parts. TODO: return Arr[Str] when we can
    // construct arrays from native code.
    let parts: Vec<&str> = s.split(delim).collect();
    let joined = parts.join("\n");
    Ok(Value::Str(joined))
}

fn str_to_int(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    let n: BigInt = s.trim().parse().map_err(|_| -> RuntimeError {
        ErrorKind::Other(format!("cannot parse '{s}' as Int")).into()
    })?;
    Ok(Value::Int(n))
}

fn str_to_float(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    let n: f64 = s.trim().parse().map_err(|_| -> RuntimeError {
        ErrorKind::Other(format!("cannot parse '{s}' as Float")).into()
    })?;
    Ok(Value::Float(n))
}

fn str_to_bin(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let s = expect_str(args, 0)?;
    Ok(ctx.intern_bin(s.as_bytes().to_vec()))
}

// ── Bin methods ─────────────────────────────────────────────────────

fn bin_len(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Bin(b) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::BIN,
            actual: args[0].type_id(),
        }
        .into());
    };
    Ok(Value::Int(BigInt::from(b.len())))
}

fn bin_get_item(_ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Bin(b) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::BIN,
            actual: args[0].type_id(),
        }
        .into());
    };
    let Value::Int(idx) = &args[1] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::INT,
            actual: args[1].type_id(),
        }
        .into());
    };
    let i = idx.to_usize().ok_or(ErrorKind::IntegerOverflow)?;
    if i >= b.len() {
        return Err(
            ErrorKind::Other(format!("Bin index out of bounds: {i}, len {}", b.len())).into(),
        );
    }
    Ok(Value::Byte(b[i]))
}

/// `std.mem.bin_from_raw(raw: RawPtr, len: Int) -> Bin`
///
/// Read `len` Byte values from the allocation at `raw` and intern as a Bin.
/// Unsafe — accesses raw memory.
fn bin_from_raw(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let id = ctx.expect_rawptr(args, 0)?;
    let len = ctx.expect_int(args, 1)?;
    let alloc = ctx
        .allocations
        .get(id as usize)
        .and_then(|s| s.as_ref())
        .ok_or(ErrorKind::UseAfterFree)?;
    let mut bytes = Vec::with_capacity(len);
    for i in 0..len {
        match alloc.get(i) {
            Some(Value::Byte(b)) => bytes.push(*b),
            Some(other) => {
                return Err(ErrorKind::TypeMismatch {
                    expected: prim::BYTE,
                    actual: other.type_id(),
                }
                .into())
            }
            None => {
                return Err(
                    ErrorKind::Other(format!("bin_from_raw: index {i} out of bounds")).into(),
                )
            }
        }
    }
    Ok(ctx.intern_bin(bytes))
}

/// `Bin.from_base64(s: Str) -> Bin` — static method.
/// Decode a base64-encoded string into bytes.
fn bin_from_base64(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let Value::Str(s) = &args[0] else {
        return Err(ErrorKind::TypeMismatch {
            expected: prim::STR,
            actual: args[0].type_id(),
        }
        .into());
    };
    let bytes = base64_decode(s)
        .map_err(|e| -> RuntimeError { ErrorKind::Other(format!("base64 decode: {e}")).into() })?;
    Ok(ctx.intern_bin(bytes))
}

/// Minimal base64 decoder (RFC 4648). No dependencies.
fn base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    const TABLE: &[u8; 128] = &{
        let mut t = [0xFFu8; 128];
        let mut i = 0u8;
        while i < 26 {
            t[(b'A' + i) as usize] = i;
            t[(b'a' + i) as usize] = i + 26;
            i += 1;
        }
        let mut d = 0u8;
        while d < 10 {
            t[(b'0' + d) as usize] = d + 52;
            d += 1;
        }
        t[b'+' as usize] = 62;
        t[b'/' as usize] = 63;
        t
    };

    let input: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if input.is_empty() {
        return Ok(vec![]);
    }

    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let chunks = input.chunks(4);
    for chunk in chunks {
        let len = chunk.len();
        if len < 2 {
            return Err("invalid base64 length");
        }
        let mut buf = [0u32; 4];
        let mut pad = 0;
        for (i, &b) in chunk.iter().enumerate() {
            if b == b'=' {
                pad += 1;
                buf[i] = 0;
            } else if b >= 128 || TABLE[b as usize] == 0xFF {
                return Err("invalid base64 character");
            } else {
                buf[i] = TABLE[b as usize] as u32;
            }
        }
        // Pad remaining slots for short final chunk.
        for slot in buf.iter_mut().skip(len) {
            *slot = 0;
            pad += 1;
        }
        let combined = (buf[0] << 18) | (buf[1] << 12) | (buf[2] << 6) | buf[3];
        out.push((combined >> 16) as u8);
        if pad < 2 {
            out.push((combined >> 8) as u8);
        }
        if pad < 1 {
            out.push(combined as u8);
        }
    }
    Ok(out)
}

// ═══════════════════════════════════════════════════════════════════
// Boot: build the module tree with all native functions
// ═══════════════════════════════════════════════════════════════════

/// Native method registrations for a prim type.
pub struct PrimMethods {
    pub methods: Vec<(&'static str, NativeFnId)>,
}

/// Result of bootstrapping: the registry + IDs needed by the interpreter.
pub struct BootResult {
    pub registry: NativeFnRegistry,
    pub std_module: ModuleId,
    pub print_id: NativeFnId,
    pub typeof_id: NativeFnId,
    pub panic_id: NativeFnId,
    pub byte_methods: PrimMethods,
    pub char_methods: PrimMethods,
    pub str_methods: PrimMethods,
    pub bin_methods: PrimMethods,
}

/// Build the complete native function registry and module tree.
pub fn bootstrap() -> BootResult {
    let mut reg = NativeFnRegistry::new();

    // Top-level builtins.
    let print_id = reg.register("print", false, native_print);
    let typeof_id = reg.register("typeof", false, native_typeof);
    let panic_id = reg.register("panic", false, native_panic);

    // std.ops module.
    let ops = reg.create_module("ops");
    for (name, handler) in [
        ("add", native_ops_add as NativeHandler),
        ("sub", native_ops_sub),
        ("mul", native_ops_mul),
        ("div", native_ops_div),
        ("eq", native_ops_eq),
        ("ne", native_ops_ne),
        ("lt", native_ops_lt),
        ("gt", native_ops_gt),
        ("le", native_ops_le),
        ("ge", native_ops_ge),
        ("neg", native_ops_neg),
        ("not", native_ops_not),
        ("truth", native_ops_truth),
    ] {
        let id = reg.register(name, false, handler);
        reg.add_fn(ops, id);
    }

    // std.mem module (all unsafe).
    let mem = reg.create_module("mem");
    for (name, handler) in [
        ("alloc", native_mem_alloc as NativeHandler),
        ("free", native_mem_free),
        ("read", native_mem_read),
        ("write", native_mem_write),
        ("capacity", native_mem_capacity),
        ("len", native_mem_len),
        ("bin_from_raw", bin_from_raw),
    ] {
        let id = reg.register(name, true, handler);
        reg.add_fn(mem, id);
    }

    // std root module.
    let std_module = reg.create_module("std");
    reg.add_submodule(std_module, "ops", ops);
    reg.add_submodule(std_module, "mem", mem);

    // Byte native methods.
    let byte_methods = {
        let mut methods = Vec::new();
        for (name, handler) in [
            ("to_int", byte_to_int as NativeHandler),
            ("band", byte_and),
            ("ior", byte_or),
            ("xor", byte_xor),
            ("inv", byte_not),
            ("shl", byte_shl),
            ("shr", byte_shr),
        ] {
            let id = reg.register(name, false, handler);
            methods.push((name, id));
        }
        PrimMethods { methods }
    };

    // Char native methods.
    let char_methods = {
        let mut methods = Vec::new();
        for (name, handler) in [
            ("to_int", char_to_int as NativeHandler),
            ("is_alpha", char_is_alpha),
            ("is_digit", char_is_digit),
            ("is_upper", char_is_upper),
            ("is_lower", char_is_lower),
            ("to_upper", char_to_upper),
            ("to_lower", char_to_lower),
        ] {
            let id = reg.register(name, false, handler);
            methods.push((name, id));
        }
        PrimMethods { methods }
    };

    // Str native methods.
    let str_methods = {
        let mut methods = Vec::new();
        for (name, handler) in [
            ("len", str_len as NativeHandler),
            ("char_len", str_char_len),
            ("contains", str_contains),
            ("starts_with", str_starts_with),
            ("ends_with", str_ends_with),
            ("trim", str_trim),
            ("trim_start", str_trim_start),
            ("trim_end", str_trim_end),
            ("to_upper", str_to_upper),
            ("to_lower", str_to_lower),
            ("replace", str_replace),
            ("substr", str_substr),
            ("split", str_split),
            ("to_int", str_to_int),
            ("to_float", str_to_float),
            ("to_bin", str_to_bin),
        ] {
            let id = reg.register(name, false, handler);
            methods.push((name, id));
        }
        PrimMethods { methods }
    };

    // Bin native methods.
    let bin_methods = {
        let mut methods = Vec::new();
        for (name, handler) in [
            ("len", bin_len as NativeHandler),
            ("get_item", bin_get_item),
            ("from_base64", bin_from_base64),
        ] {
            let id = reg.register(name, false, handler);
            methods.push((name, id));
        }
        PrimMethods { methods }
    };

    BootResult {
        registry: reg,
        std_module,
        print_id,
        typeof_id,
        panic_id,
        byte_methods,
        char_methods,
        str_methods,
        bin_methods,
    }
}

// ── Debug impls ─────────────────────────────────────────────────────

impl std::fmt::Debug for NativeFnEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeFnEntry")
            .field("name", &self.name)
            .field("requires_unsafe", &self.requires_unsafe)
            .finish()
    }
}

impl std::fmt::Display for NativeFnId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NativeFnId({})", self.0)
    }
}

impl std::fmt::Display for ModuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ModuleId({})", self.0)
    }
}
