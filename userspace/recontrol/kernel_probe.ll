; Recontrol Lang LLVM IR
source_filename = "recontrol"

declare void @rcl_println(ptr)
declare void @rcl_print(ptr)
declare void @rcl_print_i8(i8)
declare void @rcl_println_i8(i8)
declare void @rcl_print_i32(i32)
declare void @rcl_println_i32(i32)
declare void @rcl_check_bounds_i32(i32, i32)
declare void @rcl_check_divisor(i32)
declare void @rcl_check_div_overflow(i32)


define i32 @rcl_generic_probe() {
entry:
  ret i32 128
}

