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

  .globl _libnvbufsurftransform_so_tramp_table
  .hidden _libnvbufsurftransform_so_tramp_table
  .align 8
_libnvbufsurftransform_so_tramp_table:
  .zero 112

  .text

  .globl _libnvbufsurftransform_so_tramp_resolve
  .hidden _libnvbufsurftransform_so_tramp_resolve

  .globl _libnvbufsurftransform_so_save_regs_and_resolve
  .hidden _libnvbufsurftransform_so_save_regs_and_resolve
  .type _libnvbufsurftransform_so_save_regs_and_resolve, %function
_libnvbufsurftransform_so_save_regs_and_resolve:
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

  bl _libnvbufsurftransform_so_tramp_resolve
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

  .globl NvBufSurfTransform
  .p2align 4
  .type NvBufSurfTransform, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransform
#endif
NvBufSurfTransform:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+0
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+0]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
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

  .globl NvBufSurfTransformAsync
  .p2align 4
  .type NvBufSurfTransformAsync, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransformAsync
#endif
NvBufSurfTransformAsync:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+8
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+8]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
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

  .globl NvBufSurfTransformComposite
  .p2align 4
  .type NvBufSurfTransformComposite, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransformComposite
#endif
NvBufSurfTransformComposite:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+16
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+16]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
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

  .globl NvBufSurfTransformCompositeAsync
  .p2align 4
  .type NvBufSurfTransformCompositeAsync, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransformCompositeAsync
#endif
NvBufSurfTransformCompositeAsync:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+24
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+24]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
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

  .globl NvBufSurfTransformCompositeBlend
  .p2align 4
  .type NvBufSurfTransformCompositeBlend, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransformCompositeBlend
#endif
NvBufSurfTransformCompositeBlend:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+32
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+32]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
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

  .globl NvBufSurfTransformGetSessionParams
  .p2align 4
  .type NvBufSurfTransformGetSessionParams, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransformGetSessionParams
#endif
NvBufSurfTransformGetSessionParams:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+40
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+40]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
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

  .globl NvBufSurfTransformMultiInputBufCompositeBlend
  .p2align 4
  .type NvBufSurfTransformMultiInputBufCompositeBlend, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransformMultiInputBufCompositeBlend
#endif
NvBufSurfTransformMultiInputBufCompositeBlend:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+48
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+48]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
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

  .globl NvBufSurfTransformMultiInputBufCompositeBlendAsync
  .p2align 4
  .type NvBufSurfTransformMultiInputBufCompositeBlendAsync, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransformMultiInputBufCompositeBlendAsync
#endif
NvBufSurfTransformMultiInputBufCompositeBlendAsync:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+56
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+56]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
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

  .globl NvBufSurfTransformSetDefaultSession
  .p2align 4
  .type NvBufSurfTransformSetDefaultSession, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransformSetDefaultSession
#endif
NvBufSurfTransformSetDefaultSession:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+64
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+64]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
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

  .globl NvBufSurfTransformSetSessionParams
  .p2align 4
  .type NvBufSurfTransformSetSessionParams, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransformSetSessionParams
#endif
NvBufSurfTransformSetSessionParams:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+72
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+72]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
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

  .globl NvBufSurfTransformSyncObjDestroy
  .p2align 4
  .type NvBufSurfTransformSyncObjDestroy, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransformSyncObjDestroy
#endif
NvBufSurfTransformSyncObjDestroy:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+80
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+80]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
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

  .globl NvBufSurfTransformSyncObjFromFile
  .p2align 4
  .type NvBufSurfTransformSyncObjFromFile, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransformSyncObjFromFile
#endif
NvBufSurfTransformSyncObjFromFile:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+88
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+88]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
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

  .globl NvBufSurfTransformSyncObjWait
  .p2align 4
  .type NvBufSurfTransformSyncObjWait, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden NvBufSurfTransformSyncObjWait
#endif
NvBufSurfTransformSyncObjWait:
  .cfi_startproc

1:
  // Load address
  // TODO: can we do this faster on newer ARMs?
  adrp ip0, _libnvbufsurftransform_so_tramp_table+96
  ldr ip0, [ip0, #:lo12:_libnvbufsurftransform_so_tramp_table+96]
 
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
  bl _libnvbufsurftransform_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc

