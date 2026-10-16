// muguet - host.rs
// Constantin Clerc (c22dev) - 0.1

use crate::{
    default_val, fs, io, read_cstr, read_u32, read_u64, spawn_pthread, write_bytes, write_u32,
    write_u64, HostState, RT,
};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use wasmtime::{Caller, SharedMemory, Val};

#[derive(Debug)]
pub struct Unwind;
impl std::fmt::Display for Unwind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("unwind") }
}
impl std::error::Error for Unwind {}

#[derive(Debug)]
pub struct LongJmp;
impl std::fmt::Display for LongJmp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("longjmp") }
}
impl std::error::Error for LongJmp {}

static PENDING_MAILBOX: LazyLock<Mutex<HashSet<u64>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static ANY_PENDING: AtomicU64 = AtomicU64::new(0);
static STUB_SEQ: AtomicU64 = AtomicU64::new(0);

static STOPPED: AtomicBool = AtomicBool::new(false);
static FATAL: Mutex<Option<String>> = Mutex::new(None);

pub fn engine_stopped() -> bool {
    !STOPPED.swap(true, Ordering::Relaxed)
}

pub fn stop_reason() -> Option<String> {
    STOPPED.load(Ordering::Relaxed).then(|| FATAL.lock().unwrap().clone().unwrap_or_else(|| "A game thread stopped with an error.".into()))
}

thread_local! {
    static PENDING_LONGJMP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static PENDING_UNWIND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub fn take_longjmp() -> bool { PENDING_LONGJMP.with(|c| c.replace(false)) }
pub fn take_unwind() -> bool { PENDING_UNWIND.with(|c| c.replace(false)) }

fn int(p: &[Val], i: usize) -> i64 {
    match p.get(i) { Some(Val::I32(x)) => *x as i64, Some(Val::I64(x)) => *x, _ => 0 }
}
fn set_int(results: &mut [Val], v: i64) {
    if let Some(r) = results.get_mut(0) {
        *r = match r { Val::I64(_) => Val::I64(v), Val::F64(_) => Val::F64((v as f64).to_bits()), _ => Val::I32(v as i32) };
    }
}
fn set_f64(results: &mut [Val], v: f64) {
    if let Some(r) = results.get_mut(0) {
        *r = match r { Val::I64(_) => Val::I64(v as i64), _ => Val::F64(v.to_bits()) };
    }
}
fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

fn export_call(caller: &mut Caller<'_, HostState>, name: &str, args: &[Val]) -> Result<Vec<Val>> {
    let f = caller
        .get_export(name)
        .and_then(|e| e.into_func())
        .ok_or_else(|| anyhow::anyhow!("no wasm export {name}"))?;
    let ty = f.ty(&mut *caller);
    let args = crate::coerce_args(args, &ty);
    let mut results: Vec<Val> = ty.results().map(|t| default_val(&t)).collect();
    f.call(&mut *caller, &args, &mut results)?;
    Ok(results)
}

// labels the scripts print that no loaded text block has (the copy in arm1.gxt2 only loads during that mission)
pub const MISSING_TEXT: &[(&str, &str)] = &[
    ("HELP_DCK", "~s~Hold ~INPUT_VEH_DUCK~ to duck when sitting inside a vehicle."),
    ("HELP_KM", "~s~Press ~INPUT_CONTEXT~ to use the camera in your glasses, then ~INPUT_ATTACK~ to take a photo."),
];

fn missing_text(caller: &mut Caller<'_, HostState>, hash: u32) -> Result<u64> {
    static TEXT: LazyLock<Mutex<HashMap<u32, u64>>> = LazyLock::new(Default::default);
    let mut text = TEXT.lock().unwrap();
    if let Some(&p) = text.get(&hash) {
        return Ok(p);
    }
    let Some((_, s)) = MISSING_TEXT.iter().find(|(l, _)| crate::proto::joaat(l) == hash) else { return Ok(0) };
    let p = match export_call(caller, "muguet:malloc", &[Val::I64(s.len() as i64 + 1)])?.first() { Some(Val::I64(p)) => *p as u64, Some(Val::I32(p)) => *p as u32 as u64, _ => 0 };
    if p != 0 {
        write_bytes(&RT.get().expect("runtime").mem, p, &[s.as_bytes(), &[0]].concat());
        text.insert(hash, p);
    }
    Ok(p)
}

pub fn mailbox_await(mem: &SharedMemory, pthread_ptr: u64) {
    write_u32(mem, pthread_ptr + 204, 1);
}

pub fn call(mut caller: Caller<'_, HostState>, idx: usize, params: &[Val], results: &mut [Val]) -> Result<()> {
    let rt = RT.get().expect("runtime");
    let n = rt.counts[idx].fetch_add(1, Ordering::Relaxed) + 1;
    let name = rt.imports[idx].name.as_str();
    let mem = &rt.mem;

    for (r, t) in results.iter_mut().zip(rt.imports[idx].ty.results()) {
        *r = default_val(&t);
    }

    if ANY_PENDING.load(Ordering::Relaxed) != 0 && !name.contains("mailbox") {
        let me = caller.data().pthread;
        let has = me != 0 && PENDING_MAILBOX.lock().unwrap().remove(&me);
        if has {
            ANY_PENDING.fetch_sub(1, Ordering::Relaxed);
            mailbox_await(mem, me);
            export_call(&mut caller, "_emscripten_check_mailbox", &[])?;
        }
    }

    if name.starts_with("invoke_") {
        return invoke(&mut caller, params, results);
    }

    match name {
        "_emscripten_init_main_thread_js" => {
            let tb = int(params, 0);
            caller.data_mut().pthread = tb as u64;

            export_call(&mut caller, "_emscripten_thread_init", &[Val::I64(tb), Val::I32(1), Val::I32(1), Val::I32(1), Val::I64(1_048_576), Val::I32(0)])?;
        }
        "muguet_script" => crate::mp::script_tick(&mut caller, int(params, 0) as u64, int(params, 1) as u64)?,
        "muguet_gpu_upload" => crate::gpu::d3d_upload(int(params, 0) as u32, int(params, 1) as u64, int(params, 2) as u64, int(params, 3) as u64),
        "muguet_d3d_map" => { let r = int(params, 0); crate::gpu::d3d_map(mem, int(params, 1) as u64, int(params, 3) as u32, int(params, 4) as u64); set_int(results, r) }
        "muguet_d3d_unmap" => crate::gpu::d3d_unmap(int(params, 0) as u64),
        "muguet_explosion" => crate::mp::explosion(int(params, 0) as u64),
        "muguet_timewarp" => set_int(results, crate::mp::time_warp(int(params, 0))),
        "muguet_resource_map" => merge_physical_pages(mem, int(params, 0) as u64),
        "muguet_pool" => set_int(results, pool_grow(int(params, 0) as u64) as i64),
        "muguet_cash" => set_int(results, crate::mp::cash_write(int(params, 0), int(params, 1) as u64) as i64),
        "muguet_txd" => set_int(results, crate::mp::txd_loaded(int(params, 0) as u64, int(params, 1))),
        "muguet_opus_frame" => {
            let (base, len) = crate::mem_base_len(mem);
            let heap = unsafe { std::slice::from_raw_parts_mut(base, len) };
            let p = |i| int(params, i);
            set_int(results, crate::opus::decode_frame(heap, p(0) as u64, p(1) as u64, p(2) as i32, p(3) as u64, p(4) as i32, p(5) as i32))
        }
        "muguet_text" => set_int(results, missing_text(&mut caller, int(params, 0) as u32)? as i64),
        "__pthread_create_js" => {
            let r = spawn_pthread(int(params, 0) as u64, int(params, 2) as u64, int(params, 3) as u64);
            set_int(results, r as i64);
        }
        "_emscripten_thread_mailbox_await" => mailbox_await(mem, int(params, 0) as u64),
        "_emscripten_notify_mailbox_postmessage" => {
            if PENDING_MAILBOX.lock().unwrap().insert(int(params, 0) as u64) {
                ANY_PENDING.fetch_add(1, Ordering::Relaxed);
            }
        }
        "_emscripten_thread_cleanup" => {
            export_call(&mut caller, "_emscripten_thread_free_data", &[Val::I64(int(params, 0))])?;
        }
        "_emscripten_thread_set_strongref" | "emscripten_check_blocking_allowed" => {}
        "emscripten_exit_with_live_runtime" => { PENDING_UNWIND.with(|c| c.set(true)); return Err(Unwind.into()); }
        "_emscripten_throw_longjmp" => { PENDING_LONGJMP.with(|c| c.set(true)); return Err(LongJmp.into()); }
        "exit" | "proc_exit" => {
            eprintln!("[host] engine exit({})", int(params, 0));
            std::process::exit(int(params, 0) as i32);
        }
        "_abort_js" => anyhow::bail!("engine called abort()"),
        "emscripten_num_logical_cores" => set_int(results, std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(8)),
        "emscripten_get_heap_max" => set_int(results, 17_179_869_184),
        "emscripten_resize_heap" => set_int(results, resize_heap(mem, int(params, 0) as u64) as i64),
        "_emscripten_system" => set_int(results, if int(params, 0) == 0 { 0 } else { -52 }),
        "emscripten_stack_snapshot" | "emscripten_stack_unwind_buffer" => set_int(results, 0),
        "emscripten_asm_const_int" | "emscripten_asm_const_int_sync_on_main_thread" => {
            let r = em_asm(mem, int(params, 0) as u64, int(params, 1) as u64, int(params, 2) as u64);
            set_int(results, r);
        }

        "emscripten_get_now" | "wasm_now_ms" | "emscripten_date_now" => set_f64(results, now_ms()),
        "clock_time_get" => {
            write_u64(mem, int(params, 2) as u64, (now_ms() * 1e6) as u64);
            set_int(results, 0);
        }
        "_tzset_js" => {
            write_u64(mem, int(params, 0) as u64, 0);
            write_u32(mem, int(params, 1) as u64, 0);
            write_bytes(mem, int(params, 2) as u64, b"UTC\0");
            write_bytes(mem, int(params, 3) as u64, b"UTC\0");
        }
        "_gmtime_js" | "_localtime_js" => {
            write_tm(mem, int(params, 0), int(params, 1) as u64);
            set_int(results, 0);
        }
        "_mktime_js" => set_int(results, mktime_utc(mem, int(params, 0) as u64)),

        "environ_sizes_get" => {
            write_u64(mem, int(params, 0) as u64, 0);
            write_u64(mem, int(params, 1) as u64, 0);
            set_int(results, 0);
        }
        "environ_get" => set_int(results, 0),

        "wasm_print_line_js" | "wasm_hang_line_js" => {
            let line = read_cstr(mem, int(params, 0) as u64);
            if let Some(at) = line.find("Fatal Error: ") {
                FATAL.lock().unwrap().get_or_insert_with(|| line[at + 13..].to_string());
            }
            eprintln!("[eng] {line}");
        }
        "wasm_module_int_js" => set_int(results, int(params, 1)),
        "wasm_has_page_js" => set_int(results, 1),
        "wasm_userdata_page_js" => set_int(results, 0),
        "wasm_userdata_put_js" | "wasm_userdata_delete_js" => {}
        "wasm_httpfs_manifest_js" => fs::httpfs_manifest(mem, params, results),
        "wasm_httpfs_io_start_js" => io::start(mem, int(params, 0) as u64),
        "wgpu_start_worker" => crate::gpu::start(mem, int(params, 0) as u64, int(params, 1) as u64, int(params, 2) as u64),
        "wasm_input_publish_js" => crate::input::publish(int(params, 0) as u64),
        "wasm_audio_publish_js" => crate::audio::start(int(params, 0) as u64, int(params, 1) as u32),
        "fd_write" if int(params, 0) <= 2 => {
            let (iovs, count) = (int(params, 1) as u64, int(params, 2) as u64);
            let mut buf = Vec::new();
            for k in 0..count {
                let p = read_u64(mem, iovs + k * 16).unwrap_or(0);
                let l = read_u64(mem, iovs + k * 16 + 8).unwrap_or(0);
                buf.extend(crate::read_bytes(mem, p, l));
            }
            write_u64(mem, int(params, 3) as u64, buf.len() as u64);
            let text = String::from_utf8_lossy(&buf);
            for line in text.lines() {
                eprintln!("[eng:{}] {line}", int(params, 0));
            }
            set_int(results, 0);
        }
        _ if fs::handle(name, mem, params, results) => {}

        _ => {
            if name.starts_with("__syscall_") { set_int(results, -52); }
            if n <= 3 {
                let seq = STUB_SEQ.fetch_add(1, Ordering::Relaxed);
                eprintln!("[stub #{seq:05} +{:.3}s] {name}({})", rt.start.elapsed().as_secs_f64(),
                    params.iter().map(|v| format!("{}", int(std::slice::from_ref(v), 0))).collect::<Vec<_>>().join(", "));
            }
        }
    }
    Ok(())
}

fn invoke(caller: &mut Caller<'_, HostState>, params: &[Val], results: &mut [Val]) -> Result<()> {
    let index = int(params, 0) as u64;
    let table = caller
        .get_export("__indirect_function_table")
        .and_then(|e| e.into_table())
        .ok_or_else(|| anyhow::anyhow!("no function table"))?;
    let f = table
        .get(&mut *caller, index)
        .and_then(|r| r.as_func().flatten().cloned())
        .ok_or_else(|| anyhow::anyhow!("invoke: null function index {index}"))?;
    let sp = export_call(caller, "emscripten_stack_get_current", &[])?;
    match f.call(&mut *caller, &params[1..], results) {
        Ok(()) => Ok(()),
        Err(_) if take_longjmp() => {
            export_call(caller, "_emscripten_stack_restore", &sp)?;
            export_call(caller, "setThrew", &[Val::I64(1), Val::I32(0)])?;
            for r in results.iter_mut() { *r = default_val(&r.ty(&*caller).unwrap_or(wasmtime::ValType::I32)); }
            Ok(())
        }
        Err(e) => Err(anyhow::Error::from(e)),
    }
}

const STEP: u64 = 256 << 20;

fn merge_physical_pages(mem: &SharedMemory, map: u64) {
    let virt = crate::read_bytes(mem, map, 1)[0] as u64;
    let entry = |k: u64| map + 16 + 24 * k;
    let q = |a: u64| read_u64(mem, a).unwrap_or(0);
    'merge: loop {
        let phys = crate::read_bytes(mem, map + 1, 1)[0] as u64;
        for k in virt..(virt + phys).saturating_sub(1) {
            let (a, b) = (entry(k), entry(k + 1));
            let size = q(a + 16);
            if size == 0 || q(b + 16) != size || q(b) != q(a) + size {
                continue;
            }
            write_u64(mem, a + 16, size * 2);
            let tail = virt + phys - (k + 2);
            let moved = crate::read_bytes(mem, entry(k + 2), 24 * tail);
            write_bytes(mem, b, &moved);
            write_bytes(mem, entry(virt + phys - 1), &[0; 24]);
            write_bytes(mem, map + 1, &[(phys - 1) as u8]);
            continue 'merge;
        }
        return;
    }
}

fn pool_grow(size: u64) -> bool {
    static CAP: LazyLock<u64> = LazyLock::new(|| {
        let mb = std::env::var("MUGUET_POOL_CAP_MB").ok().and_then(|v| v.parse::<u64>().ok());
        mb.map(|mb| mb << 20).unwrap_or(if cfg!(target_os = "ios") { 2560 << 20 } else { u64::MAX })
    });
    let ok = size.saturating_add(STEP) <= CAP.saturating_add(16 << 20);
    static TOLD: AtomicBool = AtomicBool::new(false);
    if !ok && !TOLD.swap(true, Ordering::Relaxed) {
        eprintln!("[host] streaming pool at {} MB, not growing it past the {} MB cap", size >> 20, *CAP >> 20);
    }
    ok
}

fn resize_heap(mem: &SharedMemory, requested: u64) -> bool {
    const MAX: u64 = 17_179_869_184;
    let old = mem.data_size() as u64;
    if requested <= old { return false; }
    if requested > MAX { return false; }
    let mut cut = 1u64;
    while cut <= 4 {
        let over = (old as f64 * (1.0 + 0.2 / cut as f64)) as u64;
        let over = over.min(requested + 100_663_296);
        let new = MAX.min(requested.max(over).div_ceil(65536) * 65536);
        let pages = (new - old).div_ceil(65536);
        if mem.grow(pages).is_ok() {
            eprintln!("[host] heap grown to {} MB", mem.data_size() >> 20);
            return true;
        }
        cut *= 2;
    }
    false
}

fn em_asm(mem: &SharedMemory, code: u64, sig: u64, buf: u64) -> i64 {
    let mut args = Vec::new();
    let (mut s, mut b) = (sig, buf);
    loop {
        let ch = crate::read_bytes(mem, s, 1).first().copied().unwrap_or(0);
        if ch == 0 { break; }
        s += 1;
        let wide = ch != b'i';
        if wide && b % 8 != 0 { b += 4; }
        args.push(if ch == b'i' { read_u32(mem, b).unwrap_or(0) as i32 as i64 } else { read_u64(mem, b).unwrap_or(0) as i64 });
        b += if wide { 8 } else { 4 };
    }
    match code {
        7980558 => {
            let (dst, max) = (args.first().copied().unwrap_or(0) as u64, args.get(1).copied().unwrap_or(0) as usize);
            if max > 0 {
                let lang = b"en-US";
                let n = lang.len().min(max - 1);
                write_bytes(mem, dst, &lang[..n]);
                write_bytes(mem, dst + n as u64, &[0]);
            }
            0
        }

        7980673 => {
            eprintln!("[host] engine wants to open URL {}", read_cstr(mem, args.first().copied().unwrap_or(0) as u64));
            0
        }

        7980768 => {
            eprintln!("[host] engine requested a reload");
            0
        }
        _ => {
            eprintln!("[host] unknown EM_ASM {code}");
            0
        }
    }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn write_tm(mem: &SharedMemory, t: i64, tm: u64) {
    let days = t.div_euclid(86400);
    let secs = t.rem_euclid(86400);

    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 { y += 1; }
    let yday = days - days_from_civil(y, 1, 1);
    let wday = (days + 4).rem_euclid(7);
    let fields = [secs % 60, (secs / 60) % 60, secs / 3600, d, m - 1, y - 1900, wday, yday, 0];
    for (i, v) in fields.iter().enumerate() {
        write_u32(mem, tm + 4 * i as u64, *v as i32 as u32);
    }
    write_u64(mem, tm + 40, 0);
}

fn mktime_utc(mem: &SharedMemory, tm: u64) -> i64 {
    let f = |i: u64| read_u32(mem, tm + 4 * i).unwrap_or(0) as i32 as i64;
    let (sec, min, hour, mday, mon, year) = (f(0), f(1), f(2), f(3), f(4), f(5));
    let y = 1900 + year + mon.div_euclid(12);
    let m = mon.rem_euclid(12) + 1;
    days_from_civil(y, m, 1) * 86400 + (mday - 1) * 86400 + hour * 3600 + min * 60 + sec
}
