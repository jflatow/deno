// Copyright 2018-2023 the Deno authors. All rights reserved. MIT license.

use deno_core::error::AnyError;
use deno_core::futures::channel::mpsc;
use deno_core::OpState;

use std::cell::RefCell;
use std::mem::size_of;
use std::os::raw::c_char;
use std::os::raw::c_short;
use std::path::Path;
use std::rc::Rc;

mod call;
mod callback;
mod dlfcn;
mod ir;
mod repr;
mod r#static;
mod symbol;
mod turbocall;

use call::op_ffi_call_nonblocking;
use call::op_ffi_call_ptr;
use call::op_ffi_call_ptr_nonblocking;
use callback::op_ffi_unsafe_callback_close;
use callback::op_ffi_unsafe_callback_create;
use callback::op_ffi_unsafe_callback_ref;
use dlfcn::op_ffi_load;
use dlfcn::ForeignFunction;
use r#static::op_ffi_get_static;
use repr::*;
use symbol::NativeType;
use symbol::Symbol;

#[cfg(not(target_pointer_width = "64"))]
compile_error!("platform not supported");

const _: () = {
  assert!(size_of::<c_char>() == 1);
  assert!(size_of::<c_short>() == 2);
  assert!(size_of::<*const ()>() == 8);
};

pub(crate) const MAX_SAFE_INTEGER: isize = 9007199254740991;
pub(crate) const MIN_SAFE_INTEGER: isize = -9007199254740991;

fn check_unstable(state: &OpState, api_name: &str) {
  state
    .feature_checker
    .check_legacy_unstable_or_exit(api_name);
}

pub trait FfiPermissions {
  fn check_partial(&mut self, path: Option<&Path>) -> Result<(), AnyError>;
}

pub(crate) type PendingFfiAsyncWork = Box<dyn FnOnce()>;

pub(crate) struct FfiState {
  pub(crate) async_work_sender: mpsc::UnboundedSender<PendingFfiAsyncWork>,
  pub(crate) async_work_receiver: mpsc::UnboundedReceiver<PendingFfiAsyncWork>,
}

deno_core::extension!(deno_ffi,
  deps = [ deno_web ],
  parameters = [P: FfiPermissions],
  ops = [
    op_ffi_load<P>,
    op_ffi_get_static,
    op_ffi_call_nonblocking,
    op_ffi_call_ptr<P>,
    op_ffi_call_ptr_nonblocking<P>,
    op_ffi_ptr_create<P>,
    op_ffi_ptr_equals<P>,
    op_ffi_ptr_of<P>,
    op_ffi_ptr_offset<P>,
    op_ffi_ptr_value<P>,
    op_ffi_get_buf<P>,
    op_ffi_buf_copy_into<P>,
    op_ffi_cstr_read<P>,
    op_ffi_read_bool<P>,
    op_ffi_read_u8<P>,
    op_ffi_read_i8<P>,
    op_ffi_read_u16<P>,
    op_ffi_read_i16<P>,
    op_ffi_read_u32<P>,
    op_ffi_read_i32<P>,
    op_ffi_read_u64<P>,
    op_ffi_read_i64<P>,
    op_ffi_read_f32<P>,
    op_ffi_read_f64<P>,
    op_ffi_read_ptr<P>,
    op_ffi_unsafe_callback_create<P>,
    op_ffi_unsafe_callback_close,
    op_ffi_unsafe_callback_ref,
  ],
  esm = [ "00_ffi.js" ],
  event_loop_middleware = event_loop_middleware,
);

fn event_loop_middleware(
  op_state_rc: Rc<RefCell<OpState>>,
  _cx: &mut std::task::Context,
) -> bool {
  // FFI callbacks coming in from other threads will call in and get queued.
  let mut maybe_scheduling = false;

  let mut op_state = op_state_rc.borrow_mut();
  if let Some(ffi_state) = op_state.try_borrow_mut::<FfiState>() {
    // TODO(mmastrac): This should be a SmallVec to avoid allocations in most cases
    let mut work_items = Vec::with_capacity(1);

    while let Ok(Some(async_work_fut)) =
      ffi_state.async_work_receiver.try_next()
    {
      // Move received items to a temporary vector so that we can drop the `op_state` borrow before we do the work.
      work_items.push(async_work_fut);
      maybe_scheduling = true;
    }

    // Drop the op_state and ffi_state borrows
    drop(op_state);
    for async_work_fut in work_items.into_iter() {
      async_work_fut();
    }
  }

  maybe_scheduling
}
