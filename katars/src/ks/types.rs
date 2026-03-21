use std::collections::HashMap;
use std::fmt;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::error::{AccessKind, ArityTarget, ErrorKind, TypeKindExpectation};

// ── TypeId ───────────────────────────────────────────────────────────────────

/// Handle to a registered type. Cheap to copy, compare, store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeId(pub u32);

impl TypeId {
    /// Display name for primitive types without needing a registry reference.
    #[allow(dead_code)]
    pub fn display_static(self) -> &'static str {
        match self {
            prim::NIL => "Nil",
            prim::BOOL => "Bool",
            prim::INT => "Int",
            prim::FLOAT => "Float",
            prim::STR => "Str",
            prim::BIN => "Bin",
            prim::FUNC => "Func",
            prim::TYPE => "Type",
            _ => "<type>",
        }
    }
}

// ── TypeDef ──────────────────────────────────────────────────────────────────

/// A type definition in the registry.
#[derive(Debug, Clone)]
pub enum TypeDef {
    /// A primitive type with no internal structure.
    Prim { name: String },

    /// A generic enum definition: `enum Opt[T] { Some(T), None }`
    /// Not directly instantiable — must be instantiated with concrete type args.
    Enum {
        name: String,
        type_params: Vec<String>,
        variants: IndexMap<String, VariantDef>,
    },

    /// A concrete instantiation of a generic enum: `Opt[Int]`, `Res[Str, Int]`.
    /// Also used for non-generic enums (type_args is empty).
    EnumInstance {
        base: TypeId,
        type_args: Vec<TypeId>,
        variants: IndexMap<String, ResolvedVariantDef>,
    },

    /// A generic struct definition: `type Pair[A, B] { fst: A, snd: B }`
    Struct {
        name: String,
        type_params: Vec<String>,
        fields: IndexMap<String, TypeExpr>,
    },

    /// A concrete instantiation of a struct: `Pair[Int, Str]`.
    /// Also used for non-generic structs (type_args is empty).
    StructInstance {
        base: TypeId,
        type_args: Vec<TypeId>,
        fields: IndexMap<String, TypeId>,
    },
}

impl TypeDef {
    pub fn name(&self) -> &str {
        match self {
            TypeDef::Prim { name } => name,
            TypeDef::Enum { name, .. } => name,
            TypeDef::Struct { name, .. } => name,
            TypeDef::EnumInstance { .. } | TypeDef::StructInstance { .. } => "", // use display_name instead
        }
    }
}

/// A variant in a generic enum definition. Fields reference type params.
#[derive(Debug, Clone)]
pub struct VariantDef {
    pub fields: Vec<TypeExpr>,
}

/// A type expression in a definition — either a concrete type, a type param,
/// or a generic application (e.g., `Ptr[T]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TypeExpr {
    /// A resolved concrete type.
    Concrete(TypeId),
    /// A positional type parameter index — resolved at instantiation via `type_args[idx]`.
    Param(usize),
    /// A generic type application: base type with type argument expressions.
    /// e.g., `Ptr[T]` = `Generic { base: Ptr base TypeId, args: [Param(0)] }`
    Generic { base: TypeId, args: Vec<TypeExpr> },
}

/// A variant in an instantiated enum. All fields are concrete.
#[derive(Debug, Clone)]
pub struct ResolvedVariantDef {
    pub fields: Vec<TypeId>,
}

// ── TypeRegistry ─────────────────────────────────────────────────────────────

/// Central registry of all types. Handles registration, lookup, and
/// generic instantiation with caching.
#[derive(Debug)]
pub struct TypeRegistry {
    defs: Vec<TypeDef>,
    names: IndexMap<String, TypeId>,
    /// Cache for generic instantiations: (base_enum_id, type_args) → TypeId.
    instances: HashMap<(TypeId, Vec<TypeId>), TypeId>,
}

/// Well-known TypeIds for primitive types. Assigned during bootstrap.
pub mod prim {
    use super::TypeId;
    pub const NIL: TypeId = TypeId(0);
    pub const BOOL: TypeId = TypeId(1);
    pub const INT: TypeId = TypeId(2);
    pub const FLOAT: TypeId = TypeId(3);
    pub const STR: TypeId = TypeId(4);
    pub const BIN: TypeId = TypeId(5);
    pub const FUNC: TypeId = TypeId(6);
    pub const TYPE: TypeId = TypeId(7);
    pub const RAW_PTR: TypeId = TypeId(8);
}

impl TypeRegistry {
    /// Create a new registry with all primitive types pre-registered.
    pub fn new() -> Self {
        let mut reg = Self {
            defs: Vec::new(),
            names: IndexMap::new(),
            instances: HashMap::new(),
        };

        // Register prims in fixed order — must match prim::* constants.
        reg.register_prim("Nil");
        reg.register_prim("Bool");
        reg.register_prim("Int");
        reg.register_prim("Float");
        reg.register_prim("Str");
        reg.register_prim("Bin");
        reg.register_prim("Func");
        reg.register_prim("Type");
        reg.register_prim("RawPtr");

        reg
    }

    /// Allocate a new TypeId and push a definition.
    fn push_def(&mut self, def: TypeDef) -> TypeId {
        let id = TypeId(self.defs.len() as u32);
        self.defs.push(def);
        id
    }

    fn register_prim(&mut self, name: &str) -> TypeId {
        let id = self.push_def(TypeDef::Prim {
            name: name.to_string(),
        });
        self.names.insert(name.to_string(), id);
        id
    }

    /// Resolve a TypeExpr that must be concrete (non-generic context).
    fn resolve_concrete(texpr: TypeExpr, context: &str) -> TypeId {
        match texpr {
            TypeExpr::Concrete(tid) => tid,
            TypeExpr::Param(idx) => panic!("non-generic {context} has type param at index {idx}"),
            TypeExpr::Generic { .. } => panic!("non-generic {context} has generic type expr"),
        }
    }

    /// Resolve a TypeExpr using concrete type_args for parameter substitution.
    /// Handles recursive generic applications (e.g., `Ptr[T]` → `Ptr[Int]`).
    pub fn resolve_texpr(
        &mut self,
        texpr: TypeExpr,
        type_args: &[TypeId],
    ) -> Result<TypeId, ErrorKind> {
        match texpr {
            TypeExpr::Concrete(tid) => Ok(tid),
            TypeExpr::Param(idx) => Ok(type_args[idx]),
            TypeExpr::Generic { base, args } => {
                // Recursively resolve each arg, then instantiate.
                let resolved_args: Vec<TypeId> = args
                    .into_iter()
                    .map(|a| self.resolve_texpr(a, type_args))
                    .collect::<Result<_, _>>()?;
                // Instantiate based on what the base type is.
                match self.defs[base.0 as usize].clone() {
                    TypeDef::Enum { .. } => self.instantiate_enum(base, resolved_args),
                    TypeDef::Struct { .. } => self.instantiate_struct(base, resolved_args),
                    _ => Err(ErrorKind::WrongTypeKind {
                        type_id: base,
                        expected: TypeKindExpectation::GenericType,
                    }),
                }
            }
        }
    }

    /// Check cache and validate arity.
    /// Returns `Ok(true)` for cache miss (proceed with instantiation),
    /// `Ok(false)` for cache hit (caller should look up the cached instance).
    fn prepare_instantiation(
        &self,
        base_id: TypeId,
        type_args: &[TypeId],
        type_params_len: usize,
        name: &str,
    ) -> Result<bool, ErrorKind> {
        let key = (base_id, type_args.to_vec());
        if self.instances.contains_key(&key) {
            return Ok(false);
        }

        if type_args.len() != type_params_len {
            return Err(ErrorKind::ArityMismatch {
                target: ArityTarget::TypeArgs {
                    name: name.to_string(),
                },
                expected: type_params_len,
                actual: type_args.len(),
            });
        }

        Ok(true)
    }

    /// Register a generic enum definition. Returns the TypeId for the
    /// *uninstantiated* generic type.
    pub fn register_enum(
        &mut self,
        name: String,
        type_params: Vec<String>,
        variants: IndexMap<String, VariantDef>,
    ) -> TypeId {
        let id = self.push_def(TypeDef::Enum {
            name: name.clone(),
            type_params: type_params.clone(),
            variants: variants.clone(),
        });
        self.names.insert(name.clone(), id);

        // For non-generic enums, auto-instantiate immediately.
        if type_params.is_empty() {
            let resolved_variants = variants
                .into_iter()
                .map(|(vname, vdef)| {
                    let fields = vdef
                        .fields
                        .into_iter()
                        .map(|f| Self::resolve_concrete(f, "enum"))
                        .collect();
                    (vname, ResolvedVariantDef { fields })
                })
                .collect();

            let inst_id = self.push_def(TypeDef::EnumInstance {
                base: id,
                type_args: vec![],
                variants: resolved_variants,
            });
            self.instances.insert((id, vec![]), inst_id);

            // The name resolves to the instance, not the base.
            self.names.insert(name, inst_id);
            return inst_id;
        }

        id
    }

    /// Instantiate a generic enum with concrete type arguments.
    /// Returns the TypeId for the concrete instance (cached).
    pub fn instantiate_enum(
        &mut self,
        base_id: TypeId,
        type_args: Vec<TypeId>,
    ) -> Result<TypeId, ErrorKind> {
        // Look up the base enum definition.
        let base_def = self.defs[base_id.0 as usize].clone();
        let TypeDef::Enum {
            name,
            type_params,
            variants,
        } = base_def
        else {
            return Err(ErrorKind::WrongTypeKind {
                type_id: base_id,
                expected: TypeKindExpectation::GenericEnum,
            });
        };

        if !self.prepare_instantiation(base_id, &type_args, type_params.len(), &name)? {
            return Ok(*self.instances.get(&(base_id, type_args)).unwrap());
        }

        // Resolve all variant fields, substituting type params with concrete args.
        let resolved_variants = variants
            .into_iter()
            .map(|(vname, vdef)| {
                let fields = vdef
                    .fields
                    .into_iter()
                    .map(|f| self.resolve_texpr(f, &type_args))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((vname, ResolvedVariantDef { fields }))
            })
            .collect::<Result<IndexMap<_, _>, ErrorKind>>()?;

        let inst_id = self.push_def(TypeDef::EnumInstance {
            base: base_id,
            type_args: type_args.clone(),
            variants: resolved_variants,
        });
        self.instances.insert((base_id, type_args), inst_id);

        Ok(inst_id)
    }

    /// Look up a type by name.
    pub fn lookup(&self, name: &str) -> Option<TypeId> {
        self.names.get(name).copied()
    }

    /// Get a type definition by TypeId.
    pub fn get(&self, id: TypeId) -> &TypeDef {
        &self.defs[id.0 as usize]
    }

    /// Human-readable name for a type.
    pub fn display_name(&self, id: TypeId) -> String {
        match &self.defs[id.0 as usize] {
            TypeDef::Prim { name } => name.clone(),
            TypeDef::Enum { name, .. } | TypeDef::Struct { name, .. } => name.clone(),
            TypeDef::EnumInstance {
                base, type_args, ..
            }
            | TypeDef::StructInstance {
                base, type_args, ..
            } => {
                let base_name = self.display_name(*base);
                if type_args.is_empty() {
                    base_name
                } else {
                    let args: Vec<String> =
                        type_args.iter().map(|&t| self.display_name(t)).collect();
                    format!("{base_name}[{}]", args.join(", "))
                }
            }
        }
    }

    /// Get the variant index and resolved def for an instantiated enum.
    pub fn get_variant(
        &self,
        type_id: TypeId,
        variant_name: &str,
    ) -> Result<(u32, &ResolvedVariantDef), ErrorKind> {
        let def = self.get(type_id);
        let TypeDef::EnumInstance { variants, .. } = def else {
            return Err(ErrorKind::WrongTypeKind {
                type_id,
                expected: TypeKindExpectation::InstantiatedEnum,
            });
        };
        let (idx, _, vdef) = variants
            .get_full(variant_name)
            .ok_or_else(|| ErrorKind::NoAttr {
                type_id,
                attr: variant_name.to_string(),
                access: AccessKind::Variant,
            })?;
        Ok((idx as u32, vdef))
    }

    /// Register a struct type definition.
    /// Non-generic structs are auto-instantiated (like non-generic enums).
    pub fn register_struct(
        &mut self,
        name: String,
        type_params: Vec<String>,
        fields: IndexMap<String, TypeExpr>,
    ) -> TypeId {
        let id = self.push_def(TypeDef::Struct {
            name: name.clone(),
            type_params: type_params.clone(),
            fields: fields.clone(),
        });
        self.names.insert(name.clone(), id);

        // For non-generic structs, auto-instantiate immediately.
        if type_params.is_empty() {
            let resolved_fields = fields
                .into_iter()
                .map(|(fname, texpr)| (fname, Self::resolve_concrete(texpr, "struct")))
                .collect();

            let inst_id = self.push_def(TypeDef::StructInstance {
                base: id,
                type_args: vec![],
                fields: resolved_fields,
            });
            self.instances.insert((id, vec![]), inst_id);
            self.names.insert(name, inst_id);
            return inst_id;
        }

        id
    }

    /// Instantiate a generic struct with concrete type arguments.
    pub fn instantiate_struct(
        &mut self,
        base_id: TypeId,
        type_args: Vec<TypeId>,
    ) -> Result<TypeId, ErrorKind> {
        let base_def = self.defs[base_id.0 as usize].clone();
        let TypeDef::Struct {
            name,
            type_params,
            fields,
        } = base_def
        else {
            return Err(ErrorKind::WrongTypeKind {
                type_id: base_id,
                expected: TypeKindExpectation::GenericStruct,
            });
        };

        if !self.prepare_instantiation(base_id, &type_args, type_params.len(), &name)? {
            return Ok(*self.instances.get(&(base_id, type_args)).unwrap());
        }

        let resolved_fields = fields
            .into_iter()
            .map(|(fname, texpr)| {
                let tid = self.resolve_texpr(texpr, &type_args)?;
                Ok((fname, tid))
            })
            .collect::<Result<IndexMap<_, _>, ErrorKind>>()?;

        let inst_id = self.push_def(TypeDef::StructInstance {
            base: base_id,
            type_args: type_args.clone(),
            fields: resolved_fields,
        });
        self.instances.insert((base_id, type_args), inst_id);
        Ok(inst_id)
    }

    /// Get the type_args for an instance type. Returns empty slice for non-instances.
    pub fn instance_type_args(&self, id: TypeId) -> Vec<TypeId> {
        match self.get(id) {
            TypeDef::EnumInstance { type_args, .. } | TypeDef::StructInstance { type_args, .. } => {
                type_args.clone()
            }
            _ => vec![],
        }
    }

    /// Get the base TypeId for an instance type. Returns the id itself for non-instances.
    pub fn base_type(&self, id: TypeId) -> TypeId {
        match self.get(id) {
            TypeDef::EnumInstance { base, .. } | TypeDef::StructInstance { base, .. } => *base,
            _ => id,
        }
    }

    /// Get the type parameter names for a base type definition.
    /// Returns empty vec for non-generic types.
    pub fn type_param_names(&self, id: TypeId) -> Vec<String> {
        match self.get(id) {
            TypeDef::Enum { type_params, .. } | TypeDef::Struct { type_params, .. } => {
                type_params.clone()
            }
            _ => vec![],
        }
    }

    /// Get the field definitions for an instantiated struct.
    pub fn get_struct_fields(
        &self,
        type_id: TypeId,
    ) -> Result<&IndexMap<String, TypeId>, ErrorKind> {
        match self.get(type_id) {
            TypeDef::StructInstance { fields, .. } => Ok(fields),
            _ => Err(ErrorKind::WrongTypeKind {
                type_id,
                expected: TypeKindExpectation::StructType,
            }),
        }
    }

    /// Get the variant name by index for an instantiated enum.
    pub fn variant_name(&self, type_id: TypeId, variant_idx: u32) -> &str {
        let TypeDef::EnumInstance { variants, .. } = self.get(type_id) else {
            panic!("variant_name called on non-instance type");
        };
        variants
            .get_index(variant_idx as usize)
            .expect("variant index out of bounds")
            .0
            .as_str()
    }
}

impl fmt::Display for TypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypeId({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prim_types_registered() {
        let reg = TypeRegistry::new();
        assert_eq!(reg.lookup("Int"), Some(prim::INT));
        assert_eq!(reg.lookup("Str"), Some(prim::STR));
        assert_eq!(reg.lookup("Bool"), Some(prim::BOOL));
        assert_eq!(reg.display_name(prim::INT), "Int");
    }

    #[test]
    fn register_non_generic_enum() {
        let mut reg = TypeRegistry::new();
        let mut variants = IndexMap::new();
        variants.insert("Red".into(), VariantDef { fields: vec![] });
        variants.insert("Green".into(), VariantDef { fields: vec![] });

        let id = reg.register_enum("Color".into(), vec![], variants);
        assert_eq!(reg.display_name(id), "Color");

        let (idx, vdef) = reg.get_variant(id, "Red").unwrap();
        assert_eq!(idx, 0);
        assert!(vdef.fields.is_empty());
    }

    #[test]
    fn instantiate_generic_enum() {
        let mut reg = TypeRegistry::new();
        let mut variants = IndexMap::new();
        variants.insert(
            "Some".into(),
            VariantDef {
                fields: vec![TypeExpr::Param(0)],
            },
        );
        variants.insert("None".into(), VariantDef { fields: vec![] });

        let base = reg.register_enum("Opt".into(), vec!["T".into()], variants);
        let inst = reg.instantiate_enum(base, vec![prim::INT]).unwrap();

        assert_eq!(reg.display_name(inst), "Opt[Int]");

        let (idx, vdef) = reg.get_variant(inst, "Some").unwrap();
        assert_eq!(idx, 0);
        assert_eq!(vdef.fields, vec![prim::INT]);
    }

    #[test]
    fn instantiation_cached() {
        let mut reg = TypeRegistry::new();
        let mut variants = IndexMap::new();
        variants.insert(
            "Some".into(),
            VariantDef {
                fields: vec![TypeExpr::Param(0)],
            },
        );
        variants.insert("None".into(), VariantDef { fields: vec![] });

        let base = reg.register_enum("Opt".into(), vec!["T".into()], variants);
        let inst1 = reg.instantiate_enum(base, vec![prim::INT]).unwrap();
        let inst2 = reg.instantiate_enum(base, vec![prim::INT]).unwrap();
        assert_eq!(inst1, inst2);
    }

    #[test]
    fn instantiate_multi_param_enum() {
        let mut reg = TypeRegistry::new();
        let mut variants = IndexMap::new();
        variants.insert(
            "Ok".into(),
            VariantDef {
                fields: vec![TypeExpr::Param(0)],
            },
        );
        variants.insert(
            "Err".into(),
            VariantDef {
                fields: vec![TypeExpr::Param(1)],
            },
        );

        let base = reg.register_enum("Res".into(), vec!["T".into(), "E".into()], variants);
        let inst = reg
            .instantiate_enum(base, vec![prim::INT, prim::STR])
            .unwrap();

        assert_eq!(reg.display_name(inst), "Res[Int, Str]");

        let (_, ok_def) = reg.get_variant(inst, "Ok").unwrap();
        assert_eq!(ok_def.fields, vec![prim::INT]);

        let (_, err_def) = reg.get_variant(inst, "Err").unwrap();
        assert_eq!(err_def.fields, vec![prim::STR]);
    }

    #[test]
    fn multi_param_enum_order_matters() {
        let mut reg = TypeRegistry::new();
        let mut variants = IndexMap::new();
        variants.insert(
            "Ok".into(),
            VariantDef {
                fields: vec![TypeExpr::Param(0)],
            },
        );
        variants.insert(
            "Err".into(),
            VariantDef {
                fields: vec![TypeExpr::Param(1)],
            },
        );

        let base = reg.register_enum("Res".into(), vec!["T".into(), "E".into()], variants);

        let int_str = reg
            .instantiate_enum(base, vec![prim::INT, prim::STR])
            .unwrap();
        let str_int = reg
            .instantiate_enum(base, vec![prim::STR, prim::INT])
            .unwrap();

        // Different type arg order → different instances.
        assert_ne!(int_str, str_int);

        // Ok field follows param index 0.
        let (_, ok_a) = reg.get_variant(int_str, "Ok").unwrap();
        let (_, ok_b) = reg.get_variant(str_int, "Ok").unwrap();
        assert_eq!(ok_a.fields, vec![prim::INT]);
        assert_eq!(ok_b.fields, vec![prim::STR]);
    }

    #[test]
    fn instantiate_generic_struct() {
        let mut reg = TypeRegistry::new();
        let mut fields = IndexMap::new();
        fields.insert("val".into(), TypeExpr::Param(0));

        let base = reg.register_struct("Box".into(), vec!["T".into()], fields);
        let inst = reg.instantiate_struct(base, vec![prim::INT]).unwrap();

        assert_eq!(reg.display_name(inst), "Box[Int]");
        let resolved = reg.get_struct_fields(inst).unwrap();
        assert_eq!(resolved.get("val"), Some(&prim::INT));
    }

    #[test]
    fn instantiate_multi_param_struct() {
        let mut reg = TypeRegistry::new();
        let mut fields = IndexMap::new();
        fields.insert("fst".into(), TypeExpr::Param(0));
        fields.insert("snd".into(), TypeExpr::Param(1));

        let base = reg.register_struct("Pair".into(), vec!["A".into(), "B".into()], fields);
        let inst = reg
            .instantiate_struct(base, vec![prim::INT, prim::STR])
            .unwrap();

        assert_eq!(reg.display_name(inst), "Pair[Int, Str]");
        let resolved = reg.get_struct_fields(inst).unwrap();
        assert_eq!(resolved.get("fst"), Some(&prim::INT));
        assert_eq!(resolved.get("snd"), Some(&prim::STR));
    }

    #[test]
    fn type_args_arity_mismatch() {
        let mut reg = TypeRegistry::new();
        let mut variants = IndexMap::new();
        variants.insert(
            "Some".into(),
            VariantDef {
                fields: vec![TypeExpr::Param(0)],
            },
        );

        let base = reg.register_enum("Opt".into(), vec!["T".into()], variants);
        let err = reg
            .instantiate_enum(base, vec![prim::INT, prim::STR])
            .unwrap_err();

        assert!(matches!(
            err,
            ErrorKind::ArityMismatch {
                expected: 1,
                actual: 2,
                ..
            }
        ));
    }
}
