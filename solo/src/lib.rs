// iac-solo: the single-player host. One world of `iac-sim` with its bots
// and one commander, behind a door of bytes (`Machine::call`, see
// `machine.rs`), so it runs wherever bytes can be passed: natively in the
// tests, and as WebAssembly in a browser worker (`web/worker.js`), where
// the page talks to it in the server's wire protocol with no server.
//
// No rules live here. The crate adds what a host adds: a session, a clock
// fed by the caller and the catch-up after an absence. Its save is the
// document every host stores (`iac_sim::store`), with its commander and its
// clock as the host's part; the bytes go out and come in through the door,
// and whoever calls it keeps them.
//
// The WebAssembly boundary is three exports and one import, no bindings
// generator: the caller sizes the request buffer and writes into it, calls,
// and reads the reply buffer.
//
//   iac_request(len) -> ptr     a buffer of `len` bytes for the request
//   iac_call() -> len           run it; the reply is `len` bytes
//   iac_reply() -> ptr          where the reply is, until the next call
//   env.iac_panic(ptr, len)     imported: the message of a panic, before
//                               the trap that ends the instance

mod away;
mod machine;
mod world;

use std::cell::RefCell;

pub use machine::Machine;
pub use world::CATCH_UP_LIMIT;

#[derive(Default)]
struct Port {
    machine: Machine,
    request: Vec<u8>,
    reply: Vec<u8>,
}

thread_local! {
    static PORT: RefCell<Port> = RefCell::default();
}

#[unsafe(no_mangle)]
pub extern "C" fn iac_request(len: usize) -> *mut u8 {
    report_panics();
    PORT.with_borrow_mut(|port| {
        port.request.clear();
        port.request.resize(len, 0);
        port.request.as_mut_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn iac_call() -> usize {
    PORT.with_borrow_mut(|port| {
        port.reply = port.machine.call(&port.request);
        port.reply.len()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn iac_reply() -> *const u8 {
    PORT.with_borrow(|port| port.reply.as_ptr())
}

#[cfg(target_arch = "wasm32")]
fn report_panics() {
    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn iac_panic(ptr: *const u8, len: usize);
    }
    static HOOKED: std::sync::Once = std::sync::Once::new();
    HOOKED.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let message = info.to_string();
            unsafe { iac_panic(message.as_ptr(), message.len()) }
        }));
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn report_panics() {}
