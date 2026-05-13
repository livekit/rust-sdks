---
webrtc-sys: patch
---

fix: fix LICENSE.md generation in webrtc build scripts

- Add fix_license_json_parsing.patch to handle GN warnings in JSON output
- Enable add_licenses.patch for iOS and Android builds (was commented out)
- Restore LICENSE.md copy in iOS build script (regression from #1053)

The license generation script was failing because `gn desc --format=json`
outputs warnings before the JSON when certain build args trigger deprecation
notices. The new patch strips non-JSON content before parsing.
