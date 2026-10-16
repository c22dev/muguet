// muguet - patch.rs
// Constantin Clerc (c22dev) - 0.1

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use wasm_encoder::reencode::{utils, Error, Reencode};
use wasm_encoder::ValType::I64;
use wasmparser::{KnownCustom, Name, Parser, Payload, TypeRef};

#[derive(Clone, Copy, PartialEq)]
enum Arg {
    Local0,
    TlsBase,

    Local1Replaced,

    MissingText,

    TxdLoaded,

    PoolGrow,
    CashWrite,
    OpusFrame,

    ResourceMap,
    /// First arg only: wraps the body; the host gets the function's i32 result (then the listed
    /// locals) and returns the value to use.
    ExitI32,
    /// Any local by index: an i64 (pointer / i64 param), or an i32 param sign-extended.
    L64(u32),
    L32(u32),
}

const HOOKS: &[(&str, &str, &[Arg])] = &[
    ("muguet_script", "GtaThread::Update(int)", &[Arg::Local0, Arg::TlsBase]),
    ("muguet_explosion", "CExplosionManager::AddExplosion(CExplosionManager::CExplosionArgs&, CProjectile*, bool)", &[Arg::Local0]),
    ("muguet_timewarp", "CTimeWarper::SetTargetTimeWarp(TW_Dir)", &[Arg::Local1Replaced]),
    ("muguet_text", "CTextFile::GetInternal(unsigned int)", &[Arg::MissingText]),
    ("muguet_txd", "graphics_commands::HasStreamedTxdLoaded(char const*)", &[Arg::TxdLoaded, Arg::TxdLoaded]),
    ("muguet_pool", "rage::sysMemGrowBuddyAllocator::GrowHeap()", &[Arg::PoolGrow]),
    ("muguet_cash", "stats_commands::CommandStatSetInt(int, int, bool)", &[Arg::CashWrite, Arg::CashWrite]),
    ("muguet_opus_frame", "opus_decode_frame", &[Arg::OpusFrame; 6]),
    ("muguet_resource_map", "rage::datResourceInfo::GenerateMap(rage::datResourceMap&) const", &[Arg::ResourceMap]),
    // the port's D3D11 layer: where mapped buffers live, and which ring id a resource uploads to
    ("muguet_gpu_upload", "wasm_null_d3d::GpuUploadBuffer(unsigned int, unsigned long, void const*, unsigned long, unsigned int)", &[Arg::L32(0), Arg::L64(1), Arg::L64(2), Arg::L64(3), Arg::L32(4)]),
    ("muguet_d3d_map", "wasm_null_d3d::NullContext::Map(ID3D11Resource*, unsigned int, D3D11_MAP, unsigned int, D3D11_MAPPED_SUBRESOURCE*)", &[Arg::ExitI32, Arg::L64(1), Arg::L32(2), Arg::L32(3), Arg::L64(5)]),
    ("muguet_d3d_unmap", "wasm_null_d3d::NullContext::Unmap(ID3D11Resource*, unsigned int)", &[Arg::L64(1), Arg::L32(2)]),
];

const SCENARIO_CHECK: &str = "CScenarioManager::PerformCollisionChecksForScenarioPoint(CScenarioManager::CollisionCheckParams const&)";
const MOVIE_PRELOAD: &str = "CMovieMgr::PreloadMovie(char const*, bool)";

pub const VERSION: u32 = 12;

fn returns(args: &[Arg]) -> bool {
    args.contains(&Arg::ExitI32) || args.contains(&Arg::Local1Replaced) || args.contains(&Arg::MissingText) || args.contains(&Arg::TxdLoaded) || args.contains(&Arg::PoolGrow) || args.contains(&Arg::CashWrite) || args.contains(&Arg::OpusFrame)
}

struct Patcher {
    imported_funcs: u32,
    types: u32,
    tls_global: u32,

    targets: HashMap<u32, usize>,
    scenario_check: Option<u32>,
    movie_preload: Option<u32>,
    pool_size: Option<u32>,
    exports: Vec<(String, u32)>,
    next_func: u32,
}

impl Patcher {
    fn shift(&self) -> u32 {
        HOOKS.len() as u32
    }
}

impl Reencode for Patcher {
    type Error = anyhow::Error;

    fn function_index(&mut self, f: u32) -> Result<u32, Error<Self::Error>> {
        Ok(if f >= self.imported_funcs { f + self.shift() } else { f })
    }

    fn parse_type_section(&mut self, types: &mut wasm_encoder::TypeSection, section: wasmparser::TypeSectionReader<'_>) -> Result<(), Error<Self::Error>> {
        utils::parse_type_section(self, types, section)?;

        for (_, _, args) in HOOKS {
            types.ty().function(vec![I64; args.len()], if returns(args) { vec![I64] } else { vec![] });
        }
        Ok(())
    }

    fn parse_import_section(&mut self, imports: &mut wasm_encoder::ImportSection, section: wasmparser::ImportSectionReader<'_>) -> Result<(), Error<Self::Error>> {
        utils::parse_import_section(self, imports, section)?;
        for (k, (name, _, _)) in HOOKS.iter().enumerate() {
            imports.import("env", name, wasm_encoder::EntityType::Function(self.types + k as u32));
        }
        Ok(())
    }

    fn parse_export_section(&mut self, exports: &mut wasm_encoder::ExportSection, section: wasmparser::ExportSectionReader<'_>) -> Result<(), Error<Self::Error>> {
        utils::parse_export_section(self, exports, section)?;
        for (name, f) in std::mem::take(&mut self.exports) {
            exports.export(&name, wasm_encoder::ExportKind::Func, f + self.shift());
        }
        Ok(())
    }

    fn parse_function_body(&mut self, code: &mut wasm_encoder::CodeSection, func: wasmparser::FunctionBody<'_>) -> Result<(), Error<Self::Error>> {
        let index = self.next_func;
        self.next_func += 1;
        if Some(index) == self.scenario_check {
            use wasm_encoder::Instruction as I;
            let mut f = self.new_function_with_parsed_locals(&func)?;
            let mut reader = func.get_operators_reader()?;
            while !reader.eof() {
                let i = self.parse_instruction(&mut reader)?;
                f.instruction(if matches!(i, I::I32Const(6)) { &I::I32Const(5) } else { &i });
            }
            code.function(&f);
            return Ok(());
        }
        if Some(index) == self.movie_preload {
            use wasm_encoder::Instruction as I;
            let mut f = self.new_function_with_parsed_locals(&func)?;
            f.instruction(&I::I32Const(0)).instruction(&I::Return);
            let mut reader = func.get_operators_reader()?;
            while !reader.eof() {
                f.instruction(&self.parse_instruction(&mut reader)?);
            }
            code.function(&f);
            return Ok(());
        }
        let Some(&hook) = self.targets.get(&index) else {
            return utils::parse_function_body(self, code, func);
        };
        let mut f = self.new_function_with_parsed_locals(&func)?;
        use wasm_encoder::Instruction as I;
        let call = I::Call(self.imported_funcs + hook as u32);
        if HOOKS[hook].2.first() == Some(&Arg::ExitI32) {
            // block (result i32) { body, with `return` as a branch out of the block } -> host(result, locals...) -> return
            f.instruction(&I::Block(wasm_encoder::BlockType::Result(wasm_encoder::ValType::I32)));
            let mut depth = 0u32;
            let mut reader = func.get_operators_reader()?;
            while !reader.eof() {
                let i = self.parse_instruction(&mut reader)?;
                match &i {
                    I::Block(_) | I::Loop(_) | I::If(_) => { depth += 1; f.instruction(&i); }
                    I::End if depth == 0 => {
                        f.instruction(&I::End);
                        f.instruction(&I::I64ExtendI32U);
                        for arg in &HOOKS[hook].2[1..] {
                            match arg {
                                Arg::L64(k) => { f.instruction(&I::LocalGet(*k)); }
                                Arg::L32(k) => { f.instruction(&I::LocalGet(*k)).instruction(&I::I64ExtendI32S); }
                                _ => unreachable!(),
                            }
                        }
                        f.instruction(&call).instruction(&I::I32WrapI64);
                        f.instruction(&I::End);
                    }
                    I::End => { depth -= 1; f.instruction(&i); }
                    I::Return => { f.instruction(&I::Br(depth)); }
                    _ => { f.instruction(&i); }
                }
            }
            code.function(&f);
            return Ok(());
        }
        if HOOKS[hook].2 == [Arg::ResourceMap] {
            let mut reader = func.get_operators_reader()?;
            let mut body = Vec::new();
            while !reader.eof() {
                body.push(self.parse_instruction(&mut reader)?);
            }
            let end = body.pop();
            for i in &body {
                f.instruction(i);
            }
            f.instruction(&I::LocalGet(1)).instruction(&call);
            if let Some(e) = end {
                f.instruction(&e);
            }
            code.function(&f);
            return Ok(());
        }
        if HOOKS[hook].2 == [Arg::PoolGrow] {
            let Some(size) = self.pool_size else { return Err(Error::UserError(anyhow::anyhow!("GetHeapSize not found"))) };
            f.instruction(&I::LocalGet(0)).instruction(&I::Call(size + self.shift())).instruction(&call).instruction(&I::I64Eqz);
            f.instruction(&I::If(wasm_encoder::BlockType::Empty));
            f.instruction(&I::I32Const(-1)).instruction(&I::Return);
            f.instruction(&I::End);
        } else if HOOKS[hook].2.contains(&Arg::OpusFrame) {
            for (k, wide) in [true, true, false, true, false, false].into_iter().enumerate() {
                f.instruction(&I::LocalGet(k as u32));
                if !wide { f.instruction(&I::I64ExtendI32S); }
            }
            f.instruction(&call).instruction(&I::I32WrapI64).instruction(&I::Return);
        } else if HOOKS[hook].2 == [Arg::MissingText] {
            // only call out for the listed hashes: text lookups are hot
            for (label, _) in crate::host::MISSING_TEXT {
                f.instruction(&I::LocalGet(0)).instruction(&I::I32Const(crate::proto::joaat(label) as i32)).instruction(&I::I32Eq);
                f.instruction(&I::If(wasm_encoder::BlockType::Empty));
                f.instruction(&I::LocalGet(0)).instruction(&I::I64ExtendI32U).instruction(&call).instruction(&I::Return);
                f.instruction(&I::End);
            }
        } else if HOOKS[hook].2.first() == Some(&Arg::CashWrite) {
            for k in 0..3 {
                f.instruction(&I::LocalGet(0)).instruction(&I::I32Const(crate::proto::joaat(&format!("SP{k}_TOTAL_CASH")) as i32)).instruction(&I::I32Eq);
                f.instruction(&I::If(wasm_encoder::BlockType::Empty));
                f.instruction(&I::LocalGet(1)).instruction(&I::I64ExtendI32S).instruction(&I::GlobalGet(self.tls_global)).instruction(&call).instruction(&I::I64Eqz).instruction(&I::I32Eqz);
                f.instruction(&I::If(wasm_encoder::BlockType::Empty)).instruction(&I::I32Const(1)).instruction(&I::Return).instruction(&I::End);
                f.instruction(&I::End);
            }
        } else if HOOKS[hook].2.first() == Some(&Arg::TxdLoaded) {
            f.instruction(&I::LocalGet(0)).instruction(&I::I64Const(0)).instruction(&call).instruction(&I::Drop);
            let mut reader = func.get_operators_reader()?;
            let mut last = None;
            while !reader.eof() {
                let i = self.parse_instruction(&mut reader)?;
                if let (I::Return, Some(I::LocalGet(2))) = (&i, &last) {
                    f.instruction(&I::Drop);
                    f.instruction(&I::I64Const(0)).instruction(&I::LocalGet(2)).instruction(&I::I64ExtendI32U).instruction(&I::I64Const(1)).instruction(&I::I64Add);
                    f.instruction(&call).instruction(&I::I32WrapI64);
                }
                f.instruction(&i);
                last = Some(i);
            }
            code.function(&f);
            return Ok(());
        } else {
            for arg in HOOKS[hook].2 {
                match arg {
                    Arg::Local0 => f.instruction(&I::LocalGet(0)),
                    Arg::TlsBase => f.instruction(&I::GlobalGet(self.tls_global)),
                    Arg::Local1Replaced => f.instruction(&I::LocalGet(1)).instruction(&I::I64ExtendI32U),
                    Arg::L64(k) => f.instruction(&I::LocalGet(*k)),
                    Arg::L32(k) => f.instruction(&I::LocalGet(*k)).instruction(&I::I64ExtendI32S),
                    Arg::ExitI32 | Arg::MissingText | Arg::TxdLoaded | Arg::PoolGrow | Arg::CashWrite | Arg::OpusFrame | Arg::ResourceMap => unreachable!(),
                };
            }
            f.instruction(&call);
            if returns(HOOKS[hook].2) {
                f.instruction(&I::I32WrapI64);
                f.instruction(&I::LocalSet(1));
            }
        }
        let mut reader = func.get_operators_reader()?;
        while !reader.eof() {
            f.instruction(&self.parse_instruction(&mut reader)?);
        }
        code.function(&f);
        Ok(())
    }
}

pub fn is_patched(wasm: &[u8]) -> bool {
    Parser::new(0).parse_all(wasm).any(|p| match p {
        Ok(Payload::ImportSection(s)) => s.into_imports().any(|i| i.is_ok_and(|i| i.name == HOOKS[0].0)),
        _ => false,
    })
}

pub fn patch(wasm: &[u8], wanted: &[(&str, &str)]) -> Result<Vec<u8>> {
    let (mut imported_funcs, mut types, mut tls_global) = (0u32, 0u32, None);
    let mut by_name: HashMap<String, u32> = HashMap::new();
    for payload in Parser::new(0).parse_all(wasm) {
        match payload? {
            Payload::TypeSection(s) => types = s.count(),
            Payload::ImportSection(s) => {
                for imp in s.into_imports() {
                    if matches!(imp?.ty, TypeRef::Func(_)) {
                        imported_funcs += 1;
                    }
                }
            }
            Payload::CustomSection(c) => {
                if let KnownCustom::Name(names) = c.as_known() {
                    for sub in names {
                        match sub? {
                            Name::Function(map) => {
                                for n in map {
                                    let n = n?;
                                    by_name.entry(n.name.to_string()).or_insert(n.index);
                                }
                            }
                            Name::Global(map) => {
                                for n in map {
                                    let n = n?;
                                    if n.name == "__tls_base" { tls_global = Some(n.index); }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if by_name.contains_key(HOOKS[0].0) {
        bail!("game.wasm is already patched");
    }
    let mut targets = HashMap::new();
    for (k, (_, func, _)) in HOOKS.iter().enumerate() {
        let f = *by_name.get(*func).with_context(|| format!("{func} not found (no name section?)"))?;
        targets.insert(f, k);
    }
    let mut exports = Vec::new();
    if let Some(&malloc) = by_name.get("emscripten_builtin_malloc") {
        exports.push(("muguet:malloc".to_string(), malloc));
    }
    for (export, func) in wanted {
        match by_name.get(*func) {
            Some(&i) => exports.push((format!("mp:{export}"), i)),
            None => eprintln!("[patch] {func} not found; multiplayer will skip it"),
        }
    }
    let mut p = Patcher {
        imported_funcs,
        types,
        tls_global: tls_global.unwrap_or(1),
        targets,
        scenario_check: by_name.get(SCENARIO_CHECK).copied(),
        movie_preload: by_name.get(MOVIE_PRELOAD).copied(),
        pool_size: by_name.get("rage::sysMemGrowBuddyAllocator::GetHeapSize() const").copied(),
        exports,
        next_func: imported_funcs,
    };
    let mut module = wasm_encoder::Module::new();
    p.parse_core_module(&mut module, Parser::new(0), wasm).map_err(|e| anyhow::anyhow!("patch game.wasm: {e:?}"))?;
    Ok(module.finish())
}
