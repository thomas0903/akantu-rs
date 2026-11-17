| system | n | COLAMD predicted work | COLAMD | ND / COLAMD | auto / COLAMD | auto chose | fallback |
|---|---|---|---|---|---|---|---|
| strip 4 x 600, c 1 | 2400 | – | 0.0015 s | 4.413 | 1.003 | colamd | false |
| strip 6 x 400, c 2 | 4800 | – | 0.0062 s | 2.548 | 0.997 | colamd | false |
| square 30 x 30, c 3 | 2700 | – | 0.0138 s | 0.856 | 0.994 | colamd | false |
| square 50 x 50, c 1 | 2500 | – | 0.0091 s | 1.627 | 0.999 | colamd | false |
| square 50 x 50, c 3 | 7500 | 1.02e8 | 0.0688 s | 0.610 | 0.827 | nested_dissection | false |
| indefinite 16^3 | 4096 | – | 0.1164 s | 2.302 | 1.003 | colamd | false |
| indefinite 20^3 | 8000 | – | 0.4725 s | 2.340 | 3.419 | colamd | true |
| square 70 x 70, c 1 | 4900 | – | 0.0200 s | 1.400 | 1.003 | colamd | false |
| square 100 x 100, c 1 | 10000 | – | 0.0461 s | 1.435 | 0.999 | colamd | false |
| square 140 x 140, c 1 | 19600 | 1.33e8 | 0.0991 s | 1.345 | 1.042 | colamd | false |
| square 200 x 200, c 1 | 40000 | 4.22e8 | 0.2625 s | 1.168 | 1.000 | colamd | false |
| square 70 x 70, c 2 | 9800 | 1.54e8 | 0.0946 s | 0.527 | 0.699 | nested_dissection | false |
| square 100 x 100, c 2 | 20000 | 4.32e8 | 0.2358 s | 0.506 | 0.654 | nested_dissection | false |
| square 70 x 70, c 3 | 14700 | 3.22e8 | 0.1762 s | 0.510 | 0.678 | nested_dissection | false |
| square 100 x 100, c 3 | 30000 | 1.93e9 | 0.7462 s | 0.289 | 0.378 | nested_dissection | false |
| cube 12^3, c 1 | 1728 | – | 0.0265 s | 0.832 | 1.000 | colamd | false |
| cube 15^3, c 1 | 3375 | – | 0.0618 s | 0.787 | 1.001 | colamd | false |
| cube 18^3, c 1 | 5832 | 6.65e8 | 0.1740 s | 0.546 | 0.785 | nested_dissection | false |
| cube 22^3, c 1 | 10648 | 3.92e9 | 0.8520 s | 0.276 | 0.387 | nested_dissection | false |
| cube 26^3, c 1 | 17576 | 2.11e10 | 4.5780 s | 0.110 | 0.144 | nested_dissection | false |
| cube 8^3, c 3 | 1536 | – | 0.0354 s | 0.447 | 1.001 | colamd | false |
| cube 10^3, c 3 | 3000 | 3.23e8 | 0.1086 s | 0.398 | 0.575 | nested_dissection | false |
| cube 12^3, c 3 | 5184 | 1.20e9 | 0.2470 s | 0.425 | 0.561 | nested_dissection | false |
| cube 14^3, c 3 | 8232 | 1.74e9 | 0.4286 s | 0.544 | 0.684 | nested_dissection | false |
