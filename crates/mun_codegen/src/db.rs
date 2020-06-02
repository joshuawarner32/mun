#![allow(clippy::type_repetition_in_bounds)]

use inkwell::{
    targets::TargetData,
    OptimizationLevel,
};
use std::sync::Arc;

/// The `IrDatabase` enables caching of intermediate in the process of LLVM IR generation. It uses
/// [salsa](https://github.com/salsa-rs/salsa) for this purpose.
#[salsa::query_group(IrDatabaseStorage)]
pub trait IrDatabase: hir::HirDatabase {
    /// Gets the optimization level for generation.
    #[salsa::input]
    fn optimization_lvl(&self) -> OptimizationLevel;

    /// Returns the target machine's data layout for code generation.
    #[salsa::invoke(crate::code_gen::target_data_query)]
    fn target_data(&self) -> Arc<TargetData>;

    // #[salsa::invoke(crate::ir::ty::ir_query)]
    // fn type_ir(&self, ty: hir::Ty, params: CodeGenParams) -> AnyTypeEnum;

    // #[salsa::invoke(crate::ir::ty::struct_ty_query)]
    // fn struct_ty(&self, s: hir::Struct) -> StructType;

    // #[salsa::invoke(crate::ir::file_group::ir_query)]
    // fn group_ir(&self, file: hir::FileId) -> Arc<FileGroupIR>;

    // #[salsa::invoke(crate::ir::file::ir_query)]
    // fn file_ir(&self, file: hir::FileId) -> Arc<FileIR>;

    // #[salsa::invoke(crate::ir::ty::type_info_query)]
    // fn type_info(&self, ty: hir::Ty) -> TypeInfo;
}
