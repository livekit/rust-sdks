# retrieve webrtc JNI symbols from a lib
# Usage: generate_jni_symbols.sh <path_to_lib> jni_symbols.txt

lib=$1

llvm-readelf -s $lib \
| grep Java_org_webrtc \
| awk '{print $8}' \
| sort \
| uniq \
| sed 's/@@JNI_WEBRTC//' \
> jni_symbols.txt

