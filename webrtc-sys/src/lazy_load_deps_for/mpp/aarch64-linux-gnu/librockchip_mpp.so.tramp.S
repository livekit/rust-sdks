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

  .globl _librockchip_mpp_so_tramp_table
  .hidden _librockchip_mpp_so_tramp_table
  .align 8
_librockchip_mpp_so_tramp_table:
  .zero 232

  .text

  .globl _librockchip_mpp_so_tramp_resolve
  .hidden _librockchip_mpp_so_tramp_resolve

  .globl _librockchip_mpp_so_save_regs_and_resolve
  .hidden _librockchip_mpp_so_save_regs_and_resolve
  .type _librockchip_mpp_so_save_regs_and_resolve, %function
_librockchip_mpp_so_save_regs_and_resolve:
  .cfi_startproc

#define PUSH_PAIR(reg1, reg2) stp reg1, reg2, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset reg1, 0; .cfi_rel_offset reg2, 8
#define POP_PAIR(reg1, reg2) ldp reg1, reg2, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore reg2; .cfi_restore reg1

#define PUSH_WIDE_PAIR(reg1, reg2) stp reg1, reg2, [sp, #-32]!; .cfi_adjust_cfa_offset 32; .cfi_rel_offset reg1, 0; .cfi_rel_offset reg2, 16
#define POP_WIDE_PAIR(reg1, reg2) ldp reg1, reg2, [sp], #32; .cfi_adjust_cfa_offset -32; .cfi_restore reg2; .cfi_restore reg1

  PUSH_PAIR(x0, x1)
  PUSH_PAIR(x2, x3)
  PUSH_PAIR(x4, x5)
  PUSH_PAIR(x6, x7)
  PUSH_PAIR(x8, lr)

  ldr x0, [sp, #80]

  PUSH_WIDE_PAIR(q0, q1)
  PUSH_WIDE_PAIR(q2, q3)
  PUSH_WIDE_PAIR(q4, q5)
  PUSH_WIDE_PAIR(q6, q7)

  bl _librockchip_mpp_so_tramp_resolve
  mov ip0, x0

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

// Trampoline macro for each symbol
// INDEX: symbol index in tramp_table (0-based)
// OFFSET: INDEX * 8 (byte offset into pointer table)
.macro MPP_TRAMP name, index, offset
  .globl \name
  .p2align 4
  .type \name, %function
#ifndef IMPLIB_EXPORT_SHIMS
  .hidden \name
#endif
\name:
  .cfi_startproc
1:
  adrp ip0, _librockchip_mpp_so_tramp_table+\offset
  ldr ip0, [ip0, #:lo12:_librockchip_mpp_so_tramp_table+\offset]
  cbz ip0, 2f
  br ip0
2:
  mov ip0, \index & 0xffff
  stp ip0, lr, [sp, #-16]!; .cfi_adjust_cfa_offset 16; .cfi_rel_offset lr, 8
  bl _librockchip_mpp_so_save_regs_and_resolve
  ldp xzr, lr, [sp], #16; .cfi_adjust_cfa_offset -16; .cfi_restore lr
  br ip0
  .cfi_endproc
.endm

// Symbol trampolines (must match sym_names[] order in .init.c)
MPP_TRAMP mpp_create,                    0,   0
MPP_TRAMP mpp_init,                      1,   8
MPP_TRAMP mpp_destroy,                   2,  16
MPP_TRAMP mpp_check_support_format,      3,  24
MPP_TRAMP mpp_frame_init,                4,  32
MPP_TRAMP mpp_frame_deinit,              5,  40
MPP_TRAMP mpp_frame_set_width,           6,  48
MPP_TRAMP mpp_frame_set_height,          7,  56
MPP_TRAMP mpp_frame_set_hor_stride,      8,  64
MPP_TRAMP mpp_frame_set_ver_stride,      9,  72
MPP_TRAMP mpp_frame_set_fmt,            10,  80
MPP_TRAMP mpp_frame_set_buffer,         11,  88
MPP_TRAMP mpp_frame_set_eos,            12,  96
MPP_TRAMP mpp_frame_get_meta,           13, 104
MPP_TRAMP mpp_packet_init_with_buffer,  14, 112
MPP_TRAMP mpp_packet_deinit,            15, 120
MPP_TRAMP mpp_packet_set_length,        16, 128
MPP_TRAMP mpp_packet_get_pos,           17, 136
MPP_TRAMP mpp_packet_get_length,        18, 144
MPP_TRAMP mpp_enc_cfg_init,             19, 152
MPP_TRAMP mpp_enc_cfg_deinit,           20, 160
MPP_TRAMP mpp_enc_cfg_set_s32,          21, 168
MPP_TRAMP mpp_enc_cfg_set_u32,          22, 176
MPP_TRAMP mpp_buffer_get_with_tag,      23, 184
MPP_TRAMP mpp_buffer_put_with_caller,   24, 192
MPP_TRAMP mpp_buffer_get_ptr_with_caller, 25, 200
MPP_TRAMP mpp_buffer_group_get,         26, 208
MPP_TRAMP mpp_buffer_group_put,         27, 216
MPP_TRAMP mpp_meta_set_packet,          28, 224
