use crate::code_gen::CodeGenConfig;
use std::collections::HashMap;
use mun_target::abi::TargetDataLayout;
use inkwell::context::Context;
use super::try_convert_any_to_basic;
use crate::{
    type_info::{TypeInfo, TypeSize},
    CodeGenParams,
};
use hir::{
    ApplicationTy, CallableDef, FloatBitness, FloatTy, IntBitness, IntTy, ResolveBitness, Ty,
    TypeCtor,
};
use inkwell::{
    types::{AnyTypeEnum, BasicType, BasicTypeEnum, FloatType, IntType, StructType},
    AddressSpace,
};
use std::collections::hash_map::Entry;

#[derive(Debug)]
pub struct TypeManager {
    structs: HashMap<hir::Struct, StructCacheState>,
    infos: HashMap<hir::Ty, TypeInfo>,
}

#[derive(Debug)]
struct StructCacheState {
    fields: Vec<(hir::StructField, AnyTypeEnum)>,
    ty: StructType,
}

impl TypeManager {
    pub fn new() -> TypeManager {
        TypeManager {
            structs: HashMap::new(),
            infos: HashMap::new(),
        }
    }

    /// Given a mun type, construct an LLVM IR type
    pub fn type_ir<D: hir::HirDatabase>(&mut self, context: &Context, db: &D, ty: hir::Ty, params: CodeGenParams) -> AnyTypeEnum {
        let layout = db.target_data_layout();
        match ty {
            Ty::Empty => AnyTypeEnum::StructType(context.struct_type(&[], false)),
            Ty::Apply(ApplicationTy { ctor, .. }) => match ctor {
                TypeCtor::Float(fty) => float_ty_query(context, &layout, fty).into(),
                TypeCtor::Int(ity) => int_ty_query(context, &layout, ity).into(),
                TypeCtor::Bool => AnyTypeEnum::IntType(context.bool_type()),

                TypeCtor::FnDef(def @ CallableDef::Function(_)) => {
                    let ty = db.callable_sig(def);
                    let param_tys: Vec<BasicTypeEnum> = ty
                        .params()
                        .iter()
                        .map(|p| {
                            try_convert_any_to_basic(self.type_ir(context, db, p.clone(), params.clone())).unwrap()
                        })
                        .collect();

                    let fn_type = match ty.ret() {
                        Ty::Empty => context.void_type().fn_type(&param_tys, false),
                        ty => try_convert_any_to_basic(self.type_ir(context, db, ty.clone(), params))
                            .expect("could not convert return value")
                            .fn_type(&param_tys, false),
                    };

                    AnyTypeEnum::FunctionType(fn_type)
                }
                TypeCtor::Struct(s) => {
                    let struct_ty = self.struct_ty(context, db, s);
                    match s.data(db).memory_kind {
                        hir::StructMemoryKind::GC => struct_ty.ptr_type(AddressSpace::Generic).ptr_type(AddressSpace::Const).into(),
                        hir::StructMemoryKind::Value if params.make_marshallable =>
                                struct_ty.ptr_type(AddressSpace::Generic).ptr_type(AddressSpace::Const).into(),
                        hir::StructMemoryKind::Value => struct_ty.into(),
                    }
                }
                _ => unreachable!(),
            },
            _ => unreachable!("unknown type can not be converted"),
        }
    }

    pub fn struct_ty<D: hir::HirDatabase>(&mut self, context: &Context, db: &D, s: hir::Struct) -> StructType {
        let name = s.name(db).to_string();
        let fields = s.fields(db).into_iter()
            .map(|field| {
                let field_type = field.ty(db);
                let field_type_ir = self.type_ir(
                    context,
                    db,
                    field_type,
                    CodeGenParams {
                        make_marshallable: false,
                    },
                );
                (field, field_type_ir)
            })
            .collect::<Vec<_>>();

        match self.structs.entry(s) {
            Entry::Occupied(mut entry) => {
                let entry = entry.get_mut();
                if entry.fields == fields {
                    entry.ty
                } else {
                    let ty = context.opaque_struct_type(&name);
                    entry.ty = ty;
                    entry.fields = fields;
                    ty
                }
            }
            Entry::Vacant(entry) => {
                let ty = context.opaque_struct_type(&name);
                entry.insert(StructCacheState {
                    ty,
                    fields
                });
                ty
            }
        }
    }

    pub fn type_info<D: hir::HirDatabase>(&mut self, context: &Context, config: &CodeGenConfig, db: &D, ty: hir::Ty) -> TypeInfo {
        if let Some(info) = self.infos.get(&ty) {
            return info.clone();
        }

        let target = config.target_data.as_ref();
        let layout = db.target_data_layout();
 
        let res = match &ty {
            Ty::Apply(ctor) => match ctor.ctor {
                TypeCtor::Float(ty) => {
                    let ir_ty = float_ty_query(&context, &layout, ty);
                    let type_size = TypeSize::from_ir_type(&ir_ty, target);
                    TypeInfo::new_fundamental(
                        format!("core::{}", ty.resolve(&layout)),
                        type_size,
                    )
                }
                TypeCtor::Int(ty) => {
                    let ir_ty = int_ty_query(&context, &layout, ty);
                    let type_size = TypeSize::from_ir_type(&ir_ty, target);
                    TypeInfo::new_fundamental(
                        format!("core::{}", ty.resolve(&layout)),
                        type_size,
                    )
                }
                TypeCtor::Bool => {
                    let ir_ty = context.bool_type();
                    let type_size = TypeSize::from_ir_type(&ir_ty, target);
                    TypeInfo::new_fundamental("core::bool", type_size)
                }
                TypeCtor::Struct(s) => {
                    let ir_ty = self.struct_ty(context, db, s);
                    let type_size = TypeSize::from_ir_type(&ir_ty, target);
                    return TypeInfo::new_struct(db, s, type_size)
                }
                _ => unreachable!("{:?} unhandled", ctor),
            },
            _ => unreachable!("{:?} unhandled", ty),
        };

        assert!(self.infos.insert(ty, res.clone()).is_none());
        res
    }
}

/// Returns the LLVM IR type of the specified float type
fn float_ty_query(context: &Context, layout: &TargetDataLayout, fty: FloatTy) -> FloatType {
    match fty.bitness.resolve(layout) {
        FloatBitness::X64 => context.f64_type(),
        FloatBitness::X32 => context.f32_type(),
    }
}

/// Returns the LLVM IR type of the specified int type
fn int_ty_query(context: &Context, layout: &TargetDataLayout, ity: IntTy) -> IntType {
    match ity.bitness.resolve(layout) {
        IntBitness::X128 => context.i128_type(),
        IntBitness::X64 => context.i64_type(),
        IntBitness::X32 => context.i32_type(),
        IntBitness::X16 => context.i16_type(),
        IntBitness::X8 => context.i8_type(),
        _ => unreachable!(),
    }
}
