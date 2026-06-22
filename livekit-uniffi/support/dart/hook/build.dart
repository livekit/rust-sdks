// Native Assets build hook for the LiveKit UniFFI Dart bindings.
//
// The generated bindings call into the native library through `@Native`
// annotations bound to the asset id `package:<pkg>/uniffi:livekit_uniffi`.
// This hook registers that code asset, resolving the prebuilt dynamic library
// from the package root.
import 'dart:io';

import 'package:code_assets/code_assets.dart';
import 'package:hooks/hooks.dart';

// Matches `cdylib_name` in livekit-uniffi/uniffi.toml and the asset name baked
// into the generated Dart.
const _cdylibName = 'livekit_uniffi';

String _libFileName() {
  if (Platform.isMacOS) return 'lib$_cdylibName.dylib';
  if (Platform.isWindows) return '$_cdylibName.dll';
  return 'lib$_cdylibName.so';
}

void main(List<String> args) async {
  await build(args, (input, output) async {
    final libPath = input.packageRoot.resolve(_libFileName());

    output.assets.code.add(
      CodeAsset(
        package: input.packageName,
        // Dart prefixes this with `package:<packageName>/`.
        name: 'uniffi:$_cdylibName',
        linkMode: DynamicLoadingBundled(),
        file: libPath,
      ),
    );
  });
}
