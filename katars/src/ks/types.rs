use std::collections::HashMap;
use std::fmt;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

// ── TypeId ───────────────────────────────────────────────────────────────────

/// Handle to a registered type. Cheap to copy, compare, store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeId(pub u32);

impl TypeId {
    /// Display name for primitive types without needing a registry reference.
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
}

impl TypeDef {
    pub fn name(&self) -> &str {
        match self {
            TypeDef::Prim { name } => name,
            TypeDef::Enum { name, .. } => name,
            TypeDef::EnumInstance { .. } => "", // use display_name instead
        }
    }
}

/// A variant in a generic enum definition. Fields reference type params.
#[derive(Debug, Clone)]
pub struct VariantDef {
    pub fields: Vec<TypeExpr>,
}

/// A type expression in a definition — either a concrete type or a type param.
#[derive(Debug, Clone)]
pub enum TypeExpr {
    /// A resolved concrete type.
    Concrete(TypeId),
    /// A type parameter name (e.g., "T") — resolved at instantiation time.
    Param(String),
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

        reg
    }

    fn register_prim(&mut self, name: &str) -> TypeId {
        let id = TypeId(self.defs.len() as u32);
        self.defs.push(TypeDef::Prim {
            name: name.to_string(),
        });
        self.names.insert(name.to_string(), id);
        id
    }

    /// Register a generic enum definition. Returns the TypeId for the
    /// *uninstantiated* generic type.
    pub fn register_enum(
        &mut self,
        name: String,
        type_params: Vec<String>,
        variants: IndexMap<String, VariantDef>,
    ) -> TypeId {
        let id = TypeId(self.defs.len() as u32);
        // For non-generic enums, also create an instance immediately.
        if type_params.is_empty() {
            // Register the base definition.
            self.defs.push(TypeDef::Enum {
                name: name.clone(),
                type_params: vec![],
                variants: variants.clone(),
            });
            self.names.insert(name.clone(), id);

            // Auto-instantiate (no type args to resolve).
            let resolved_variants = variants
                .into_iter()
                .map(|(vname, vdef)| {
                    let fields = vdef
                        .fields
                        .into_iter()
                        .map(|f| match f {
                            TypeExpr::Concrete(tid) => tid,
                            TypeExpr::Param(p) => {
                                panic!("non-generic enum has type param {p}")
                            }
                        })
                        .collect();
                    (vname, ResolvedVariantDef { fields })
                })
                .collect();

            let inst_id = TypeId(self.defs.len() as u32);
            self.defs.push(TypeDef::EnumInstance {
                base: id,
                type_args: vec![],
                variants: resolved_variants,
            });
            self.instances.insert((id, vec![]), inst_id);

            // The name resolves to the instance, not the base.
            self.names.insert(name, inst_id);
            return inst_id;
        }

        self.defs.push(TypeDef::Enum {
            name: name.clone(),
            type_params,
            variants,
        });
        self.names.insert(name, id);
        id
    }

    /// Instantiate a generic enum with concrete type arguments.
    /// Returns the TypeId for the concrete instance (cached).
    pub fn instantiate_enum(
        &mut self,
        base_id: TypeId,
        type_args: Vec<TypeId>,
    ) -> Result<TypeId, String> {
        // Check cache.
        let key = (base_id, type_args.clone());
        if let Some(&cached) = self.instances.get(&key) {
            return Ok(cached);
        }

        // Look up the base enum definition.
        let base_def = self.defs[base_id.0 as usize].clone();
        let TypeDef::Enum {
            name,
            type_params,
            variants,
        } = base_def
        else {
            return Err(format!(
                "'{}' is not a generic enum",
                self.display_name(base_id)
            ));
        };

        if type_args.len() != type_params.len() {
            return Err(format!(
                "'{name}' expects {} type argument(s), got {}",
                type_params.len(),
                type_args.len()
            ));
        }

        // Build param → TypeId mapping.
        let param_map: HashMap<&str, TypeId> = type_params
            .iter()
            .zip(type_args.iter())
            .map(|(p, &t)| (p.as_str(), t))
            .collect();

        // Resolve all variant fields.
        let resolved_variants = variants
            .into_iter()
            .map(|(vname, vdef)| {
                let fields = vdef
                    .fields
                    .into_iter()
                    .map(|f| match f {
                        TypeExpr::Concrete(tid) => Ok(tid),
                        TypeExpr::Param(ref p) => {
                            param_map.get(p.as_str()).copied().ok_or_else(|| {
                                format!("unknown type parameter '{p}' in variant '{vname}'")
                            })
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((vname, ResolvedVariantDef { fields }))
            })
            .collect::<Result<IndexMap<_, _>, String>>()?;

        let inst_id = TypeId(self.defs.len() as u32);
        self.defs.push(TypeDef::EnumInstance {
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
            TypeDef::Enum { name, .. } => name.clone(),
            TypeDef::EnumInstance {
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
    ) -> Result<(u32, &ResolvedVariantDef), String> {
        let def = self.get(type_id);
        let TypeDef::EnumInstance { variants, .. } = def else {
            return Err(format!(
                "'{}' is not an instantiated enum type",
                self.display_name(type_id)
            ));
        };
        let (idx, _, vdef) = variants.get_full(variant_name).ok_or_else(|| {
            format!(
                "'{}' has no variant '{variant_name}'",
                self.display_name(type_id)
            )
        })?;
        Ok((idx as u32, vdef))
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
                fields: vec![TypeExpr::Param("T".into())],
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
                fields: vec![TypeExpr::Param("T".into())],
            },
        );
        variants.insert("None".into(), VariantDef { fields: vec![] });

        let base = reg.register_enum("Opt".into(), vec!["T".into()], variants);
        let inst1 = reg.instantiate_enum(base, vec![prim::INT]).unwrap();
        let inst2 = reg.instantiate_enum(base, vec![prim::INT]).unwrap();
        assert_eq!(inst1, inst2);
    }
}
