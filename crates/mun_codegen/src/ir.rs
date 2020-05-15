use crate::type_info::TypeInfo;
use inkwell::context::Context;
use inkwell::targets::TargetData;
use inkwell::types::{
    AnyType, AnyTypeEnum, BasicType, BasicTypeEnum, FloatType, FunctionType, IntType, PointerType,
};
use inkwell::AddressSpace;

pub(crate) mod abi_types;
pub mod adt;
pub mod body;
#[macro_use]
pub(crate) mod dispatch_table;
pub mod file;
pub(crate) mod file_group;
pub mod function;
mod intrinsics;
pub mod ty;
pub(crate) mod type_table;

/// Try to down cast an `AnyTypeEnum` into a `BasicTypeEnum`.
fn try_convert_any_to_basic(ty: AnyTypeEnum) -> Option<BasicTypeEnum> {
    match ty {
        AnyTypeEnum::ArrayType(t) => Some(t.into()),
        AnyTypeEnum::FloatType(t) => Some(t.into()),
        AnyTypeEnum::IntType(t) => Some(t.into()),
        AnyTypeEnum::PointerType(t) => Some(t.into()),
        AnyTypeEnum::StructType(t) => Some(t.into()),
        AnyTypeEnum::VectorType(t) => Some(t.into()),
        _ => None,
    }
}

/// Defines that a type has a static representation in inkwell
pub trait IsIrType<'ink> {
    type Type: AnyType<'ink>;

    fn ir_type(context: &'ink Context, target: &TargetData) -> Self::Type;
}

/// Defines that a type has a static represention in inkwell that can be described as a BasicType.
pub trait IsBasicIrType<'ink> {
    fn ir_type(context: &'ink Context, target: &TargetData) -> BasicTypeEnum<'ink>;
}

impl<'ink, T: IsIrType<'ink>> IsBasicIrType<'ink> for T
    where T::Type: BasicType<'ink>
{
    fn ir_type(context: &'ink Context, target: &TargetData) -> BasicTypeEnum<'ink> {
        Self::ir_type(context, target).as_basic_type_enum()
    }
}

/// Defines that a type can statically be used as a return type for a function
pub trait IsFunctionReturnType<'ink> {
    fn fn_type(
        context: &'ink Context,
        target: &TargetData,
        arg_types: &[BasicTypeEnum<'ink>],
        is_var_args: bool,
    ) -> FunctionType<'ink>;
}

/// All types that statically have a BasicTypeEnum can also be used as a function return type
impl<'ink, T: IsBasicIrType<'ink>+'ink> IsFunctionReturnType<'ink> for T {
    fn fn_type(
        context: &'ink Context,
        target: &TargetData,
        arg_types: &[BasicTypeEnum<'ink>],
        is_var_args: bool,
    ) -> FunctionType<'ink> {
        T::ir_type(context, target).fn_type(arg_types, is_var_args)
    }
}

impl<'ink> IsFunctionReturnType<'ink> for () {
    fn fn_type(
        context: &'ink Context,
        _target: &TargetData,
        arg_types: &[BasicTypeEnum<'ink>],
        is_var_args: bool,
    ) -> FunctionType<'ink> {
        context.void_type().fn_type(arg_types, is_var_args)
    }
}

/// Defines that a value can be converted to an inkwell type
pub trait AsIrType<'ink> {
    type Type: AnyType<'ink>;

    fn as_ir_type(&self, context: &'ink Context, target: &TargetData) -> Self::Type;
}

pub trait AsBasicIrType<'ink> {
    fn as_ir_type(&self, context: &'ink Context, target: &TargetData) -> BasicTypeEnum<'ink>;
}

impl<'ink, T: AsIrType<'ink>> AsBasicIrType<'ink> for T
    where T::Type: BasicType<'ink>
{
    fn as_ir_type(&self, context: &'ink Context, target: &TargetData) -> BasicTypeEnum<'ink> {
        self.as_ir_type(context, target).as_basic_type_enum()
    }
}

/// Defines that a value can be used to construct a function type.
pub trait AsFunctionReturnType<'ink> {
    fn as_fn_type(
        &self,
        context: &'ink Context,
        target: &TargetData,
        arg_types: &[BasicTypeEnum<'ink>],
        is_var_args: bool,
    ) -> FunctionType<'ink>;
}

impl<'ink, T: AsBasicIrType<'ink>> AsFunctionReturnType<'ink> for T {
    fn as_fn_type(
        &self,
        context: &'ink Context,
        target: &TargetData,
        arg_types: &[BasicTypeEnum<'ink>],
        is_var_args: bool,
    ) -> FunctionType<'ink> {
        self.as_ir_type(context, target)
            .fn_type(arg_types, is_var_args)
    }
}

impl<'ink> AsFunctionReturnType<'ink> for () {
    fn as_fn_type(
        &self,
        context: &'ink Context,
        _target: &TargetData,
        arg_types: &[BasicTypeEnum<'ink>],
        is_var_args: bool,
    ) -> FunctionType<'ink> {
        context.void_type().fn_type(arg_types, is_var_args)
    }
}

macro_rules! impl_fundamental_ir_types {
    ($(
        $ty:ty => $context_fun:ident():$inkwell_ty:ty
    ),+) => {
        $(
            impl<'ink> IsIrType<'ink> for $ty {
                type Type = $inkwell_ty;

                fn ir_type(context: &'ink Context, _target: &TargetData) -> Self::Type {
                    context.$context_fun()
                }
            }
        )+
    }
}

impl_fundamental_ir_types!(
    i8 => i8_type():IntType<'ink>,
    i16 => i16_type():IntType<'ink>,
    i32 => i32_type():IntType<'ink>,
    i64 => i64_type():IntType<'ink>,
    i128 => i128_type():IntType<'ink>,

    u8 => i8_type():IntType<'ink>,
    u16 => i16_type():IntType<'ink>,
    u32 => i32_type():IntType<'ink>,
    u64 => i64_type():IntType<'ink>,
    u128 => i128_type():IntType<'ink>,

    bool => bool_type():IntType<'ink>,

    f32 => f32_type():FloatType<'ink>,
    f64 => f64_type():FloatType<'ink>
);

impl<'ink> IsIrType<'ink> for usize {
    type Type = IntType<'ink>;

    fn ir_type(context: &'ink Context, target: &TargetData) -> Self::Type {
        match target.get_pointer_byte_size(None) {
            4 => <u32 as IsIrType>::ir_type(context, target),
            8 => <u64 as IsIrType>::ir_type(context, target),
            _ => unimplemented!("unsupported pointer byte size"),
        }
    }
}

impl<'ink> IsIrType<'ink> for isize {
    type Type = IntType<'ink>;

    fn ir_type(context: &'ink Context, target: &TargetData) -> Self::Type {
        match target.get_pointer_byte_size(None) {
            4 => <i32 as IsIrType>::ir_type(context, target),
            8 => <i64 as IsIrType>::ir_type(context, target),
            _ => unimplemented!("unsupported pointer byte size"),
        }
    }
}

pub trait IsPointerType<'ink> {
    fn ir_type(context: &'ink Context, target: &TargetData) -> PointerType<'ink>;
}

impl<'ink, S: BasicType<'ink>, T: IsIrType<'ink, Type = S>> IsPointerType<'ink> for *const T {
    fn ir_type(context: &'ink Context, target: &TargetData) -> PointerType<'ink> {
        T::ir_type(context, target).ptr_type(AddressSpace::Const)
    }
}

// HACK: Manually add `*const TypeInfo`
impl<'ink> IsPointerType<'ink> for *const TypeInfo {
    fn ir_type(context: &'ink Context, _target: &TargetData) -> PointerType<'ink> {
        context.i8_type().ptr_type(AddressSpace::Const)
    }
}

// HACK: Manually add `*const c_void`
impl<'ink> IsPointerType<'ink> for *const std::ffi::c_void {
    fn ir_type(context: &'ink Context, _target: &TargetData) -> PointerType<'ink> {
        context.i8_type().ptr_type(AddressSpace::Const)
    }
}

// HACK: Manually add `*mut c_void`
impl<'ink> IsPointerType<'ink> for *mut std::ffi::c_void {
    fn ir_type(context: &'ink Context, _target: &TargetData) -> PointerType<'ink> {
        context.i8_type().ptr_type(AddressSpace::Generic)
    }
}

impl<'ink, S: BasicType<'ink>, T: IsIrType<'ink, Type = S>> IsPointerType<'ink> for *mut T {
    fn ir_type(context: &'ink Context, target: &TargetData) -> PointerType<'ink> {
        T::ir_type(context, target).ptr_type(AddressSpace::Generic)
    }
}

impl<'ink, T: IsPointerType<'ink>> IsIrType<'ink> for T {
    type Type = PointerType<'ink>;

    fn ir_type(context: &'ink Context, target: &TargetData) -> Self::Type {
        T::ir_type(context, target)
    }
}
