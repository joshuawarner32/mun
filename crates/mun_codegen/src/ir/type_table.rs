use inkwell::context::Context;
use crate::code_gen::{
    gen_global, gen_string_array, gen_struct_ptr_array, gen_u16_array, intern_string,
};
use crate::ir::{
    abi_types::AbiTypes,
    dispatch_table::{DispatchTable, FunctionPrototype},
};
use crate::type_info::{TypeGroup, TypeInfo};
use crate::CodegenContext;
use hir::{Body, ExprId, InferenceResult};
use inkwell::{
    module::Module,
    targets::TargetData,
    types::ArrayType,
    values::{GlobalValue, IntValue, PointerValue, StructValue},
    AddressSpace,
};
use std::collections::{BTreeSet, HashMap};
use std::{convert::TryInto, mem, sync::Arc};

/// A type table in IR is a list of pointers to unique type information that are used to generate
/// function and struct information.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeTable<'ink> {
    type_info_to_index: HashMap<TypeInfo, usize>,
    entries: Vec<PointerValue<'ink>>,
    table_type: ArrayType<'ink>,
}

impl<'ink> TypeTable<'ink> {
    /// The name of the TypeTable's LLVM `GlobalValue`.
    pub(crate) const NAME: &'static str = "global_type_table";

    /// Generates a `TypeInfo` lookup through the `TypeTable`, equivalent to something along the
    /// lines of: `type_table[i]`, where `i` is the index of the type and `type_table` is an array
    /// of `TypeInfo` pointers.
    pub fn gen_type_info_lookup(
        &self,
        builder: &inkwell::builder::Builder<'ink>,
        type_info: &TypeInfo,
        table_ref: Option<GlobalValue<'ink>>,
    ) -> PointerValue<'ink> {
        let table_ref = table_ref.expect("no type table defined");

        let index = *self
            .type_info_to_index
            .get(type_info)
            .expect("unknown type");

        let ptr_to_type_info_ptr = {
            builder.build_struct_gep(
                table_ref.as_pointer_value(),
                index as u32,
                &format!("{}_ptr_ptr", type_info.name),
            )
        }.unwrap();
        builder
            .build_load(ptr_to_type_info_ptr, &format!("{}_ptr", type_info.name))
            .into_pointer_value()
    }

    /// Retrieves the global `TypeInfo` IR value corresponding to `type_info`, if it exists.
    pub fn get(module: &Module<'ink>, type_info: &TypeInfo) -> Option<GlobalValue<'ink>> {
        module.get_global(&type_info_global_name(type_info))
    }

    /// Returns the number of types in the `TypeTable`.
    pub fn num_types(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the type table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the IR type of the type table's global value, if it exists.
    pub fn ty(&self) -> ArrayType<'ink> {
        self.table_type
    }
}

/// Used to build a `TypeTable` from HIR.
pub(crate) struct TypeTableBuilder<'ink, 'a> {
    context: &'ink Context,
    target_data: Arc<TargetData>,
    module: &'a Module<'ink>,
    abi_types: &'a AbiTypes<'ink>,
    dispatch_table: &'a DispatchTable<'ink>,
    entries: BTreeSet<TypeInfo>, // Use a `BTreeSet` to guarantee deterministically ordered output
}

impl<'a , 'ink: 'a> TypeTableBuilder<'ink, 'a> {
    /// Creates a new `TypeTableBuilder`.
    pub(crate) fn new<'f, D: hir::HirDatabase>(
        context: &'ink Context,
        db: &mut CodegenContext<'ink, D>,
        module: &'a Module<'ink>,
        abi_types: &'a AbiTypes<'ink>,
        intrinsics: impl Iterator<Item = &'f FunctionPrototype>,
        dispatch_table: &'a DispatchTable<'ink>,
    ) -> Self {
        let mut builder = Self {
            context,
            target_data: db.target_data(),
            module,
            abi_types,
            dispatch_table,
            entries: BTreeSet::new(),
        };

        for prototype in intrinsics {
            for arg_type in prototype.arg_types.iter() {
                builder.collect_type(db, arg_type.clone());
            }
            if let Some(ret_type) = prototype.ret_type.as_ref() {
                builder.collect_type(db, ret_type.clone());
            }
        }

        builder
    }

    /// Collects unique `TypeInfo` from the given `Ty`.
    fn collect_type<D: hir::HirDatabase>(&mut self, db: &mut CodegenContext<'ink, D>, type_info: TypeInfo) {
        if let TypeGroup::StructTypes(hir_struct) = type_info.group {
            self.collect_struct(db, hir_struct);
        } else {
            self.entries.insert(type_info);
        }
    }

    /// Collects unique `TypeInfo` from the specified expression and its sub-expressions.
    fn collect_expr(&mut self, expr_id: ExprId, body: &Arc<Body>, infer: &InferenceResult) {
        let expr = &body[expr_id];

        // TODO: Collect used external `TypeInfo` for the type dispatch table

        // Recurse further
        expr.walk_child_exprs(|expr_id| self.collect_expr(expr_id, body, infer))
    }

    /// Collects unique `TypeInfo` from the specified function signature and body.
    pub fn collect_fn<D: hir::HirDatabase>(&mut self, db: &mut CodegenContext<'ink, D>, hir_fn: hir::Function) {
        // Collect type info for exposed function
        if !hir_fn.data(db.hir_db()).visibility().is_private() || self.dispatch_table.contains(hir_fn) {
            let fn_sig = hir_fn.ty(db.hir_db()).callable_sig(db.hir_db()).unwrap();

            // Collect argument types
            for ty in fn_sig.params().iter() {
                let ti = db.type_info(self.context, ty.clone());
                self.collect_type(db, ti);
            }

            // Collect return type
            let ret_ty = fn_sig.ret();
            if !ret_ty.is_empty() {
                let ti = db.type_info(self.context, ret_ty.clone());
                self.collect_type(db, ti);
            }
        }

        // Collect used types from body
        let body = hir_fn.body(db.hir_db());
        let infer = hir_fn.infer(db.hir_db());
        self.collect_expr(body.body_expr(), &body, &infer);
    }

    /// Collects unique `TypeInfo` from the specified struct type.
    pub fn collect_struct<D: hir::HirDatabase>(&mut self, db: &mut CodegenContext<'ink, D>, hir_struct: hir::Struct) {
        let type_info = db.type_info(self.context, hir_struct.ty(db.hir_db()));
        self.entries.insert(type_info);

        let fields = hir_struct.fields(db.hir_db());
        for field in fields.into_iter() {
            let ti = db.type_info(self.context, field.ty(db.hir_db()));
            self.collect_type(db, ti);
        }
    }

    fn gen_type_info<D: hir::HirDatabase>(
        &self,
        db: &mut CodegenContext<'ink, D>,
        type_info_to_ir: &mut HashMap<TypeInfo, GlobalValue<'ink>>,
        type_info: &TypeInfo,
    ) -> GlobalValue<'ink> {
        let guid_bytes_ir: [IntValue; 16] = array_init::array_init(|i| {
            self.context
                .i8_type()
                .const_int(u64::from(type_info.guid.b[i]), false)
        });
        let type_info_ir = self.abi_types.type_info_type.const_named_struct(&[
            self.context.i8_type().const_array(&guid_bytes_ir).into(),
            intern_string(
                &self.context,
                self.module,
                &type_info.name,
                &format!("type_info::<{}>::name", type_info.name),
            )
            .into(),
            self.context
                .i32_type()
                .const_int(type_info.size.bit_size, false)
                .into(),
            self.context
                .i8_type()
                .const_int(type_info.size.alignment as u64, false)
                .into(),
            self.context
                .i8_type()
                .const_int(type_info.group.clone().into(), false)
                .into(),
        ]);
        let type_info_ir = match type_info.group {
            TypeGroup::FundamentalTypes => type_info_ir,
            TypeGroup::StructTypes(s) => {
                let struct_info_ir = self.gen_struct_info(db, type_info_to_ir, s);
                self.context.const_struct(&[type_info_ir.into(), struct_info_ir.into()], false)
            }
        };
        gen_global(
            self.module,
            &type_info_ir,
            &type_info_global_name(type_info),
        )
    }

    fn gen_struct_info<D: hir::HirDatabase>(
        &self,
        db: &mut CodegenContext<'ink, D>,
        type_info_to_ir: &mut HashMap<TypeInfo, GlobalValue<'ink>>,
        hir_struct: hir::Struct,
    ) -> StructValue {
        let name = hir_struct.name(db.hir_db()).to_string();
        let fields = hir_struct.fields(db.hir_db());
        println!("computed fields inside gen_struct_info: {}", fields.len());
        let struct_ir = db.struct_ty(self.context, hir_struct);

        // This is because the struct is generated with opaque_struct_type
        println!("llvm ir has {} fields", struct_ir.count_fields());

        let field_names = gen_string_array(
            &self.context,
            self.module,
            fields.iter().map(|field| field.name(db.hir_db()).to_string()),
            &format!("struct_info::<{}>::field_names", name),
        );
        let field_types: Vec<PointerValue> = fields
            .iter()
            .map(|field| {
                let field_type_info = db.type_info(self.context, field.ty(db.hir_db()));
                if let Some(ir_value) = type_info_to_ir.get(&field_type_info) {
                    *ir_value
                } else {
                    let ir_value = self.gen_type_info(db, type_info_to_ir, &field_type_info);
                    type_info_to_ir.insert(field_type_info, ir_value);
                    ir_value
                }
                .as_pointer_value()
            })
            .collect();

        let field_types = gen_struct_ptr_array(
            self.module,
            self.abi_types.type_info_type,
            &field_types,
            &format!("struct_info::<{}>::field_types", name),
        );

        let field_offsets = gen_u16_array(
            &self.context,
            self.module,
            (0..fields.len()).map(|idx| {
                self.target_data
                    .offset_of_element(&struct_ir, idx as u32)
                    .unwrap()
            }),
            &format!("struct_info::<{}>::field_offsets", name),
        );

        self.abi_types.struct_info_type.const_named_struct(&[
            field_names.into(),
            field_types.into(),
            field_offsets.into(),
            self.context
                .i16_type()
                .const_int(fields.len() as u64, false)
                .into(),
            self.context
                .i8_type()
                .const_int(hir_struct.data(db.hir_db()).memory_kind.clone().into(), false)
                .into(),
        ])
    }

    /// Constructs a `TypeTable` from all *used* types.
    pub fn build<D: hir::HirDatabase>(mut self, db: &mut CodegenContext<'ink, D>) -> TypeTable<'ink> {
        let mut entries = BTreeSet::new();
        mem::swap(&mut entries, &mut self.entries);

        let mut type_info_to_ir = HashMap::with_capacity(entries.len());
        let mut type_info_to_index = HashMap::with_capacity(entries.len());

        let type_info_ptr_type = self.abi_types.type_info_type.ptr_type(AddressSpace::Const);
        let table_type = type_info_ptr_type.array_type(
            entries
                .len()
                .try_into()
                .expect("expected a maximum of u32::MAX entries"),
        );

        let type_info_ptrs: Vec<PointerValue> = entries
            .into_iter()
            .enumerate()
            .map(|(index, type_info)| {
                let ptr = if let Some(ir_value) = type_info_to_ir.get(&type_info) {
                    *ir_value
                } else {
                    let ir_value = self.gen_type_info(db, &mut type_info_to_ir, &type_info);
                    type_info_to_ir.insert(type_info.clone(), ir_value);
                    ir_value
                }
                .as_pointer_value();

                type_info_to_index.insert(type_info, index);
                ptr
            })
            .collect();

        if !type_info_ptrs.is_empty() {
            let global = self.module.add_global(table_type, None, TypeTable::NAME);

            let type_info_ptrs_array = type_info_ptr_type.const_array(&type_info_ptrs);
            global.set_initializer(&type_info_ptrs_array);
        };

        TypeTable {
            type_info_to_index,
            entries: type_info_ptrs,
            table_type,
        }
    }
}

fn type_info_global_name(type_info: &TypeInfo) -> String {
    format!("type_info::<{}>", type_info.name)
}
