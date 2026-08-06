# Vkit tract-tflite patch

Upstream package: `tract-tflite` 0.23.4  
Upstream repository: <https://github.com/sonos/tract>  
Upstream licenses: MIT OR Apache-2.0 (both texts are retained beside this file)

This vendored package differs from the published crate in one source file:

- `src/ops/element_wise.rs` registers the TFLite `DEQUANTIZE` importer as a
  cast to the declared output datum type.
- The same file registers `PRELU` as the broadcast expression
  `max(x, 0) + alpha * min(x, 0)`.

The change is required by the official MediaPipe Face Landmarker V2 detector
and landmark models.  No public registry-extension API exists in tract-tflite
0.23.4 or tract git commit `546124dd11bb4c05008e2f7ea66bffcd55c58712`.

Modified for Vkit on 2026-07-17.
