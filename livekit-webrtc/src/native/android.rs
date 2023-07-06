pub fn initialize_android(vm: jni::JavaVM) {
    vm.get_java_vm_pointer()
}
