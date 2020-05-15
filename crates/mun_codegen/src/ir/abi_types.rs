use inkwell::context::Context;
use inkwell::types::{ArrayType, IntType, StructType};
use inkwell::AddressSpace;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AbiTypes<'ink> {
    pub guid_type: ArrayType<'ink>,
    pub type_group_type: IntType<'ink>,
    pub privacy_type: IntType<'ink>,
    pub type_info_type: StructType<'ink>,
    pub function_signature_type: StructType<'ink>,
    pub function_prototype_type: StructType<'ink>,
    pub function_definition_type: StructType<'ink>,
    pub struct_info_type: StructType<'ink>,
    pub module_info_type: StructType<'ink>,
    pub dispatch_table_type: StructType<'ink>,
    pub assembly_info_type: StructType<'ink>,
}

/// Returns an `AbiTypes` struct that contains references to all LLVM ABI types.
pub(crate) fn gen_abi_types(context: &Context) -> AbiTypes {
    let str_type = context.i8_type().ptr_type(AddressSpace::Const);

    // Construct the `MunGuid` type
    let guid_type = context.i8_type().array_type(16);

    // Construct the `MunTypeGroup` type
    let type_group_type = context.i8_type();

    // Construct the `MunPrivacy` enum
    let privacy_type = context.i8_type();

    // Construct the `MunTypeInfo` struct
    let type_info_type = context.opaque_struct_type("struct.MunTypeInfo");
    type_info_type.set_body(
        &[
            guid_type.into(),          // guid
            str_type.into(),           // name
            context.i32_type().into(), // size_in_bits
            context.i8_type().into(),  // alignment
            type_group_type.into(),    // group
        ],
        false,
    );

    let type_info_ptr_type = type_info_type.ptr_type(AddressSpace::Const);

    // Construct the `MunFunctionSignature` type
    let function_signature_type = context.opaque_struct_type("struct.MunFunctionSignature");
    function_signature_type.set_body(
        &[
            type_info_ptr_type.ptr_type(AddressSpace::Const).into(), // arg_types
            type_info_ptr_type.into(),                               // return_type
            context.i16_type().into(),                               // num_arg_types
        ],
        false,
    );

    // Construct the `MunFunctionSignature` type
    let function_prototype_type = context.opaque_struct_type("struct.MunFunctionPrototype");
    function_prototype_type.set_body(
        &[
            str_type.into(),                // name
            function_signature_type.into(), // signature
        ],
        false,
    );

    // Construct the `MunFunctionDefinition` struct
    let function_definition_type = context.opaque_struct_type("struct.MunFunctionDefinition");
    function_definition_type.set_body(
        &[
            function_prototype_type.into(), // prototype
            context
                .void_type()
                .fn_type(&[], false)
                .ptr_type(AddressSpace::Const)
                .into(), // fn_ptr
        ],
        false,
    );

    // Construct the `MunStructInfo` struct
    let struct_info_type = context.opaque_struct_type("struct.MunStructInfo");
    struct_info_type.set_body(
        &[
            str_type.ptr_type(AddressSpace::Const).into(), // field_names
            type_info_ptr_type.ptr_type(AddressSpace::Const).into(), // field_types
            context.i16_type().ptr_type(AddressSpace::Const).into(), // field_offsets
            context.i16_type().into(),                     // num_fields
            context.i8_type().into(),                      // memory_kind
        ],
        false,
    );

    // Construct the `MunModuleInfo` struct
    let module_info_type = context.opaque_struct_type("struct.MunModuleInfo");
    module_info_type.set_body(
        &[
            str_type.into(), // path
            function_definition_type
                .ptr_type(AddressSpace::Const)
                .into(), // functions
            context.i32_type().into(), // num_functions
            type_info_ptr_type.ptr_type(AddressSpace::Const).into(), // types
            context.i32_type().into(), // num_types
        ],
        false,
    );

    // Construct the `MunDispatchTable` struct
    let dispatch_table_type = context.opaque_struct_type("struct.MunDispatchTable");
    dispatch_table_type.set_body(
        &[
            function_signature_type.ptr_type(AddressSpace::Const).into(), // signatures
            context
                .void_type()
                .fn_type(&[], false)
                .ptr_type(AddressSpace::Generic)
                .ptr_type(AddressSpace::Const)
                .into(), // fn_ptrs
            context.i32_type().into(),                                    // num_entries
        ],
        false,
    );

    // Construct the `MunAssemblyInfo` struct
    let assembly_info_type = context.opaque_struct_type("struct.MunAssemblyInfo");
    assembly_info_type.set_body(
        &[
            module_info_type.into(),
            dispatch_table_type.into(),
            str_type.ptr_type(AddressSpace::Const).into(),
            context.i32_type().into(),
        ],
        false,
    );

    AbiTypes {
        guid_type,
        type_group_type,
        privacy_type,
        type_info_type,
        function_signature_type,
        function_prototype_type,
        function_definition_type,
        struct_info_type,
        module_info_type,
        dispatch_table_type,
        assembly_info_type,
    }
}
