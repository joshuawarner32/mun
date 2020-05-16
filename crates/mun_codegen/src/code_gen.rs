use inkwell::context::Context;
use crate::code_gen::linker::LinkerError;
use crate::CodegenContext;
use failure::Fail;
use hir::{FileId, RelativePathBuf};
use inkwell::{
    module::{Linkage, Module},
    passes::{PassManager, PassManagerBuilder},
    targets::{TargetTriple, CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine},
    types::StructType,
    values::{BasicValue, GlobalValue, IntValue, PointerValue, UnnamedAddress},
    AddressSpace, OptimizationLevel,
};
use mun_target::spec;
use std::io::{self, Write};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::NamedTempFile;

mod linker;
pub mod symbols;

#[derive(Debug, Fail)]
enum CodeGenerationError {
    #[fail(display = "{}", 0)]
    LinkerError(#[fail(cause)] LinkerError),
    #[fail(display = "error linking modules: {}", 0)]
    ModuleLinkerError(String),
    #[fail(display = "unknown target triple: {}", 0)]
    UnknownTargetTriple(String),
    #[fail(display = "error creating target machine")]
    CouldNotCreateTargetMachine,
    #[fail(display = "error creating object file")]
    CouldNotCreateObjectFile(io::Error),
    #[fail(display = "error generating machine code")]
    CodeGenerationError(String),
}

impl From<LinkerError> for CodeGenerationError {
    fn from(e: LinkerError) -> Self {
        CodeGenerationError::LinkerError(e)
    }
}

pub struct ObjectFile {
    target: spec::Target,
    src_path: RelativePathBuf,
    obj_file: NamedTempFile,
}

impl ObjectFile {
    pub fn new(
        target: &spec::Target,
        target_machine: &TargetMachine,
        src_path: RelativePathBuf,
        module: Arc<inkwell::module::Module>,
    ) -> Result<Self, failure::Error> {
        let obj = target_machine
            .write_to_memory_buffer(&module, FileType::Object)
            .map_err(|e| CodeGenerationError::CodeGenerationError(e.to_string()))?;

        let mut obj_file = tempfile::NamedTempFile::new()
            .map_err(CodeGenerationError::CouldNotCreateObjectFile)?;
        obj_file
            .write(obj.as_slice())
            .map_err(CodeGenerationError::CouldNotCreateObjectFile)?;

        Ok(Self {
            target: target.clone(),
            src_path,
            obj_file,
        })
    }

    pub fn into_shared_object(self, out_dir: Option<&Path>) -> Result<PathBuf, failure::Error> {
        // Construct a linker for the target
        let mut linker = linker::create_with_target(&self.target);
        linker.add_object(self.obj_file.path())?;

        let output_path = assembly_output_path(&self.src_path, out_dir);

        // Link the object
        linker.build_shared_object(&output_path)?;
        linker.finalize()?;

        Ok(output_path)
    }
}

/// A struct that can be used to build an LLVM `Module`.
pub struct ModuleBuilder<'a, 'ink, D: hir::HirDatabase> {
    context: &'ink Context,
    pub(crate) db: &'a mut CodegenContext<'ink, D>,
    file_id: FileId,
    _target: inkwell::targets::Target,
    target_machine: inkwell::targets::TargetMachine,
    assembly_module: Arc<inkwell::module::Module<'ink>>,
}

impl<'a, 'ink, D: hir::HirDatabase> ModuleBuilder<'a, 'ink, D> {
    /// Constructs module for the given `hir::FileId` at the specified output file location.
    pub fn new(context: &'ink Context, db: &'a mut CodegenContext<'ink, D>, file_id: FileId) -> Result<Self, failure::Error> {
        let target = db.hir_db().target();

        // Construct a module for the assembly
        let assembly_module = Arc::new(
            context.create_module(db.hir_db().file_relative_path(file_id).as_str()),
        );

        // Initialize the x86 target
        Target::initialize_x86(&InitializationConfig::default());

        // Retrieve the LLVM target using the specified target.
        let target_triple = TargetTriple::create(&target.llvm_target);
        let llvm_target = Target::from_triple(&target_triple)
            .map_err(|e| CodeGenerationError::UnknownTargetTriple(e.to_string()))?;
        assembly_module.set_triple(&target_triple);

        // Construct target machine for machine code generation
        let target_machine = llvm_target
            .create_target_machine(
                &target_triple,
                &target.options.cpu,
                &target.options.features,
                db.optimization_lvl(),
                RelocMode::PIC,
                CodeModel::Default,
            )
            .ok_or(CodeGenerationError::CouldNotCreateTargetMachine)?;

        Ok(Self {
            context,
            db,
            file_id,
            _target: llvm_target,
            target_machine,
            assembly_module,
        })
    }

    /// Constructs an object file.
    pub fn build(self) -> Result<ObjectFile, failure::Error> {
        let group_ir = self.db.group_ir(self.context, self.file_id);
        let file = self.db.file_ir(self.context, self.file_id);

        // Clone the LLVM modules so that we can modify it without modifying the cached value.
        self.assembly_module
            .link_in_module(group_ir.llvm_module.clone())
            .map_err(|e| CodeGenerationError::ModuleLinkerError(e.to_string()))?;

        self.assembly_module
            .link_in_module(file.llvm_module.clone())
            .map_err(|e| CodeGenerationError::ModuleLinkerError(e.to_string()))?;

        // Generate the `get_info` method.
        symbols::gen_reflection_ir(
            self.context,
            self.db,
            &self.assembly_module,
            &file.api,
            &group_ir.dispatch_table,
            &group_ir.type_table,
        );

        // Optimize the assembly module
        optimize_module(&self.assembly_module, self.db.optimization_lvl());

        // Debug print the IR
        //println!("{}", assembly_module.print_to_string().to_string());

        ObjectFile::new(
            &self.db.hir_db().target(),
            &self.target_machine,
            self.db.hir_db().file_relative_path(self.file_id),
            self.assembly_module,
        )
    }
}

/// Computes the output path for the assembly of the specified file.
fn assembly_output_path(src_path: &RelativePathBuf, out_dir: Option<&Path>) -> PathBuf {
    let original_filename = Path::new(src_path.file_name().unwrap());

    // Add the `munlib` suffix to the original filename
    let output_file_name = original_filename.with_extension("munlib");

    // If there is an out dir specified, prepend the output directory
    if let Some(out_dir) = out_dir {
        out_dir.join(output_file_name)
    } else {
        output_file_name
    }
}

/// Optimizes the specified LLVM `Module` using the default passes for the given
/// `OptimizationLevel`.
fn optimize_module<'ink>(module: &'ink Module, optimization_lvl: OptimizationLevel) {
    let pass_builder = PassManagerBuilder::create();
    pass_builder.set_optimization_level(optimization_lvl);

    let module_pass_manager = PassManager::create(());
    pass_builder.populate_module_pass_manager(&module_pass_manager);
    module_pass_manager.run_on(module);
}

/// Intern a string by constructing a global value. Looks something like this:
/// ```c
/// const char[] GLOBAL_ = "str";
/// ```
pub(crate) fn intern_string<'a, 'ink>(context: &'ink Context, module: &'a Module<'ink>, string: &str, name: &str) -> PointerValue<'ink> {
    let value = context.const_string(string.as_bytes(), true);
    gen_global(module, &value, name).as_pointer_value()
}

/// Construct a global from the specified value
pub(crate) fn gen_global<'a, 'ink>(module: &'a Module<'ink>, value: &dyn BasicValue<'ink>, name: &str) -> GlobalValue<'ink> {
    let global = module.add_global(value.as_basic_value_enum().get_type(), None, name);
    global.set_linkage(Linkage::Private);
    global.set_constant(true);
    global.set_unnamed_address(UnnamedAddress::Global);
    global.set_initializer(value);
    global
}

/// Generates a global array from the specified list of strings
pub(crate) fn gen_string_array<'a, 'ink: 'a>(
    context: &'ink Context,
    module: &'a Module<'ink>,
    strings: impl Iterator<Item = String>,
    name: &str,
) -> PointerValue<'ink> {
    let str_type = context.i8_type().ptr_type(AddressSpace::Const);

    let mut strings = strings.peekable();
    if strings.peek().is_none() {
        str_type.ptr_type(AddressSpace::Const).const_null()
    } else {
        let strings = strings
            .map(|s| intern_string(context, module, &s, name))
            .collect::<Vec<PointerValue>>();

        let strings_ir = str_type.const_array(&strings);
        gen_global(module, &strings_ir, "").as_pointer_value()
    }
}

/// Generates a global array from the specified list of struct pointers
pub(crate) fn gen_struct_ptr_array<'a, 'ink: 'a>(
    module: &'a Module<'ink>,
    ir_type: StructType<'ink>,
    ptrs: &[PointerValue<'ink>],
    name: &str,
) -> PointerValue<'ink> {
    if ptrs.is_empty() {
        ir_type
            .ptr_type(AddressSpace::Const)
            .ptr_type(AddressSpace::Const)
            .const_null()
    } else {
        let ptr_array_ir = ir_type.ptr_type(AddressSpace::Const).const_array(&ptrs);

        gen_global(module, &ptr_array_ir, name).as_pointer_value()
    }
}

/// Generates a global array from the specified list of integers
pub(crate) fn gen_u16_array<'a, 'ink: 'a>(
    context: &'ink Context,
    module: &'a Module<'ink>,
    integers: impl Iterator<Item = u64>,
    name: &str,
) -> PointerValue<'ink> {
    let u16_type = context.i16_type();

    let mut integers = integers.peekable();
    if integers.peek().is_none() {
        u16_type.ptr_type(AddressSpace::Const).const_null()
    } else {
        let integers = integers
            .map(|i| u16_type.const_int(i, false))
            .collect::<Vec<IntValue>>();

        let array_ir = u16_type.const_array(&integers);
        gen_global(module, &array_ir, name).as_pointer_value()
    }
}

