//! Native function registry, module tree, and all builtin implementations.
//!
//! Native functions are Rust functions with the signature:
//!     `fn(&mut NativeCtx, &[Value]) -> Result<Value, RuntimeError>`
//!
//! They are registered at boot time and organized into a module tree.
//! At call time, `Value::NativeFn(NativeFnId)` is looked up by ID
//! and dispatched — no string matching.

use std::io::Write;

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

/// A node in the module tree. Contains sub-modules and native function entries.
#[derive(Debug)]
pub struct Module {
    pub name: String,
    pub entries: IndexMap<String, ModuleItem>,
}

/// An entry in a module — either a sub-module or a native function.
#[derive(Debug)]
pub enum ModuleItem {
    SubModule(ModuleId),
    NativeFn(NativeFnId),
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
    pub out: &'a mut dyn Write,
    /// Available for native fns that need to know if they're in an unsafe context.
    /// Currently checked at the dispatch level before the handler is called.
    #[allow(dead_code)]
    pub in_unsafe: bool,
}

impl<'a> NativeCtx<'a> {
    /// Extract a usize from args at the given position, expecting an Int.
    pub fn expect_int(&self, args: &[Value], pos: usize) -> Result<usize, RuntimeError> {
        match args.get(pos) {
            Some(Value::Int(n)) => n
                .to_usize()
                .ok_or_else(|| ErrorKind::Other("integer out of range".into()).into()),
            Some(other) => Err(ErrorKind::TypeMismatch {
                expected: prim::INT,
                actual: other.type_id(),
            }
            .into()),
            None => Err(ErrorKind::Other("missing argument".into()).into()),
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
            .insert(name.into(), ModuleItem::SubModule(child));
    }

    /// Add a native function to a module.
    pub fn add_fn(&mut self, module: ModuleId, fn_id: NativeFnId) {
        let name = self.fns[fn_id.0 as usize].name.to_string();
        self.modules[module.0 as usize]
            .entries
            .insert(name, ModuleItem::NativeFn(fn_id));
    }

    /// Look up a native function entry by ID.
    pub fn get(&self, id: NativeFnId) -> &NativeFnEntry {
        &self.fns[id.0 as usize]
    }

    /// Look up a module by ID.
    pub fn get_module(&self, id: ModuleId) -> &Module {
        &self.modules[id.0 as usize]
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

pub fn native_mem_alloc(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let cap = ctx.expect_int(args, 0)?;
    let id = ctx.allocations.len();
    ctx.allocations.push(Some(Vec::with_capacity(cap)));
    Ok(Value::Int(BigInt::from(id)))
}

pub fn native_mem_dealloc(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let id = ctx.expect_int(args, 0)?;
    let slot = ctx.allocations.get_mut(id).ok_or(ErrorKind::UseAfterFree)?;
    if slot.is_none() {
        return Err(ErrorKind::UseAfterFree.into());
    }
    *slot = None;
    Ok(Value::Nil)
}

pub fn native_mem_read(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let id = ctx.expect_int(args, 0)?;
    let idx = ctx.expect_int(args, 1)?;
    let alloc = ctx
        .allocations
        .get(id)
        .and_then(|s| s.as_ref())
        .ok_or(ErrorKind::UseAfterFree)?;
    Ok(alloc.get(idx).cloned().unwrap_or(Value::Nil))
}

pub fn native_mem_write(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let id = ctx.expect_int(args, 0)?;
    let idx = ctx.expect_int(args, 1)?;
    let val = args.get(2).cloned().unwrap_or(Value::Nil);
    let alloc = ctx
        .allocations
        .get_mut(id)
        .and_then(|s| s.as_mut())
        .ok_or(ErrorKind::UseAfterFree)?;
    while alloc.len() <= idx {
        alloc.push(Value::Nil);
    }
    alloc[idx] = val;
    Ok(Value::Nil)
}

pub fn native_mem_grow(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let id = ctx.expect_int(args, 0)?;
    let new_cap = ctx.expect_int(args, 1)?;
    let alloc = ctx
        .allocations
        .get_mut(id)
        .and_then(|s| s.as_mut())
        .ok_or(ErrorKind::UseAfterFree)?;
    if new_cap > alloc.capacity() {
        alloc.reserve(new_cap - alloc.capacity());
    }
    Ok(Value::Nil)
}

pub fn native_mem_capacity(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let id = ctx.expect_int(args, 0)?;
    let alloc = ctx
        .allocations
        .get(id)
        .and_then(|s| s.as_ref())
        .ok_or(ErrorKind::UseAfterFree)?;
    Ok(Value::Int(BigInt::from(alloc.capacity())))
}

pub fn native_mem_len(ctx: &mut NativeCtx, args: &[Value]) -> Result<Value, RuntimeError> {
    let id = ctx.expect_int(args, 0)?;
    let alloc = ctx
        .allocations
        .get(id)
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
    n.to_f64()
        .ok_or_else(|| ErrorKind::Other(format!("integer too large for float conversion: {n}")))
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
// Boot: build the module tree with all native functions
// ═══════════════════════════════════════════════════════════════════

/// Result of bootstrapping: the registry + IDs needed by the interpreter.
pub struct BootResult {
    pub registry: NativeFnRegistry,
    pub std_module: ModuleId,
    pub print_id: NativeFnId,
    pub typeof_id: NativeFnId,
}

/// Build the complete native function registry and module tree.
pub fn bootstrap() -> BootResult {
    let mut reg = NativeFnRegistry::new();

    // Top-level builtins.
    let print_id = reg.register("print", false, native_print);
    let typeof_id = reg.register("typeof", false, native_typeof);

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
        ("dealloc", native_mem_dealloc),
        ("read", native_mem_read),
        ("write", native_mem_write),
        ("grow", native_mem_grow),
        ("capacity", native_mem_capacity),
        ("len", native_mem_len),
    ] {
        let id = reg.register(name, true, handler);
        reg.add_fn(mem, id);
    }

    // std root module.
    let std_module = reg.create_module("std");
    reg.add_submodule(std_module, "ops", ops);
    reg.add_submodule(std_module, "mem", mem);

    BootResult {
        registry: reg,
        std_module,
        print_id,
        typeof_id,
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
