### h012: cpp-strict vs rust
| material | h | rust preload | regime | tets cpp-strict / rust | reps | cpp-strict solve [ms/step] | rust solve [ms/step] | solve ratio | cpp-strict check [ms/step] | rust check [ms/step] | check ratio | cpp-strict core-µs/el/step | rust core-µs/el/step | cpp-strict init / static [s] | rust init / static [s] | peak RSS cpp-strict / rust [MiB] | coh. el. cpp-strict / rust |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cohesive | 0.12 | yes | same | 23,156 / 23,156 | 5/5 | 7.76 [6.13, 15.6] | 24.5 [17.6, 29.3] | 3.16 | 31.3 [21.5, 71.5] | 18.9 [10, 26.7] | 0.60 | 1.69 | 1.88 | 0.835 / 1.15 | 0.341 / 11.8 | 271 / 464 | 0 / 0 |
| elastic | 0.12 | yes | same | 23,156 / 23,156 | 5/5 | 8.27 [6.36, 12.1] | 23.2 [20, 29.7] | 2.80 | — | — | nan | 0.357 | 1 | 0.826 / 1.04 | 0.328 / 9.53 | 271 / 464 | 0 / 0 |
### h012: cpp-default vs rust
| material | h | rust preload | regime | tets cpp-default / rust | reps | cpp-default solve [ms/step] | rust solve [ms/step] | solve ratio | cpp-default check [ms/step] | rust check [ms/step] | check ratio | cpp-default core-µs/el/step | rust core-µs/el/step | cpp-default init / static [s] | rust init / static [s] | peak RSS cpp-default / rust [MiB] | coh. el. cpp-default / rust |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cohesive | 0.12 | yes | same | 23,156 / 23,156 | 5/5 | 10.1 [6.76, 12.5] | 24.5 [17.6, 29.3] | 2.42 | 36.5 [17.3, 50.7] | 18.9 [10, 26.7] | 0.52 | 2.01 | 1.88 | 0.934 / 1.21 | 0.341 / 11.8 | 272 / 464 | 0 / 0 |
| elastic | 0.12 | yes | same | 23,156 / 23,156 | 5/5 | 7.97 [7.64, 14.1] | 23.2 [20, 29.7] | 2.91 | — | — | nan | 0.344 | 1 | 1.02 / 1.13 | 0.328 / 9.53 | 272 / 464 | 0 / 0 |
### h012: cpp-strict vs cpp-default
| material | h | default preload | regime | tets strict / default | reps | strict solve [ms/step] | default solve [ms/step] | solve ratio | strict check [ms/step] | default check [ms/step] | check ratio | strict core-µs/el/step | default core-µs/el/step | strict init / static [s] | default init / static [s] | peak RSS strict / default [MiB] | coh. el. strict / default |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cohesive | 0.12 | yes | same | 23,156 / 23,156 | 5/5 | 7.76 [6.13, 15.6] | 10.1 [6.76, 12.5] | 1.31 | 31.3 [21.5, 71.5] | 36.5 [17.3, 50.7] | 1.17 | 1.69 | 2.01 | 0.835 / 1.15 | 0.934 / 1.21 | 271 / 272 | 0 / 0 |
| elastic | 0.12 | yes | same | 23,156 / 23,156 | 5/5 | 8.27 [6.36, 12.1] | 7.97 [7.64, 14.1] | 0.96 | — | — | nan | 0.357 | 0.344 | 0.826 / 1.04 | 1.02 / 1.13 | 271 / 272 | 0 / 0 |
### h006: cpp-strict vs rust-no-preload
| material | h | rust-no-preload preload | regime | tets cpp-strict / rust-no-preload | reps | cpp-strict solve [ms/step] | rust-no-preload solve [ms/step] | solve ratio | cpp-strict check [ms/step] | rust-no-preload check [ms/step] | check ratio | cpp-strict core-µs/el/step | rust-no-preload core-µs/el/step | cpp-strict init / static [s] | rust-no-preload init / static [s] | peak RSS cpp-strict / rust-no-preload [MiB] | coh. el. cpp-strict / rust-no-preload |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cohesive | 0.06 | **no** (upper bound, not like for like) | **differs** | 177,328 / 177,328 | 3/3 | 60.2 [59.7, 61] | 189 [168, 201] | 3.14 | 304 [297, 328] | 159 [132, 169] | 0.52 | 2.06 | 1.96 | 13.4 / 13.6 | 2.5 / skipped | 2163 / 810 | 0 / 162 |
| elastic | 0.06 | **no** (upper bound, not like for like) | same | 177,328 / 177,328 | 3/3 | 72.7 [64, 85.5] | 190 [181, 201] | 2.62 | — | — | nan | 0.41 | 1.07 | 15.4 / 16.5 | 2.98 / skipped | 2163 / 642 | 0 / 0 |
### h006: cpp-default vs rust-no-preload
| material | h | rust-no-preload preload | regime | tets cpp-default / rust-no-preload | reps | cpp-default solve [ms/step] | rust-no-preload solve [ms/step] | solve ratio | cpp-default check [ms/step] | rust-no-preload check [ms/step] | check ratio | cpp-default core-µs/el/step | rust-no-preload core-µs/el/step | cpp-default init / static [s] | rust-no-preload init / static [s] | peak RSS cpp-default / rust-no-preload [MiB] | coh. el. cpp-default / rust-no-preload |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cohesive | 0.06 | **no** (upper bound, not like for like) | **differs** | 177,328 / 177,328 | 3/3 | 63.5 [60.8, 79.5] | 189 [168, 201] | 2.98 | 277 [267, 395] | 159 [132, 169] | 0.57 | 1.92 | 1.96 | 13.4 / 16.3 | 2.5 / skipped | 2164 / 810 | 0 / 162 |
| elastic | 0.06 | **no** (upper bound, not like for like) | same | 177,328 / 177,328 | 3/3 | 89.4 [72.9, 90.2] | 190 [181, 201] | 2.13 | — | — | nan | 0.504 | 1.07 | 14.8 / 17.4 | 2.98 / skipped | 2164 / 642 | 0 / 0 |
### h006: cpp-strict vs cpp-default
| material | h | default preload | regime | tets strict / default | reps | strict solve [ms/step] | default solve [ms/step] | solve ratio | strict check [ms/step] | default check [ms/step] | check ratio | strict core-µs/el/step | default core-µs/el/step | strict init / static [s] | default init / static [s] | peak RSS strict / default [MiB] | coh. el. strict / default |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cohesive | 0.06 | yes | same | 177,328 / 177,328 | 3/3 | 60.2 [59.7, 61] | 63.5 [60.8, 79.5] | 1.06 | 304 [297, 328] | 277 [267, 395] | 0.91 | 2.06 | 1.92 | 13.4 / 13.6 | 13.4 / 16.3 | 2163 / 2164 | 0 / 0 |
| elastic | 0.06 | yes | same | 177,328 / 177,328 | 3/3 | 72.7 [64, 85.5] | 89.4 [72.9, 90.2] | 1.23 | — | — | nan | 0.41 | 0.504 | 15.4 / 16.5 | 14.8 / 17.4 | 2163 / 2164 | 0 / 0 |
### h004: cpp-strict vs rust-no-preload
| material | h | rust-no-preload preload | regime | tets cpp-strict / rust-no-preload | reps | cpp-strict solve [ms/step] | rust-no-preload solve [ms/step] | solve ratio | cpp-strict check [ms/step] | rust-no-preload check [ms/step] | check ratio | cpp-strict core-µs/el/step | rust-no-preload core-µs/el/step | cpp-strict init / static [s] | rust-no-preload init / static [s] | peak RSS cpp-strict / rust-no-preload [MiB] | coh. el. cpp-strict / rust-no-preload |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cohesive | 0.04 | **no** (upper bound, not like for like) | same | 558,875 / 558,875 | 3/3 | 186 [184, 225] | 763 [716, 860] | 4.10 | 1.09e+03 [1.05e+03, 1.38e+03] | 497 [449, 582] | 0.46 | 2.28 | 2.25 | 78.3 / 90.1 | 10.4 / skipped | 8516 / 2323 | 0 / 0 |
| elastic | 0.04 | **no** (upper bound, not like for like) | same | 558,875 / 558,875 | 3/3 | 218 [187, 261] | 819 [604, 913] | 3.76 | — | — | nan | 0.39 | 1.47 | 91 / 88.6 | 10.7 / skipped | 8515 / 1994 | 0 / 0 |
