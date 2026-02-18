/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

#ifndef _GNU_SOURCE
#define _GNU_SOURCE // For RTLD_DEFAULT
#endif

#define HAS_DLOPEN_CALLBACK 0
#define HAS_DLSYM_CALLBACK 0
#define NO_DLOPEN 0
#define LAZY_LOAD 1
#define THREAD_SAFE 1

#include <dlfcn.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <assert.h>

#if THREAD_SAFE
#include <pthread.h>
#endif

// Sanity check for ARM to avoid puzzling runtime crashes
#ifdef __arm__
# if defined __thumb__ && ! defined __THUMB_INTERWORK__
#   error "ARM trampolines need -mthumb-interwork to work in Thumb mode"
# endif
#endif

#ifdef __cplusplus
extern "C" {
#endif

#define CHECK(cond, fmt, ...) do { \
    if(!(cond)) { \
      fprintf(stderr, "implib-gen: librockchip_mpp.so: " fmt "\n", ##__VA_ARGS__); \
      assert(0 && "Assertion in generated code"); \
      abort(); \
    } \
  } while(0)

static void *lib_handle;
static int dlopened;

#if ! NO_DLOPEN

#if THREAD_SAFE

static pthread_mutex_t mtx;
static int rec_count;

static void init_lock(void) {
  pthread_mutexattr_t attr;
  CHECK(0 == pthread_mutexattr_init(&attr), "failed to init mutex");
  CHECK(0 == pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_RECURSIVE), "failed to init mutex");
  CHECK(0 == pthread_mutex_init(&mtx, &attr), "failed to init mutex");
}

static int lock(void) {
  static pthread_once_t once = PTHREAD_ONCE_INIT;
  CHECK(0 == pthread_once(&once, init_lock), "failed to init lock");
  CHECK(0 == pthread_mutex_lock(&mtx), "failed to lock mutex");
  return 0 == __sync_fetch_and_add(&rec_count, 1);
}

static void unlock(void) {
  __sync_fetch_and_add(&rec_count, -1);
  CHECK(0 == pthread_mutex_unlock(&mtx), "failed to unlock mutex");
}
#else
static int lock(void) {
  return 1;
}
static void unlock(void) {}
#endif

static int load_library(void) {
  int publish = lock();

  if (lib_handle) {
    unlock();
    return publish;
  }

  lib_handle = dlopen("librockchip_mpp.so", RTLD_LAZY | RTLD_GLOBAL);
  CHECK(lib_handle, "failed to load library 'librockchip_mpp.so' via dlopen: %s", dlerror());

  if (__sync_val_compare_and_swap(&dlopened, 0, 1)) {
    dlclose(lib_handle);
  }

  unlock();

  return publish;
}

static void __attribute__((destructor(101))) unload_lib(void) {
  if (dlopened) {
    dlclose(lib_handle);
    lib_handle = 0;
    dlopened = 0;
  }
}
#endif

#if ! NO_DLOPEN && ! LAZY_LOAD
static void __attribute__((constructor(101))) load_lib(void) {
  load_library();
}
#endif

// MPP API symbols used by the encoder
static const char *const sym_names[] = {
  "mpp_create",
  "mpp_init",
  "mpp_destroy",
  "mpp_check_support_format",
  "mpp_frame_init",
  "mpp_frame_deinit",
  "mpp_frame_set_width",
  "mpp_frame_set_height",
  "mpp_frame_set_hor_stride",
  "mpp_frame_set_ver_stride",
  "mpp_frame_set_fmt",
  "mpp_frame_set_buffer",
  "mpp_frame_set_eos",
  "mpp_frame_get_meta",
  "mpp_packet_init_with_buffer",
  "mpp_packet_deinit",
  "mpp_packet_set_length",
  "mpp_packet_get_pos",
  "mpp_packet_get_length",
  "mpp_enc_cfg_init",
  "mpp_enc_cfg_deinit",
  "mpp_enc_cfg_set_s32",
  "mpp_enc_cfg_set_u32",
  "mpp_buffer_get_with_tag",
  "mpp_buffer_put_with_caller",
  "mpp_buffer_get_ptr_with_caller",
  "mpp_buffer_group_get",
  "mpp_buffer_group_put",
  "mpp_meta_set_packet",
  0
};

#define SYM_COUNT (sizeof(sym_names)/sizeof(sym_names[0]) - 1)

extern void *_librockchip_mpp_so_tramp_table[];

void *_librockchip_mpp_so_tramp_resolve(size_t i) {
  assert(i < SYM_COUNT);

  int publish = 1;

  void *h = 0;
#if NO_DLOPEN
  if (lib_handle) {
    h = lib_handle;
  } else {
#   ifndef IMPLIB_EXPORT_SHIMS
    h = RTLD_DEFAULT;
#   else
    h = RTLD_NEXT;
#   endif
  }
#else
  publish = load_library();
  h = lib_handle;
  CHECK(h, "failed to resolve symbol '%s', library failed to load", sym_names[i]);
#endif

  void *addr;
  addr = dlsym(h, sym_names[i]);
  CHECK(addr, "failed to resolve symbol '%s' via dlsym: %s", sym_names[i], dlerror());

  if (publish) {
    (void)__sync_val_compare_and_swap(&_librockchip_mpp_so_tramp_table[i], 0, addr);
  }

  return addr;
}

void _librockchip_mpp_so_tramp_resolve_all(void) {
  size_t i;
  for(i = 0; i < SYM_COUNT; ++i)
    _librockchip_mpp_so_tramp_resolve(i);
}

void _librockchip_mpp_so_tramp_set_handle(void *handle) {
  lib_handle = handle;
  dlopened = 0;
}

void _librockchip_mpp_so_tramp_reset(void) {
  memset(_librockchip_mpp_so_tramp_table, 0, SYM_COUNT * sizeof(_librockchip_mpp_so_tramp_table[0]));
  lib_handle = 0;
  dlopened = 0;
}

#ifdef __cplusplus
}  // extern "C"
#endif
