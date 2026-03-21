# Decision: generics
**ID:** generics
**Status:** decided
**Date opened:** 2026-03-21
**Date done:** 2026-03-21
**Affects:** parser, types, interpreter

## Question
How do generic type parameters work in KataScript?

## Context
KataScript supports generic type parameters on `enum`, `kind`, and `type` definitions. This decision documents the precise semantics — how parameters are declared, stored, resolved, instantiated, cached, and type-checked.

## Design

### Declaration
Type parameters are declared in square brackets after the type name:

```
enum Opt[T] { Val(T), Non }
kind Pair[A, B] { fst: A, snd: B }
type Iter[T] { func next(self): Opt[T] }
```

Parameters are positional. The name is source-level syntax only — internally, each parameter is identified by its index in the declaration list (0, 1, 2, ...).

### Instantiation
Generic types are instantiated with concrete type arguments in square brackets:

```
Opt[Int]
Pair[Str, Float]
Res[Int, Str]
```

Nested generics work: `Opt[Opt[Int]]`, `Res[Opt[Str], Int]`.

### Positional semantics
Type parameters are resolved by position, not by name:

```
enum Res[T, E] { Val(T), Err(E) }
```

- `T` is parameter index 0. In `Res[Int, Str]`, `T` resolves to `Int`.
- `E` is parameter index 1. In `Res[Int, Str]`, `E` resolves to `Str`.

Swapping arguments changes the types: `Res[Str, Int].Val("hello")` is valid; `Res[Int, Str].Val("hello")` is a type error.

### Parameter reuse
The same parameter can appear in multiple fields or variant positions:

```
kind Point[T] { x: T, y: T }
```

Both `x` and `y` resolve to the same type argument.

### Mixed concrete and parameter fields
Fields can mix concrete types and type parameters:

```
kind Named[T] { name: Str, value: T }
```

`name` is always `Str`; `value` depends on the type argument.

### Multi-field variants
A single variant can use multiple type parameters:

```
enum Either[L, R] { Left(L), Right(R), Both(L, R) }
```

### Phantom parameters
A declared type parameter need not appear in any field:

```
enum Unit[T] { Val }
```

`Unit[Int]` and `Unit[Str]` are distinct types (different type arguments produce different instances) even though they have identical runtime behavior.

### Type checking
At construction time, each field value is checked against the resolved type:

```
let x = Opt[Int].Val(42)    # OK
let y = Opt[Int].Val("no")   # type mismatch: expected Int, got Str
```

Arity is also checked:

```
let x = Opt[Int, Str].Val(1)  # 'Opt' expects 1 type argument(s), got 2
```

### Instantiation caching
Each `(base_type_id, type_args)` pair is instantiated at most once. Subsequent uses of the same instantiation return the same `TypeId`:

```
typeof(Opt[Int].Val(1)) == typeof(Opt[Int].Val(2))  # true
```

### Implementation details
- **AST layer**: Parameters are `Vec<String>` — source text. This is correct.
- **Registry layer**: Parameters become `TypeExpr::Param(usize)` — positional indices. `TypeExpr::Concrete(TypeId)` for non-parameter fields. No strings in the resolution path.
- **Instantiation**: Direct index lookup `type_args[idx]` — O(1), no HashMap.
- **Translation boundary**: `resolve_type_ann` in the interpreter is the single point where parameter names (strings) become positional indices. Everything downstream is index-based.

### Known limitations
- **Interface conformance type args**: In `impl K as Iter[Int] { ... }`, the `[Int]` type arguments on the interface are parsed but not validated during conformance checking.
- **No bounds/constraints**: There is no way to constrain a type parameter (e.g., `T: Iter`). All type parameters accept any type.
- **No type parameter inference**: Type arguments must always be written explicitly. `Opt.Val(42)` does not infer `Opt[Int]`.

## Decision
Generics use positional parameter indices. The AST stores parameter names as strings (source text). The type registry and interpreter use `TypeExpr::Param(usize)` — the index into the definition's parameter list. Instantiation resolves parameters via direct indexing into `type_args`. Caching is keyed on `(base_id, type_args)`.

## References
- `katars/src/ks/types.rs` — `TypeExpr`, `TypeDef`, `TypeRegistry`
- `katars/src/ks/interpreter.rs` — `resolve_type_ann` (translation boundary)
- `demos/generics.ks` — comprehensive examples
- `demos/generic_patterns.ks` — usage patterns with functions and control flow
- [spec: type-system](type-system.md) — parent type system architecture
