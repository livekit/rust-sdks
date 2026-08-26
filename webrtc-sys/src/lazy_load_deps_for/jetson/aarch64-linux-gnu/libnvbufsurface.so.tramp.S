/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

#define lr x30
#define ip0 x16

  .section .note.GNU-stack,"",@progbits

  .data

  .globl _libnvbufsurface_so_tramp_table
  .hidden _libnvbufsurface_so_tramp_table
  .align 8
_libnvbufsurface_so_tramp_table:
  .zero 136

  .text

  .globl _libnvbufsurface_so_tramp_resolve
  .hidden _libnvbufsurface_so_tramp_resolve

  .globl _libnvbufsurface_so_save_regs_and_resolve
  .hidden _libnvbufsurface_so_save_regs_and_resolve
  .type _libnvbufsurface_so_save_regs_and_resolve, %function
_libnvbufsurface_so_save_regs_and_resolve:
  .cfi_startproc

  // Slow path which calls dlsym, taken only on first call.
  // Registers are saved according to "Procedure Call Standard for the Arm® 64-bit Architecture".
  // For DWARF directives, read https://www.imperialviolet.org/2017/01/18/cfi.html.

  // Stack is aligned at 16 bytes

#define PUSH_PAIR(reg1, reg2) stp reg1, reg2, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset reg1, 0; .cfi_rel_offset reg2, 8
#define POP_PAIR(reg1, reg2) ldp reg1, reg2, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore reg2; .cfi_restore reg1

#define PUSH_WIDE_PAIR(reg1, reg2) stp reg1, reg2, [sp, #-32]!; .cfi_adjust_cfa_offset 32; .cfi_rel_offset reg1, 0; .cfi_rel_offset reg2, 16
#define POP_WIDE_PAIR(reg1, reg2) ldp reg1, reg2, [sp], #32; .cfi_adjust_cfa_offset -32; .cfi_restore reg2; .cfi_restore reg1

  // Save only arguments (and lr)
  PUSH_PAIR(x0, x1)
  PUSH_PAIR(x2, x3)
  PUSH_PAIR(x4, x5)
  PUSH_PAIR(x6, x7)
  PUSH_PAIR(x8, lr)

  ldr x0, [sp, #80]  // 16*5

  PUSH_WIDE_PAIR(q0, q1)
  PUSH_WIDE_PAIR(q2, q3)
  PUSH_WIDE_PAIR(q4, q5)
  PUSH_WIDE_PAIR(q6, q7)

  // Stack is aligned at 16 bytes

  bl _libnvbufsurface_so_tramp_resolve
  mov ip0, x0

  // TODO: pop pc?

  POP_WIDE_PAIR(q6, q7)
  POP_WIDE_PAIR(q4, q5)
  POP_WIDE_PAIR(q2, q3)
  POP_WIDE_PAIR(q0, q1)

  POP_PAIR(x8, lr)
  POP_PAIR(x6, x7)
  POP_PAIR(x4, x5)
  POP_PAIR(x2, x3)
  POP_PAIR(x0, x1)

  br lr

  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurface2Raw
  .p2align 4
  .type NvBufSurface2Raw, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurface2Raw
#endif
NvBufSurface2Raw:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+0
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+0]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 0 & 0xffff
#if 0 > 0xffff
  movk ip0, 0 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceAllocate
  .p2align 4
  .type NvBufSurfaceAllocate, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceAllocate
#endif
NvBufSurfaceAllocate:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+8
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+8]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 1 & 0xffff
#if 1 > 0xffff
  movk ip0, 1 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceCopy
  .p2align 4
  .type NvBufSurfaceCopy, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceCopy
#endif
NvBufSurfaceCopy:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+16
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+16]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 2 & 0xffff
#if 2 > 0xffff
  movk ip0, 2 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceCreate
  .p2align 4
  .type NvBufSurfaceCreate, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceCreate
#endif
NvBufSurfaceCreate:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+24
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+24]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 3 & 0xffff
#if 3 > 0xffff
  movk ip0, 3 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceDestroy
  .p2align 4
  .type NvBufSurfaceDestroy, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceDestroy
#endif
NvBufSurfaceDestroy:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+32
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+32]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 4 & 0xffff
#if 4 > 0xffff
  movk ip0, 4 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceFromFd
  .p2align 4
  .type NvBufSurfaceFromFd, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceFromFd
#endif
NvBufSurfaceFromFd:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+40
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+40]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 5 & 0xffff
#if 5 > 0xffff
  movk ip0, 5 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceGetMapParams
  .p2align 4
  .type NvBufSurfaceGetMapParams, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceGetMapParams
#endif
NvBufSurfaceGetMapParams:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+48
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+48]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 6 & 0xffff
#if 6 > 0xffff
  movk ip0, 6 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceImport
  .p2align 4
  .type NvBufSurfaceImport, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceImport
#endif
NvBufSurfaceImport:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+56
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+56]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 7 & 0xffff
#if 7 > 0xffff
  movk ip0, 7 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceMap
  .p2align 4
  .type NvBufSurfaceMap, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceMap
#endif
NvBufSurfaceMap:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+64
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+64]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 8 & 0xffff
#if 8 > 0xffff
  movk ip0, 8 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceMapEglImage
  .p2align 4
  .type NvBufSurfaceMapEglImage, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceMapEglImage
#endif
NvBufSurfaceMapEglImage:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+72
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+72]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 9 & 0xffff
#if 9 > 0xffff
  movk ip0, 9 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceMemSet
  .p2align 4
  .type NvBufSurfaceMemSet, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceMemSet
#endif
NvBufSurfaceMemSet:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+80
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+80]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 10 & 0xffff
#if 10 > 0xffff
  movk ip0, 10 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceSyncForCpu
  .p2align 4
  .type NvBufSurfaceSyncForCpu, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceSyncForCpu
#endif
NvBufSurfaceSyncForCpu:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+88
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+88]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 11 & 0xffff
#if 11 > 0xffff
  movk ip0, 11 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceSyncForDevice
  .p2align 4
  .type NvBufSurfaceSyncForDevice, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceSyncForDevice
#endif
NvBufSurfaceSyncForDevice:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+96
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+96]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 12 & 0xffff
#if 12 > 0xffff
  movk ip0, 12 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceUnMap
  .p2align 4
  .type NvBufSurfaceUnMap, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceUnMap
#endif
NvBufSurfaceUnMap:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+104
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+104]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 13 & 0xffff
#if 13 > 0xffff
  movk ip0, 13 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl NvBufSurfaceUnMapEglImage
  .p2align 4
  .type NvBufSurfaceUnMapEglImage, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfaceUnMapEglImage
#endif
NvBufSurfaceUnMapEglImage:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+112
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+112]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 14 & 0xffff
#if 14 > 0xffff
  movk ip0, 14 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

/*
 * Copyright 2018-2025 Yury Gribov
 *
 * The MIT License (MIT)
 *
 * Use of this source code is governed by MIT license that can be
 * found in the LICENSE.txt file.
 */

  .globl Raw2NvBufSurface
  .p2align 4
  .type Raw2NvBufSurface, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden Raw2NvBufSurface
#endif
Raw2NvBufSurface:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurface_so_tramp_table+120
  ldr ip0, [ip0, #:lo12:_libnvbufsurface_so_tramp_table+120]
 
  cbz ip0, 2f

  // Fast path
  br ip0

2:
  // Slow path
  mov ip0, 15 & 0xffff
#if 15 > 0xffff
  movk ip0, 15 >> 16, lsl #16
#endif
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _libnvbufsurface_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

