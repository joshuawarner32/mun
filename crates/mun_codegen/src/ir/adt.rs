//use crate::ir::module::Types;
use inkwell::context::Context;
use crate::ir::try_convert_any_to_basic;
use crate::{CodeGenParams, CodegenContext};
use inkwell::types::{BasicTypeEnum, StructType};

pub(super) fn gen_struct_decl<'ink, D: hir::HirDatabase>(context: &'ink Context, db: &CodegenContext<D>, s: hir::Struct) -> StructType<'ink> {
    let struct_type = db.struct_ty(context, s);
    if struct_type.is_opaque() {
        let field_types: Vec<BasicTypeEnum> = s
            .fields(db.hir_db())
            .iter()
            .map(|field| {
                let field_type = field.ty(db.hir_db());
                try_convert_any_to_basic(db.type_ir(
                    context,
                    field_type,
                    CodeGenParams {
                        make_marshallable: false,
                    },
                ))
                .expect("could not convert field type")
            })
            .collect();

        struct_type.set_body(&field_types, false);
    }
    struct_type
}
