| variant | clean build [s] per round | / base (per round) | explicit_step_timing: file / .text [MB] | cohesive_extrinsic: file / .text [MB] | facet_scaling .text [MB] | load at start |
|---|---|---|---|---|---|---|
| base | 89, 92 | 1.00, 1.00 | 75.0 / 6.92 | 87.4 / 8.80 | 0.91 | 3.52, 3.71 |
| cgu1 | 119, 121 | 1.33, 1.31 | 62.9 / 6.25 | 72.9 / 7.92 | 0.80 | 2.78, 2.60 |
| thin | 121, 121 | 1.35, 1.30 | 59.6 / 7.18 | 76.8 / 9.12 | 0.91 | 4.30, 3.93 |
| fat | 220, 215 | 2.47, 2.33 | 49.1 / 6.60 | 62.2 / 8.24 | 0.79 | 2.13, 2.23 |
| thin+cgu1 | 153, 157 | 1.71, 1.70 | 50.3 / 6.55 | 64.4 / 8.29 | 0.82 | 3.34, 3.01 |
| fat+cgu1 | 220, 210 | 2.47, 2.27 | 46.7 / 6.20 | 59.5 / 7.80 | 0.77 | 2.23, 1.34 |
{"variant":"thin","build_s":106.5,"rustflags":"","lto":"thin","cgu":"","panic":"","load":"5.61 6.06 3.62","utc":"2026-11-16T06:25:42Z","commit":"72ebd0629","explicit_bytes":59563400,"explicit_text":7181254,"cohesive_bytes":76826264,"cohesive_text":9117958,"facet_bytes":9314088,"facet_text":914336}
