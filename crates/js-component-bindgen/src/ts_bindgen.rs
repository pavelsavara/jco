use crate::files::Files;
use crate::function_bindgen::{array_ty, as_nullable, maybe_null};
use crate::names::{maybe_quote_id, LocalNames};
use crate::source::Source;
use crate::transpile_bindgen::{parse_world_key, TranspileOpts};
use crate::{uwrite, uwriteln};
use heck::*;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;
use std::mem;
use wit_parser::abi::AbiVariant;
use wit_parser::*;

struct TsBindgen {
    /// The source code for the "main" file that's going to be created for the
    /// component we're generating bindings for. This is incrementally added to
    /// over time and primarily contains the main `instantiate` function as well
    /// as a type-description of the input/output interfaces.
    src: Source,

    export_names: LocalNames,
    import_names: LocalNames,
    local_names: LocalNames,

    /// TypeScript definitions which will become the import object
    import_object: Source,
    /// TypeScript definitions which will become the export object
    export_object: Source,
}

/// Used to generate a `*.d.ts` file for each imported and exported interface for
/// a component.
///
/// This generated source does not contain any actual JS runtime code, it's just
/// typescript definitions.
struct TsInterface<'a> {
    src: Source,
    resolve: &'a Resolve,
    needs_ty_option: bool,
    needs_ty_result: bool,
    resources: HashMap<String, TsInterface<'a>>,
}

pub fn ts_bindgen(
    name: &str,
    resolve: &Resolve,
    id: WorldId,
    opts: &TranspileOpts,
    files: &mut Files,
) {
    let mut bindgen = TsBindgen {
        src: Source::default(),
        export_names: LocalNames::default(),
        import_names: LocalNames::default(),
        local_names: LocalNames::default(),
        import_object: Source::default(),
        export_object: Source::default(),
    };

    let world = &resolve.worlds[id];

    {
        let mut funcs = Vec::new();
        let mut interface_imports = BTreeMap::new();
        for (name, import) in world.imports.iter() {
            match import {
                WorldItem::Function(f) => match name {
                    WorldKey::Name(name) => funcs.push((name.to_string(), f)),
                    WorldKey::Interface(id) => funcs.push((resolve.id_of(*id).unwrap(), f)),
                },
                WorldItem::Interface(id) => match name {
                    WorldKey::Name(name) => {
                        // kebab name -> direct ns namespace import
                        bindgen.import_interface(resolve, name, *id, files);
                    }
                    // namespaced ns:pkg/iface
                    // TODO: map support
                    WorldKey::Interface(id) => {
                        let import_specifier = resolve.id_of(*id).unwrap();
                        let (_, _, iface) = parse_world_key(&import_specifier).unwrap();
                        let iface = iface.to_string();
                        match interface_imports.entry(import_specifier) {
                            Entry::Vacant(entry) => {
                                entry.insert(vec![("*".into(), id)]);
                            }
                            Entry::Occupied(ref mut entry) => {
                                entry.get_mut().push((iface, id));
                            }
                        }
                    }
                },
                WorldItem::Type(tid) => {
                    let ty = &resolve.types[*tid];

                    let name = ty.name.as_ref().unwrap();

                    let mut gen = bindgen.ts_interface(resolve);
                    gen.docs(&ty.docs);
                    match &ty.kind {
                        TypeDefKind::Record(record) => {
                            gen.type_record(*tid, name, record, &ty.docs)
                        }
                        TypeDefKind::Flags(flags) => gen.type_flags(*tid, name, flags, &ty.docs),
                        TypeDefKind::Tuple(tuple) => gen.type_tuple(*tid, name, tuple, &ty.docs),
                        TypeDefKind::Enum(enum_) => gen.type_enum(*tid, name, enum_, &ty.docs),
                        TypeDefKind::Variant(variant) => {
                            gen.type_variant(*tid, name, variant, &ty.docs)
                        }
                        TypeDefKind::Option(t) => gen.type_option(*tid, name, t, &ty.docs),
                        TypeDefKind::Result(r) => gen.type_result(*tid, name, r, &ty.docs),
                        TypeDefKind::List(t) => gen.type_list(*tid, name, t, &ty.docs),
                        TypeDefKind::Type(t) => {
                            gen.type_alias(*tid, name, t, None, AbiVariant::GuestImport, &ty.docs)
                        }
                        TypeDefKind::Future(_) => todo!("generate for future"),
                        TypeDefKind::Stream(_) => todo!("generate for stream"),
                        TypeDefKind::Unknown => unreachable!(),
                        TypeDefKind::Resource => todo!(),
                        TypeDefKind::Handle(_) => todo!(),
                    }
                    let output = gen.finish();
                    bindgen.src.push_str(&output);
                }
            }
        }
        // kebab import funcs (always default imports)
        for (name, func) in funcs {
            bindgen.import_funcs(resolve, &name, &func, files);
        }
        // namespace imports are grouped by namespace / kebab name
        // kebab name imports are direct
        for (name, import_interfaces) in interface_imports {
            bindgen.import_interfaces(resolve, name.as_ref(), import_interfaces, files);
        }
    }

    let mut funcs = Vec::new();
    let mut seen_names = HashSet::new();
    let mut export_aliases: Vec<(String, String)> = Vec::new();
    for (name, export) in world.exports.iter() {
        match export {
            WorldItem::Function(f) => {
                let export_name = match name {
                    WorldKey::Name(export_name) => export_name,
                    WorldKey::Interface(_) => unreachable!(),
                };
                seen_names.insert(export_name.to_string());
                funcs.push((export_name.to_lower_camel_case(), f));
            }
            WorldItem::Interface(id) => {
                let iface_id: String;
                let (export_name, iface_name): (&str, &str) = match name {
                    WorldKey::Name(export_name) => (export_name, export_name),
                    WorldKey::Interface(interface) => {
                        iface_id = resolve.id_of(*interface).unwrap();
                        let (_, _, iface) = parse_world_key(&iface_id).unwrap();
                        (iface_id.as_ref(), iface)
                    }
                };
                seen_names.insert(export_name.to_string());
                let local_name =
                    bindgen.export_interface(resolve, &export_name, *id, files, opts.instantiation);
                export_aliases.push((iface_name.to_lower_camel_case(), local_name));
            }
            WorldItem::Type(_) => unimplemented!("type exports"),
        }
    }
    for (alias, local_name) in export_aliases {
        if !seen_names.contains(&alias) {
            if opts.instantiation {
                uwriteln!(
                    bindgen.export_object,
                    "{}: typeof {},",
                    alias,
                    local_name.to_upper_camel_case()
                );
            } else {
                uwriteln!(
                    bindgen.export_object,
                    "export const {}: typeof {};",
                    alias,
                    local_name.to_upper_camel_case()
                );
            }
        }
    }
    if !funcs.is_empty() {
        bindgen.export_funcs(resolve, id, &funcs, files, !opts.instantiation);
    }

    let camel = world.name.to_upper_camel_case();

    // Generate a type definition for the import object to type-check
    // all imports to the component.
    //
    // With the current representation of a "world" this is an import object
    // per-imported-interface where the type of that field is defined by the
    // interface itbindgen.
    if opts.instantiation {
        uwriteln!(bindgen.src, "export interface ImportObject {{");
        bindgen.src.push_str(&bindgen.import_object);
        uwriteln!(bindgen.src, "}}");
    }

    // Generate a type definition for the export object from instantiating
    // the component.
    if opts.instantiation {
        uwriteln!(bindgen.src, "export interface {camel} {{",);
        bindgen.src.push_str(&bindgen.export_object);
        uwriteln!(bindgen.src, "}}");
    } else {
        bindgen.src.push_str(&bindgen.export_object);
    }

    if opts.tla_compat {
        uwriteln!(
            bindgen.src,
            "
            export const $init: Promise<void>;"
        );
    }

    // Generate the TypeScript definition of the `instantiate` function
    // which is the main workhorse of the generated bindings.
    if opts.instantiation {
        uwriteln!(
            bindgen.src,
            "
            /**
                * Instantiates this component with the provided imports and
                * returns a map of all the exports of the component.
                *
                * This function is intended to be similar to the
                * `WebAssembly.instantiate` function. The second `imports`
                * argument is the \"import object\" for wasm, except here it
                * uses component-model-layer types instead of core wasm
                * integers/numbers/etc.
                *
                * The first argument to this function, `compileCore`, is
                * used to compile core wasm modules within the component.
                * Components are composed of core wasm modules and this callback
                * will be invoked per core wasm module. The caller of this
                * function is responsible for reading the core wasm module
                * identified by `path` and returning its compiled
                * WebAssembly.Module object. This would use `compileStreaming`
                * on the web, for example.
                */
                export function instantiate(
                    compileCore: (path: string, imports: Record<string, any>) => Promise<WebAssembly.Module>,
                    imports: ImportObject,
                    instantiateCore?: (module: WebAssembly.Module, imports: Record<string, any>) => Promise<WebAssembly.Instance>
                ): Promise<{camel}>;
            ",
        );
    }

    files.push(&format!("{name}.d.ts"), bindgen.src.as_bytes());
}

impl TsBindgen {
    fn import_interface(
        &mut self,
        resolve: &Resolve,
        name: &str,
        id: InterfaceId,
        files: &mut Files,
    ) -> String {
        // in case an imported type is used as an exported type
        self.generate_interface(name, resolve, id, files, AbiVariant::GuestExport, false);
        let local_name =
            self.generate_interface(name, resolve, id, files, AbiVariant::GuestImport, true);
        uwriteln!(
            self.import_object,
            "{}: typeof {local_name},",
            maybe_quote_id(name)
        );
        local_name
    }
    fn import_interfaces(
        &mut self,
        resolve: &Resolve,
        import_name: &str,
        ifaces: Vec<(String, &InterfaceId)>,
        files: &mut Files,
    ) {
        if ifaces.len() == 1 {
            let (iface_name, &id) = ifaces.iter().next().unwrap();
            if iface_name == "*" {
                uwrite!(self.import_object, "{}: ", maybe_quote_id(import_name));
                let name = resolve.interfaces[id].name.as_ref().unwrap();
                // in case an imported type is used as an exported type
                self.generate_interface(&name, resolve, id, files, AbiVariant::GuestExport, false);
                let local_name = self.generate_interface(
                    &name,
                    resolve,
                    id,
                    files,
                    AbiVariant::GuestImport,
                    true,
                );
                uwriteln!(self.import_object, "typeof {local_name},",);
                return;
            }
        }
        uwriteln!(self.import_object, "{}: {{", maybe_quote_id(import_name));
        for (iface_name, &id) in ifaces {
            let name = resolve.interfaces[id].name.as_ref().unwrap();
            // in case an imported type is used as an exported type
            self.generate_interface(&name, resolve, id, files, AbiVariant::GuestExport, false);
            let local_name =
                self.generate_interface(&name, resolve, id, files, AbiVariant::GuestImport, true);
            uwriteln!(
                self.import_object,
                "{}: typeof {local_name},",
                iface_name.to_lower_camel_case()
            );
        }
        uwriteln!(self.import_object, "}},");
    }

    fn import_funcs(
        &mut self,
        resolve: &Resolve,
        import_name: &str,
        func: &Function,
        _files: &mut Files,
    ) {
        uwriteln!(self.import_object, "{}: {{", maybe_quote_id(import_name));
        let mut gen = self.ts_interface(resolve);
        gen.ts_func(func, AbiVariant::GuestImport, true, false);
        let src = gen.finish();
        self.import_object.push_str(&src);
        uwriteln!(self.import_object, "}},");
    }

    fn export_interface(
        &mut self,
        resolve: &Resolve,
        export_name: &str,
        id: InterfaceId,
        files: &mut Files,
        instantiation: bool,
    ) -> String {
        let local_name = self.generate_interface(
            export_name,
            resolve,
            id,
            files,
            AbiVariant::GuestExport,
            true,
        );
        if instantiation {
            uwriteln!(
                self.export_object,
                "{}: typeof {local_name},",
                maybe_quote_id(export_name)
            );
        } else {
            if export_name != maybe_quote_id(export_name) {
                // TODO: TypeScript doesn't currently support
                // non-identifier exports
                // tracking in https://github.com/microsoft/TypeScript/issues/40594
            } else {
                uwriteln!(
                    self.export_object,
                    "export const {}: typeof {local_name};",
                    maybe_quote_id(export_name)
                );
            }
        }
        local_name
    }

    fn export_funcs(
        &mut self,
        resolve: &Resolve,
        _world: WorldId,
        funcs: &[(String, &Function)],
        _files: &mut Files,
        declaration: bool,
    ) {
        let mut gen = self.ts_interface(resolve);
        for (_, func) in funcs {
            gen.ts_func(func, AbiVariant::GuestExport, false, declaration);
        }
        let src = gen.finish();
        self.export_object.push_str(&src);
    }

    fn generate_interface(
        &mut self,
        name: &str,
        resolve: &Resolve,
        id: InterfaceId,
        files: &mut Files,
        abi: AbiVariant,
        used: bool,
    ) -> String {
        let dir = match abi {
            AbiVariant::GuestImport => "imports",
            AbiVariant::GuestExport => "exports",
        };
        let id_name = resolve.id_of(id).unwrap_or_else(|| name.to_string());
        let goal_name = interface_goal_name(&id_name);
        let goal_name_kebab = goal_name.to_kebab_case();
        let file_name = &format!("{dir}/{}.d.ts", goal_name_kebab);
        let (name, exists) = match abi {
            AbiVariant::GuestImport => &mut self.import_names,
            AbiVariant::GuestExport => &mut self.export_names,
        }
        .get_or_create(&file_name, &goal_name);

        let camel = name.to_upper_camel_case();

        let local_name = if used {
            self.local_names
                .get_or_create(&file_name, &goal_name)
                .0
                .to_upper_camel_case()
        } else {
            camel.to_string()
        };

        if used {
            uwriteln!(
                self.src,
                "import {{ {} }} from './{}';",
                if camel == local_name {
                    camel.to_string()
                } else {
                    format!("{camel} as {local_name}")
                },
                &file_name[0..file_name.len() - 5]
            );
        }

        if exists {
            return local_name;
        }

        let mut gen = self.ts_interface(resolve);

        uwriteln!(gen.src, "export namespace {camel} {{");
        for (_, func) in resolve.interfaces[id].functions.iter() {
            gen.ts_func(func, abi, false, true);
        }
        uwriteln!(gen.src, "}}");

        gen.types(id, abi);
        gen.post_types();

        let src = gen.finish();

        files.push(file_name, src.as_bytes());

        local_name
    }

    fn ts_interface<'b>(&'b mut self, resolve: &'b Resolve) -> TsInterface<'b> {
        TsInterface {
            src: Source::default(),
            resources: HashMap::new(),
            resolve,
            needs_ty_option: false,
            needs_ty_result: false,
        }
    }
}

#[derive(Copy, Clone)]
enum Mode {
    Lift,
    Lower,
}

impl<'a> TsInterface<'a> {
    fn new(resolve: &'a Resolve) -> Self {
        TsInterface {
            src: Source::default(),
            resources: HashMap::new(),
            resolve,
            needs_ty_option: false,
            needs_ty_result: false,
        }
    }

    fn finish(mut self) -> Source {
        for (resource, source) in self.resources {
            uwriteln!(
                self.src,
                "\nexport class {} {{",
                resource.to_upper_camel_case()
            );
            self.src.push_str(&source.src);
            uwriteln!(self.src, "}}")
        }
        self.src
    }

    fn docs_raw(&mut self, docs: &str) {
        self.src.push_str("/**\n");
        for line in docs.lines() {
            self.src.push_str(&format!(" * {}\n", line));
        }
        self.src.push_str(" */\n");
    }

    fn docs(&mut self, docs: &Docs) {
        match &docs.contents {
            Some(docs) => self.docs_raw(docs),
            None => return,
        }
    }

    fn types(&mut self, iface_id: InterfaceId, abi: AbiVariant) {
        let iface = &self.resolve().interfaces[iface_id];
        for (name, id) in iface.types.iter() {
            let id = *id;
            let ty = &self.resolve().types[id];
            match &ty.kind {
                TypeDefKind::Record(record) => self.type_record(id, name, record, &ty.docs),
                TypeDefKind::Flags(flags) => self.type_flags(id, name, flags, &ty.docs),
                TypeDefKind::Tuple(tuple) => self.type_tuple(id, name, tuple, &ty.docs),
                TypeDefKind::Enum(enum_) => self.type_enum(id, name, enum_, &ty.docs),
                TypeDefKind::Variant(variant) => self.type_variant(id, name, variant, &ty.docs),
                TypeDefKind::Option(t) => self.type_option(id, name, t, &ty.docs),
                TypeDefKind::Result(r) => self.type_result(id, name, r, &ty.docs),
                TypeDefKind::List(t) => self.type_list(id, name, t, &ty.docs),
                TypeDefKind::Type(t) => self.type_alias(id, name, t, Some(iface_id), abi, &ty.docs),
                TypeDefKind::Future(_) => todo!("generate for future"),
                TypeDefKind::Stream(_) => todo!("generate for stream"),
                TypeDefKind::Unknown => unreachable!(),
                TypeDefKind::Resource => {}
                TypeDefKind::Handle(_) => todo!(),
            }
        }
    }

    fn print_ty(&mut self, ty: &Type, mode: Mode) {
        match ty {
            Type::Bool => self.src.push_str("boolean"),
            Type::U8
            | Type::S8
            | Type::U16
            | Type::S16
            | Type::U32
            | Type::S32
            | Type::Float32
            | Type::Float64 => self.src.push_str("number"),
            Type::U64 | Type::S64 => self.src.push_str("bigint"),
            Type::Char => self.src.push_str("string"),
            Type::String => self.src.push_str("string"),
            Type::Id(id) => {
                let ty = &self.resolve.types[*id];
                if let Some(name) = &ty.name {
                    return self.src.push_str(&name.to_upper_camel_case());
                }
                match &ty.kind {
                    TypeDefKind::Type(t) => self.print_ty(t, mode),
                    TypeDefKind::Tuple(t) => self.print_tuple(t, mode),
                    TypeDefKind::Record(_) => panic!("anonymous record"),
                    TypeDefKind::Flags(_) => panic!("anonymous flags"),
                    TypeDefKind::Enum(_) => panic!("anonymous enum"),
                    TypeDefKind::Option(t) => {
                        if maybe_null(self.resolve, t) {
                            self.needs_ty_option = true;
                            self.src.push_str("Option<");
                            self.print_ty(t, mode);
                            self.src.push_str(">");
                        } else {
                            self.print_ty(t, mode);
                            self.src.push_str(" | null");
                        }
                    }
                    TypeDefKind::Result(r) => {
                        self.needs_ty_result = true;
                        self.src.push_str("Result<");
                        self.print_optional_ty(r.ok.as_ref(), mode);
                        self.src.push_str(", ");
                        self.print_optional_ty(r.err.as_ref(), mode);
                        self.src.push_str(">");
                    }
                    TypeDefKind::Variant(_) => panic!("anonymous variant"),
                    TypeDefKind::List(v) => self.print_list(v, mode),
                    TypeDefKind::Future(_) => todo!("anonymous future"),
                    TypeDefKind::Stream(_) => todo!("anonymous stream"),
                    TypeDefKind::Unknown => unreachable!(),
                    TypeDefKind::Resource => todo!(),
                    TypeDefKind::Handle(h) => {
                        let ty = match h {
                            Handle::Own(r) => r,
                            Handle::Borrow(r) => r,
                        };
                        let ty = &self.resolve.types[*ty];
                        if let Some(name) = &ty.name {
                            return self.src.push_str(&name.to_upper_camel_case());
                        }
                        panic!("anonymous resource handle");
                    }
                }
            }
        }
    }

    fn print_optional_ty(&mut self, ty: Option<&Type>, mode: Mode) {
        match ty {
            Some(ty) => self.print_ty(ty, mode),
            None => self.src.push_str("void"),
        }
    }

    fn print_list(&mut self, ty: &Type, mode: Mode) {
        match array_ty(self.resolve, ty) {
            Some("Uint8Array") => match mode {
                Mode::Lift => self.src.push_str("Uint8Array"),
                Mode::Lower => self.src.push_str("Uint8Array | ArrayBuffer"),
            },
            Some(ty) => self.src.push_str(ty),
            None => {
                self.print_ty(ty, mode);
                self.src.push_str("[]");
            }
        }
    }

    fn print_tuple(&mut self, tuple: &Tuple, mode: Mode) {
        self.src.push_str("[");
        for (i, ty) in tuple.types.iter().enumerate() {
            if i > 0 {
                self.src.push_str(", ");
            }
            self.print_ty(ty, mode);
        }
        self.src.push_str("]");
    }

    fn ts_func(&mut self, func: &Function, abi: AbiVariant, default: bool, declaration: bool) {
        self.docs(&func.docs);

        let iface = if let FunctionKind::Method(ty)
        | FunctionKind::Static(ty)
        | FunctionKind::Constructor(ty) = func.kind
        {
            let ty = &self.resolve.types[ty];
            let resource = ty.name.as_ref().unwrap();
            if !self.resources.contains_key(resource) {
                uwriteln!(self.src, "export {{ {} }};", resource.to_upper_camel_case());
                self.resources
                    .insert(resource.to_string(), TsInterface::new(self.resolve));
            }
            self.resources.get_mut(resource).unwrap()
        } else {
            self
        };

        let is_resource = !matches!(func.kind, FunctionKind::Freestanding);
        if is_resource {}

        let end_character = if declaration {
            iface.src.push_str(match func.kind {
                FunctionKind::Freestanding => "export function ",
                FunctionKind::Method(_) => "",
                FunctionKind::Static(_) => "static ",
                FunctionKind::Constructor(_) => "",
            });
            ';'
        } else {
            ','
        };

        if default {
            iface.src.push_str("default");
        } else {
            iface.src.push_str(&func.item_name().to_lower_camel_case());
        }
        iface.src.push_str("(");

        let param_start = match &func.kind {
            FunctionKind::Freestanding => 0,
            FunctionKind::Method(_) => 1,
            FunctionKind::Static(_) => 0,
            FunctionKind::Constructor(_) => 0,
        };

        for (i, (name, ty)) in func.params[param_start..].iter().enumerate() {
            if i > 0 {
                iface.src.push_str(", ");
            }
            iface.src.push_str(&name.to_lower_camel_case());
            iface.src.push_str(": ");
            iface.print_ty(
                ty,
                match abi {
                    AbiVariant::GuestExport => Mode::Lower,
                    AbiVariant::GuestImport => Mode::Lift,
                },
            );
        }

        iface.src.push_str(")");
        if matches!(func.kind, FunctionKind::Constructor(_)) {
            iface.src.push_str("\n");
            return;
        }
        iface.src.push_str(": ");

        let result_mode = match abi {
            AbiVariant::GuestExport => Mode::Lift,
            AbiVariant::GuestImport => Mode::Lower,
        };
        if let Some((ok_ty, _)) = func.results.throws(iface.resolve) {
            iface.print_optional_ty(ok_ty, result_mode);
        } else {
            match func.results.len() {
                0 => iface.src.push_str("void"),
                1 => iface.print_ty(func.results.iter_types().next().unwrap(), result_mode),
                _ => {
                    iface.src.push_str("[");
                    for (i, ty) in func.results.iter_types().enumerate() {
                        if i != 0 {
                            iface.src.push_str(", ");
                        }
                        iface.print_ty(ty, result_mode);
                    }
                    iface.src.push_str("]");
                }
            }
        }
        iface.src.push_str(format!("{}\n", end_character).as_str());
    }

    fn post_types(&mut self) {
        if mem::take(&mut self.needs_ty_option) {
            self.src
                .push_str("export type Option<T> = { tag: 'none' } | { tag: 'some', val: T };\n");
        }
        if mem::take(&mut self.needs_ty_result) {
            self.src.push_str(
                "export type Result<T, E> = { tag: 'ok', val: T } | { tag: 'err', val: E };\n",
            );
        }
    }

    fn resolve(&self) -> &'a Resolve {
        self.resolve
    }

    fn type_record(&mut self, _id: TypeId, name: &str, record: &Record, docs: &Docs) {
        self.docs(docs);
        self.src.push_str(&format!(
            "export interface {} {{\n",
            name.to_upper_camel_case()
        ));
        for field in record.fields.iter() {
            self.docs(&field.docs);
            let (option_str, ty) =
                as_nullable(self.resolve, &field.ty).map_or(("", &field.ty), |ty| ("?", ty));
            self.src.push_str(&format!(
                "{}{}: ",
                maybe_quote_id(&field.name.to_lower_camel_case()),
                option_str
            ));
            self.print_ty(ty, Mode::Lift);
            self.src.push_str(",\n");
        }
        self.src.push_str("}\n");
    }

    fn type_tuple(&mut self, _id: TypeId, name: &str, tuple: &Tuple, docs: &Docs) {
        self.docs(docs);
        self.src
            .push_str(&format!("export type {} = ", name.to_upper_camel_case()));
        self.print_tuple(tuple, Mode::Lift);
        self.src.push_str(";\n");
    }

    fn type_flags(&mut self, _id: TypeId, name: &str, flags: &Flags, docs: &Docs) {
        self.docs(docs);
        self.src.push_str(&format!(
            "export interface {} {{\n",
            name.to_upper_camel_case()
        ));
        for flag in flags.flags.iter() {
            self.docs(&flag.docs);
            let name = flag.name.to_lower_camel_case();
            self.src.push_str(&format!("{name}?: boolean,\n"));
        }
        self.src.push_str("}\n");
    }

    fn type_variant(&mut self, _id: TypeId, name: &str, variant: &Variant, docs: &Docs) {
        self.docs(docs);
        self.src
            .push_str(&format!("export type {} = ", name.to_upper_camel_case()));
        for (i, case) in variant.cases.iter().enumerate() {
            if i > 0 {
                self.src.push_str(" | ");
            }
            self.src
                .push_str(&format!("{}_{}", name, case.name).to_upper_camel_case());
        }
        self.src.push_str(";\n");
        for case in variant.cases.iter() {
            self.docs(&case.docs);
            self.src.push_str(&format!(
                "export interface {} {{\n",
                format!("{}_{}", name, case.name).to_upper_camel_case()
            ));
            self.src.push_str("tag: '");
            self.src.push_str(&case.name);
            self.src.push_str("',\n");
            if let Some(ty) = case.ty {
                self.src.push_str("val: ");
                self.print_ty(&ty, Mode::Lift);
                self.src.push_str(",\n");
            }
            self.src.push_str("}\n");
        }
    }

    fn type_option(&mut self, _id: TypeId, name: &str, payload: &Type, docs: &Docs) {
        self.docs(docs);
        let name = name.to_upper_camel_case();
        self.src.push_str(&format!("export type {name} = "));
        if maybe_null(self.resolve, payload) {
            self.needs_ty_option = true;
            self.src.push_str("Option<");
            self.print_ty(payload, Mode::Lift);
            self.src.push_str(">");
        } else {
            self.print_ty(payload, Mode::Lift);
            self.src.push_str(" | null");
        }
        self.src.push_str(";\n");
    }

    fn type_result(&mut self, _id: TypeId, name: &str, result: &Result_, docs: &Docs) {
        self.docs(docs);
        let name = name.to_upper_camel_case();
        self.needs_ty_result = true;
        self.src.push_str(&format!("export type {name} = Result<"));
        self.print_optional_ty(result.ok.as_ref(), Mode::Lift);
        self.src.push_str(", ");
        self.print_optional_ty(result.err.as_ref(), Mode::Lift);
        self.src.push_str(">;\n");
    }

    fn type_enum(&mut self, _id: TypeId, name: &str, enum_: &Enum, docs: &Docs) {
        // The complete documentation for this enum, including documentation for variants.
        let mut complete_docs = String::new();

        if let Some(docs) = &docs.contents {
            complete_docs.push_str(docs);
            // Add a gap before the `# Variants` section.
            complete_docs.push('\n');
        }

        writeln!(complete_docs, "# Variants").unwrap();

        for case in enum_.cases.iter() {
            writeln!(complete_docs).unwrap();
            writeln!(complete_docs, "## `\"{}\"`", case.name).unwrap();

            if let Some(docs) = &case.docs.contents {
                writeln!(complete_docs).unwrap();
                complete_docs.push_str(docs);
            }
        }

        self.docs_raw(&complete_docs);

        self.src
            .push_str(&format!("export type {} = ", name.to_upper_camel_case()));
        for (i, case) in enum_.cases.iter().enumerate() {
            if i != 0 {
                self.src.push_str(" | ");
            }
            self.src.push_str(&format!("'{}'", case.name));
        }
        self.src.push_str(";\n");
    }

    fn type_alias(
        &mut self,
        _id: TypeId,
        name: &str,
        ty: &Type,
        parent_id: Option<InterfaceId>,
        abi: AbiVariant,
        docs: &Docs,
    ) {
        let owner_not_parent = match ty {
            Type::Id(type_def_id) => {
                let ty = &self.resolve.types[*type_def_id];
                match ty.owner {
                    TypeOwner::Interface(i) => {
                        if let Some(parent_id) = parent_id {
                            if parent_id == i {
                                None
                            } else {
                                Some(interface_goal_name(&self.resolve.id_of(i).unwrap()))
                            }
                        } else {
                            Some(interface_goal_name(&self.resolve.id_of(i).unwrap()))
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        let type_name = name.to_upper_camel_case();
        match owner_not_parent {
            Some(owned_interface_name) => {
                let dir = match abi {
                    AbiVariant::GuestImport => "imports",
                    AbiVariant::GuestExport => "exports",
                };
                uwriteln!(
                    self.src,
                    "import type {{ {type_name} }} from '../{dir}/{owned_interface_name}';",
                );
                self.src.push_str(&format!("export {{ {} }};\n", type_name));
            }
            _ => {
                self.docs(docs);
                self.src.push_str(&format!("export type {} = ", type_name));
                self.print_ty(ty, Mode::Lift);
                self.src.push_str(";\n");
            }
        }
    }

    fn type_list(&mut self, _id: TypeId, name: &str, ty: &Type, docs: &Docs) {
        self.docs(docs);
        self.src
            .push_str(&format!("export type {} = ", name.to_upper_camel_case()));
        self.print_list(ty, Mode::Lift);
        self.src.push_str(";\n");
    }
}

fn interface_goal_name(iface_name: &str) -> String {
    iface_name
        .replace('/', "-")
        .replace(':', "-")
        .to_kebab_case()
}
