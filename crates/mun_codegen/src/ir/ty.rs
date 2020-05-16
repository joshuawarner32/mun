use super::try_convert_any_to_basic;
use crate::{
    type_info::{TypeInfo, TypeSize},
    CodeGenParams, CodegenContext,
};
use hir::{
    ApplicationTy, CallableDef, FloatBitness, FloatTy, IntBitness, IntTy, ResolveBitness, Ty,
    TypeCtor,
};
use inkwell::{
    types::{AnyTypeEnum, BasicType, BasicTypeEnum, FloatType, IntType, StructType},
    AddressSpace,
};

/// Given a mun type, construct an LLVM IR type
#[rustfmt::skip]
pub(crate) fn ir_query<'ink, D: hir::HirDatabase>(db: &'ink CodegenContext<D>, ty: Ty, params: CodeGenParams) -> AnyTypeEnum<'ink> {
    match ty {
        Ty::Empty => AnyTypeEnum::StructType(db.context.struct_type(&[], false)),
        Ty::Apply(ApplicationTy { ctor, .. }) => match ctor {
            TypeCtor::Float(fty) => float_ty_query(db, fty).into(),
            TypeCtor::Int(ity) => int_ty_query(db, ity).into(),
            TypeCtor::Bool => AnyTypeEnum::IntType(db.context.bool_type()),

            TypeCtor::FnDef(def @ CallableDef::Function(_)) => {
                let ty = db.hir_db().callable_sig(def);
                let param_tys: Vec<BasicTypeEnum> = ty
                    .params()
                    .iter()
                    .map(|p| {
                        try_convert_any_to_basic(db.type_ir(p.clone(), params.clone())).unwrap()
                    })
                    .collect();

                let fn_type = match ty.ret() {
                    Ty::Empty => db.context.void_type().fn_type(&param_tys, false),
                    ty => try_convert_any_to_basic(db.type_ir(ty.clone(), params))
                        .expect("could not convert return value")
                        .fn_type(&param_tys, false),
                };

                AnyTypeEnum::FunctionType(fn_type)
            }
            TypeCtor::Struct(s) => {
                let struct_ty = db.struct_ty(s);
                match s.data(db.hir_db()).memory_kind {
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

/// Returns the LLVM IR type of the specified float type
fn float_ty_query<'ink, D: hir::HirDatabase>(db: &'ink CodegenContext<D>, fty: FloatTy) -> FloatType<'ink> {
    match fty.bitness.resolve(&db.hir_db().target_data_layout()) {
        FloatBitness::X64 => db.context.f64_type(),
        FloatBitness::X32 => db.context.f32_type(),
    }
}

/// Returns the LLVM IR type of the specified int type
fn int_ty_query<'ink, D: hir::HirDatabase>(db: &'ink CodegenContext<D>, ity: IntTy) -> IntType<'ink> {
    match ity.bitness.resolve(&db.hir_db().target_data_layout()) {
        IntBitness::X128 => db.context.i128_type(),
        IntBitness::X64 => db.context.i64_type(),
        IntBitness::X32 => db.context.i32_type(),
        IntBitness::X16 => db.context.i16_type(),
        IntBitness::X8 => db.context.i8_type(),
        _ => unreachable!(),
    }
}

/// Constructs the `TypeInfo` for the specified HIR type
pub fn type_info_query<'ink, D: hir::HirDatabase>(db: &'ink CodegenContext<D>, ty: Ty) -> TypeInfo {
    let target = db.target_data();
    match ty {
        Ty::Apply(ctor) => match ctor.ctor {
            TypeCtor::Float(ty) => {
                let ir_ty = float_ty_query(db, ty);
                let type_size = TypeSize::from_ir_type(&ir_ty, &target);
                TypeInfo::new_fundamental(
                    format!("core::{}", ty.resolve(&db.target_data_layout())),
                    type_size,
                )
            }
            TypeCtor::Int(ty) => {
                let ir_ty = int_ty_query(db, ty);
                let type_size = TypeSize::from_ir_type(&ir_ty, &target);
                TypeInfo::new_fundamental(
                    format!("core::{}", ty.resolve(&db.target_data_layout())),
                    type_size,
                )
            }
            TypeCtor::Bool => {
                let ir_ty = db.context.bool_type();
                let type_size = TypeSize::from_ir_type(&ir_ty, &target);
                TypeInfo::new_fundamental("core::bool", type_size)
            }
            TypeCtor::Struct(s) => {
                let ir_ty = db.struct_ty(s);
                let type_size = TypeSize::from_ir_type(&ir_ty, &target);
                TypeInfo::new_struct(db, s, type_size)
            }
            _ => unreachable!("{:?} unhandled", ctor),
        },
        _ => unreachable!("{:?} unhandled", ty),
    }
}
