Cl:
- binder: Use bitmap for faster descriptor lookup
- binder: Fix descriptor lookup for context manager
- mm/mremap: Always take rmap locks in the optimized HPAGE_PMD move
- lib/lzo: Add lzo-rle (run-length encoding) support
- lib/lzo: Separate lzo-rle from lzo
- lib/lzo: Fast 8-byte copy on arm64
- lib/lzo: 64-bit CTZ on arm64
- lib/lzo: Fix alignment bug in lzo-rle
- lib/lzo: Fix ambiguous encoding bug in lzo-rle
- lib/lzo: Fix bugs for very short or empty input
- lib/lzo: Tidy-up ifdefs
- arch: arm64: configs: Enable LZO compression (F2FS LZO + LZO-RLE)

Sumber: ~/android_kernel_samsung_sm8250 (LineageOS, lineage-23.2, 4.19.325)
Diport manual (cherry-pick konflik semua karena layout kode berbeda).
