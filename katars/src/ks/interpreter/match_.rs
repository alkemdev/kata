//! Pattern matching: `ResolvedPattern`, `eval_match`, the resolution
//! pass that turns AST patterns into handle-based form, structural
//! matching against values, and the irrefutable-destructure path used
//! by `let` and `for` bindings.

use std::io::Write;

use crate::ks::ast::{Expr, MatchArm, Pattern, Span, Spanned};
use crate::ks::error::{ErrorKind, PatternKind, RuntimeError};
use crate::ks::types::{TypeDef, TypeId};
use crate::ks::value::Value;

use super::types_protocol::{eval, literal_matches, Flow};
use super::Interpreter;

/// A pattern with all variant names replaced by `(TypeId, variant_idx)` handles.
/// Produced by `resolve_pattern`; consumed by `match_resolved`. The matching
/// phase compares `variant_idx` integers, never strings.
#[derive(Debug, Clone)]
pub(super) enum ResolvedPattern {
    /// `_` — matches anything, binds nothing.
    Wildcard,
    /// `x` — catch-all binding. Span preserved for diagnostics.
    Binding(Spanned<String>),
    /// `42`, `"hi"`, etc. — equality check against a Value.
    /// Carries the AST literal (still a string for BigInt precision); the
    /// comparison is done by `literal_matches` at match time.
    Literal(Spanned<crate::ks::ast::Literal>),
    /// `Val(...)` or `Non()` — variant match by index, not name.
    Variant {
        variant_idx: u32,
        bindings: Vec<ResolvedPattern>,
    },
    /// `(p1, p2, ...)` — tuple match. Arity already verified at resolve time.
    Tuple(Vec<ResolvedPattern>),
}

impl Interpreter {
    pub(super) fn eval_match(
        &mut self,
        keyword_span: Span,
        subject: &Spanned<Expr>,
        arms: &[MatchArm],
        out: &mut impl Write,
    ) -> Result<Flow, RuntimeError> {
        let val = eval!(self, subject, out);

        // Phase 1: resolve every arm against the subject's type. Catches typos,
        // arity errors, and pattern/type mismatches up front — even in arms that
        // would never be reached by the first matching arm.
        let subject_ty = val.type_id();
        let resolved: Vec<ResolvedPattern> = arms
            .iter()
            .map(|arm| {
                self.check_unique_bindings(&arm.pattern)?;
                self.resolve_pattern(&arm.pattern, subject_ty)
            })
            .collect::<Result<_, RuntimeError>>()?;

        // Phase 2: handle-based structural matching against the resolved trees.
        for (rp, arm) in resolved.iter().zip(arms.iter()) {
            if let Some(bindings) = self.match_resolved(&val, rp) {
                self.push_scope();
                for (name, bound_val) in bindings {
                    self.set(name, bound_val);
                }
                let result = self.exec_block(&arm.body, out);
                self.pop_scope(out);
                return result;
            }
        }

        Err(RuntimeError::new(ErrorKind::NoMatchArm)
            .at(keyword_span)
            .label(
                subject.span,
                format!("this value: {}", val.display_with(&self.fmt_ctx())),
            )
            .help("add a wildcard arm: _ -> ..."))
    }

    /// Resolve an AST pattern against an expected TypeId. Walks the pattern
    /// once, validates against the registry (variant names, arity, type
    /// compatibility, refutability constraints), and produces a ResolvedPattern
    /// where every variant name has been replaced by its (TypeId, variant_idx)
    /// handle. Matching against the resolved tree is pure structural recursion —
    /// no string comparisons, no registry lookups in the hot path.
    fn resolve_pattern(
        &self,
        pat: &Spanned<Pattern>,
        expected_ty: TypeId,
    ) -> Result<ResolvedPattern, RuntimeError> {
        match &pat.node {
            Pattern::Wildcard => Ok(ResolvedPattern::Wildcard),
            Pattern::Literal(lit) => Ok(ResolvedPattern::Literal(lit.clone())),
            Pattern::Binding(name) => {
                // Safety net: bare-ident name colliding with a variant of an
                // enum subject is almost certainly a forgotten Name() syntax.
                if let TypeDef::EnumInstance { variants, .. } =
                    self.type_registry().get(expected_ty)
                {
                    if variants.contains_key(&name.node) {
                        return Err(RuntimeError::new(ErrorKind::PatternAmbiguousBinding {
                            binding_name: name.node.clone(),
                            type_id: expected_ty,
                        })
                        .at(name.span));
                    }
                }
                Ok(ResolvedPattern::Binding(name.clone()))
            }
            Pattern::Variant { name, bindings } => {
                let TypeDef::EnumInstance { variants, .. } = self.type_registry().get(expected_ty)
                else {
                    return Err(RuntimeError::new(ErrorKind::PatternTypeMismatch {
                        pattern_kind: PatternKind::Variant,
                        subject_type: expected_ty,
                    })
                    .at(pat.span));
                };
                let Some((variant_idx, _, vdef)) = variants.get_full(&name.node) else {
                    return Err(RuntimeError::new(ErrorKind::PatternUnknownVariant {
                        type_id: expected_ty,
                        variant_name: name.node.clone(),
                    })
                    .at(name.span));
                };
                if bindings.len() != vdef.fields.len() {
                    return Err(RuntimeError::new(ErrorKind::PatternVariantArity {
                        type_id: expected_ty,
                        variant_name: name.node.clone(),
                        expected: vdef.fields.len(),
                        actual: bindings.len(),
                    })
                    .at(pat.span));
                }
                let field_types: Vec<TypeId> = vdef.fields.clone();
                let mut resolved_bindings = Vec::with_capacity(bindings.len());
                for (sub, field_ty) in bindings.iter().zip(field_types.iter()) {
                    resolved_bindings.push(self.resolve_pattern(sub, *field_ty)?);
                }
                Ok(ResolvedPattern::Variant {
                    variant_idx: variant_idx as u32,
                    bindings: resolved_bindings,
                })
            }
            Pattern::Tuple(sub_pats) => {
                let TypeDef::TupleInstance { type_args } = self.type_registry().get(expected_ty)
                else {
                    return Err(RuntimeError::new(ErrorKind::PatternTypeMismatch {
                        pattern_kind: PatternKind::Tuple,
                        subject_type: expected_ty,
                    })
                    .at(pat.span));
                };
                if sub_pats.len() != type_args.len() {
                    return Err(RuntimeError::new(ErrorKind::PatternTupleArity {
                        expected: type_args.len(),
                        actual: sub_pats.len(),
                    })
                    .at(pat.span));
                }
                let elt_types = type_args.clone();
                let mut resolved_subs = Vec::with_capacity(sub_pats.len());
                for (sub, elt_ty) in sub_pats.iter().zip(elt_types.iter()) {
                    resolved_subs.push(self.resolve_pattern(sub, *elt_ty)?);
                }
                Ok(ResolvedPattern::Tuple(resolved_subs))
            }
        }
    }

    /// Match a resolved pattern against a value. Pure structural recursion —
    /// no string comparisons, no registry lookups. Variant matching compares
    /// `variant_idx` (u32) directly against `Value::Enum::variant_idx`.
    fn match_resolved(&self, val: &Value, pat: &ResolvedPattern) -> Option<Vec<(String, Value)>> {
        match pat {
            ResolvedPattern::Wildcard => Some(vec![]),
            ResolvedPattern::Binding(name) => Some(vec![(name.node.clone(), val.clone())]),
            ResolvedPattern::Literal(lit) => {
                if literal_matches(&lit.node, val) {
                    Some(vec![])
                } else {
                    None
                }
            }
            ResolvedPattern::Variant {
                variant_idx,
                bindings,
            } => {
                let Value::Enum {
                    variant_idx: val_idx,
                    fields,
                    ..
                } = val
                else {
                    return None;
                };
                if val_idx != variant_idx {
                    return None;
                }
                let mut bound = Vec::new();
                for (sub, field) in bindings.iter().zip(fields.iter()) {
                    bound.extend(self.match_resolved(field, sub)?);
                }
                Some(bound)
            }
            ResolvedPattern::Tuple(sub_pats) => {
                let Value::Tup { fields, .. } = val else {
                    return None;
                };
                let mut bound = Vec::new();
                for (sub, field) in sub_pats.iter().zip(fields.iter()) {
                    bound.extend(self.match_resolved(field, sub)?);
                }
                Some(bound)
            }
        }
    }

    /// Walk a pattern and ensure no binding name repeats. Pure structural —
    /// no registry lookups. Wildcards never participate.
    pub(super) fn check_unique_bindings(
        &self,
        pat: &Spanned<Pattern>,
    ) -> Result<(), RuntimeError> {
        let mut seen: Vec<(String, Span)> = Vec::new();
        Self::collect_bindings(pat, &mut seen)?;
        Ok(())
    }

    fn collect_bindings(
        pat: &Spanned<Pattern>,
        seen: &mut Vec<(String, Span)>,
    ) -> Result<(), RuntimeError> {
        match &pat.node {
            Pattern::Wildcard | Pattern::Literal(_) => Ok(()),
            Pattern::Binding(name) => {
                if let Some((_, first_span)) = seen.iter().find(|(n, _)| n == &name.node) {
                    return Err(RuntimeError::new(ErrorKind::PatternRepeatedBinding {
                        name: name.node.clone(),
                        first_span: *first_span,
                        repeat_span: name.span,
                    })
                    .at(name.span));
                }
                seen.push((name.node.clone(), name.span));
                Ok(())
            }
            Pattern::Variant { bindings, .. } => {
                for sub in bindings {
                    Self::collect_bindings(sub, seen)?;
                }
                Ok(())
            }
            Pattern::Tuple(sub_pats) => {
                for sub in sub_pats {
                    Self::collect_bindings(sub, seen)?;
                }
                Ok(())
            }
        }
    }

    /// Destructure an irrefutable pattern (Binding/Wildcard/Tuple) against a value,
    /// returning the bindings to introduce. Refutable patterns (Variant/Literal) are
    /// rejected with an error — those belong in `match` arms, not `let`/`for`.
    pub(super) fn destructure_irrefutable(
        &self,
        pat: &Spanned<Pattern>,
        val: &Value,
    ) -> Result<Vec<(String, Value)>, RuntimeError> {
        match &pat.node {
            Pattern::Binding(name) => Ok(vec![(name.node.clone(), val.clone())]),
            Pattern::Wildcard => Ok(vec![]),
            Pattern::Tuple(sub_pats) => {
                let Value::Tup { fields, .. } = val else {
                    return Err(RuntimeError::new(ErrorKind::PatternTypeMismatch {
                        pattern_kind: PatternKind::Tuple,
                        subject_type: val.type_id(),
                    })
                    .at(pat.span));
                };
                if sub_pats.len() != fields.len() {
                    return Err(RuntimeError::new(ErrorKind::PatternTupleArity {
                        expected: fields.len(),
                        actual: sub_pats.len(),
                    })
                    .at(pat.span));
                }
                let mut bound = Vec::new();
                for (sub, field) in sub_pats.iter().zip(fields.iter()) {
                    bound.extend(self.destructure_irrefutable(sub, field)?);
                }
                Ok(bound)
            }
            Pattern::Variant { name, .. } => Err(RuntimeError::new(
                ErrorKind::PatternRefutableInLet {
                    kind: PatternKind::Variant,
                },
            )
            .at(name.span)),
            Pattern::Literal(lit) => Err(RuntimeError::new(
                ErrorKind::PatternRefutableInLet {
                    kind: PatternKind::Literal,
                },
            )
            .at(lit.span)),
        }
    }
}
