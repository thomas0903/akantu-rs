| class | route | general (ns) | slice kernel (ns) | fast mode (ns) | fast / slice |
|---|---|---|---|---|---|
| BtD 30x6x6 (tet10 stiffness) | Gebp { kc: 6 } | 1384.8 | 301.0 | 386.1 | 1.28 |
| BtDB 30x6x30 | Gebp { kc: 6 } | 7178.1 | 1330.4 | 2085.9 | 1.57 |
| NtN 30x3x30 (tet10 mass) | Gebp { kc: 3 } | 3761.7 | 748.3 | 1334.1 | 1.78 |
| BtDB 18x3x18 (DKT18) | Gebp { kc: 3 } | 1312.2 | 339.8 | 510.8 | 1.50 |
| J 3x10x3 (tet10 Jacobian, tree) | Coeff(Tree) | 220.9 | 40.4 | 54.6 | 1.35 |
| C eps 6x6x1 (anisotropic) | Coeff(Sequential) | 46.3 | 18.7 | 68.2 | 3.64 |
| D grad 3x3x1 (heat) | Coeff(Sequential) | 15.9 | 11.5 | 12.1 | 1.06 |
