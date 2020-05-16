#![allow(clippy::type_repetition_in_bounds)]

use inkwell::context::Context;
use mun_target::spec::Target;
use mun_target::abi::TargetDataLayout;
use crate::{
    ir::{file::FileIR, file_group::FileGroupIR},
    type_info::TypeInfo,
    CodeGenParams,
};
use inkwell::{
    targets::TargetData,
    types::{AnyTypeEnum, StructType},
    OptimizationLevel,
};
use std::sync::Arc;

#[derive(Debug)]
pub struct CodegenContext<D: hir::HirDatabase> {
    optimization_lvl: OptimizationLevel,
    hir_db: D,
    target: Target,
    target_data: Arc<TargetData>,
    target_data_layout: TargetDataLayout,
}

impl<D: hir::HirDatabase> CodegenContext<D> {
    pub fn new(hir_db: D) -> CodegenContext<D> {
        let target = hir_db.target();
        let target_data_layout = hir_db.target_data_layout().as_ref().clone();
        CodegenContext {
            optimization_lvl: OptimizationLevel::None,
            target_data: Arc::new(TargetData::create(&target.data_layout)),
            target,
            target_data_layout,
            hir_db,
        }
    }

    pub fn hir_db(&self) -> &D {
        &self.hir_db
    }

    pub fn hir_db_mut (&mut self) -> &mut D {
        &mut self.hir_db
    }

    pub fn optimization_lvl(&self) -> OptimizationLevel {
        self.optimization_lvl
    }

    pub fn set_optimization_lvl(&mut self, optimization_lvl: OptimizationLevel) {
        self.optimization_lvl = optimization_lvl;
    }

    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn target_data(&self) -> Arc<TargetData> {
        self.target_data.clone()
    }

    pub fn target_data_layout(&self) -> &TargetDataLayout {
        &self.target_data_layout
    }

    pub fn type_ir<'ink>(&self, context: &'ink Context, ty: hir::Ty, params: CodeGenParams) -> AnyTypeEnum<'ink> {
        crate::ir::ty::ir_query(context, self, ty, params)
    }

    pub fn struct_ty<'ink>(&self, context: &'ink Context, s: hir::Struct) -> StructType<'ink> {
        let name = s.name(self.hir_db()).to_string();
        context.opaque_struct_type(&name)
    }

    pub fn group_ir<'ink>(&self, context: &'ink Context, file: hir::FileId) -> Arc<FileGroupIR<'ink>> {
        crate::ir::file_group::ir_query(context, self, file)
    }

    pub fn file_ir<'ink>(&self, context: &'ink Context, file: hir::FileId) -> Arc<FileIR<'ink>> {
        crate::ir::file::ir_query(context, self, file)
    }

    pub fn type_info<'ink>(&self, context: &'ink Context, ty: hir::Ty) -> TypeInfo {
        crate::ir::ty::type_info_query(context, self, ty)
    }
}

// /// The `CodegenContext` enables caching of intermediate in the process of LLVM IR generation. It uses
// /// [salsa](https://github.com/salsa-rs/salsa) for this purpose.
// #[salsa::query_group(CodegenContextStorage)]
// pub trait CodegenContext: hir::HirDatabase {
//     // /// Get the LLVM context that should be used for all generation steps.
//     // #[salsa::input]
//     // fn context(&self) -> Arc<Context>;

//     /// Gets the optimization level for generation.
//     #[salsa::input]
//     fn optimization_lvl(&self) -> OptimizationLevel;

//     // /// Returns the target machine's data layout for code generation.
//     // #[salsa::invoke(crate::code_gen::target_data_query)]
//     // fn target_data(&self) -> Arc<TargetData>;

//     // /// Given a type and code generation parameters, return the corresponding IR type.
//     // #[salsa::invoke(crate::ir::ty::ir_query)]
//     // fn type_ir(&self, ty: hir::Ty, params: CodeGenParams) -> AnyTypeEnum;

//     // /// Given a struct, return the corresponding IR type.
//     // #[salsa::invoke(crate::ir::ty::struct_ty_query)]
//     // fn struct_ty(&self, s: hir::Struct) -> StructType;

//     // /// Given a `hir::FileId` generate code that is shared among the group of files.
//     // /// TODO: Currently, a group always consists of a single file. Need to add support for multiple
//     // /// files using something like `FileGroupId`.
//     // #[salsa::invoke(crate::ir::file_group::ir_query)]
//     // fn group_ir(&self, file: hir::FileId) -> Arc<FileGroupIR>;

//     // /// Given a `hir::FileId` generate code for the module.
//     // #[salsa::invoke(crate::ir::file::ir_query)]
//     // fn file_ir(&self, file: hir::FileId) -> Arc<FileIR>;

//     // /// Given a type, return the runtime `TypeInfo` that can be used to reflect the type.
//     // #[salsa::invoke(crate::ir::ty::type_info_query)]
//     // fn type_info(&self, ty: hir::Ty) -> TypeInfo;
// }
