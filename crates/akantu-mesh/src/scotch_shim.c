/* SPDX-License-Identifier: LGPL-3.0-or-later
 * Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). C shim of the
 * `scotch` feature (crates/akantu-mesh/src/scotch.rs): the Scotch calls of
 * upstream MeshPartitionScotch::partitionate (mesh_partition_scotch.cc:
 * 51-64, 213-326): SCOTCH_randomSeed(seed), default strategy
 * (SCOTCH_stratInit), SCOTCH_graphBuild (base 0, no vendtab/vlbltab),
 * SCOTCH_graphPart. SCOTCH_randomReset is called after the seed so that one
 * process can compute several partitions as a fresh process would.
 * MUTIL-014: the dblDump files of partitionate
 * (GraphIniFile.grf / GraphFile.grf: SCOTCH_graphSave before and after
 * SCOTCH_graphPart, mesh_partition_scotch.cc:280-360) and the node
 * reordering of MeshPartitionScotch::reorder (createMesh: SCOTCH_meshBuild,
 * SCOTCH_meshCheck, SCOTCH_meshSave to ScotchMesh.msh; reorder:
 * SCOTCH_meshGraph, SCOTCH_graphSave to ScotchMesh.grf, SCOTCH_graphOrder
 * with the default strategy; mesh_partition_scotch.cc:66-200, 378-445). */
#include <stdint.h>
#include <stdio.h>
#include <scotch.h>

int akantu_scotch_num_size(void) { return (int)sizeof(SCOTCH_Num); }

void akantu_scotch_version(int * version, int * release, int * patch) {
  SCOTCH_version(version, release, patch);
}

/* SCOTCH_graphSave / SCOTCH_meshSave to a file (NULL path: nothing). */
static int save_graph(const SCOTCH_Graph * graph, const char * path) {
  FILE * f;
  int rc;
  if (path == NULL) {
    return 0;
  }
  f = fopen(path, "w");
  if (f == NULL) {
    return -1;
  }
  rc = SCOTCH_graphSave(graph, f);
  if (fclose(f) != 0 && rc == 0) {
    rc = -1;
  }
  return rc;
}

static int save_mesh(const SCOTCH_Mesh * mesh, const char * path) {
  FILE * f;
  int rc;
  if (path == NULL) {
    return 0;
  }
  f = fopen(path, "w");
  if (f == NULL) {
    return -1;
  }
  rc = SCOTCH_meshSave(mesh, f);
  if (fclose(f) != 0 && rc == 0) {
    rc = -1;
  }
  return rc;
}

int akantu_scotch_graph_part(SCOTCH_Num vertnbr, const SCOTCH_Num * verttab,
                             const SCOTCH_Num * velotab, SCOTCH_Num edgenbr,
                             const SCOTCH_Num * edgetab, const SCOTCH_Num * edlotab,
                             SCOTCH_Num partnbr, SCOTCH_Num seed, SCOTCH_Num * parttab,
                             const char * graph_ini_file, const char * graph_file) {
  SCOTCH_Graph graph;
  SCOTCH_Strat strat;
  int rc;
  SCOTCH_randomSeed(seed);
  SCOTCH_randomReset();
  if ((rc = SCOTCH_stratInit(&strat)) != 0) {
    return rc;
  }
  if ((rc = SCOTCH_graphInit(&graph)) != 0) {
    SCOTCH_stratExit(&strat);
    return rc;
  }
  /* Scotch does not modify the arrays it is given (const casts only). */
  rc = SCOTCH_graphBuild(&graph, 0, vertnbr, (SCOTCH_Num *)verttab, NULL,
                         (SCOTCH_Num *)velotab, NULL, edgenbr,
                         (SCOTCH_Num *)edgetab, (SCOTCH_Num *)edlotab);
  if (rc == 0) {
    rc = save_graph(&graph, graph_ini_file);
  }
  if (rc == 0) {
    rc = SCOTCH_graphCheck(&graph);
  }
  if (rc == 0) {
    rc = SCOTCH_graphPart(&graph, partnbr, &strat, parttab);
  }
  if (rc == 0) {
    rc = save_graph(&graph, graph_file);
  }
  SCOTCH_graphExit(&graph);
  SCOTCH_stratExit(&strat);
  return rc;
}

/* MeshPartitionScotch::reorder: node ordering of the element-node mesh
 * (verttab: vnodnbr + velmnbr + 1 compact start indices, vendtab =
 * verttab + 1, no loads or labels). permtab receives vnodnbr entries. */
int akantu_scotch_mesh_order(SCOTCH_Num velmbas, SCOTCH_Num vnodbas, SCOTCH_Num velmnbr,
                             SCOTCH_Num vnodnbr, const SCOTCH_Num * verttab,
                             SCOTCH_Num edgenbr, const SCOTCH_Num * edgetab,
                             SCOTCH_Num seed, SCOTCH_Num * permtab, const char * mesh_file,
                             const char * graph_file) {
  SCOTCH_Mesh mesh;
  SCOTCH_Graph graph;
  SCOTCH_Strat strat;
  SCOTCH_Num cblknbr = 0;
  int rc;
  SCOTCH_randomSeed(seed);
  SCOTCH_randomReset();
  if ((rc = SCOTCH_meshInit(&mesh)) != 0) {
    return rc;
  }
  /* Scotch does not modify the arrays it is given (const casts only). */
  rc = SCOTCH_meshBuild(&mesh, velmbas, vnodbas, velmnbr, vnodnbr, (SCOTCH_Num *)verttab,
                        (SCOTCH_Num *)verttab + 1, NULL, NULL, NULL, edgenbr,
                        (SCOTCH_Num *)edgetab);
  if (rc == 0) {
    rc = SCOTCH_meshCheck(&mesh);
  }
  if (rc == 0) {
    rc = save_mesh(&mesh, mesh_file);
  }
  if (rc == 0) {
    if ((rc = SCOTCH_stratInit(&strat)) != 0) {
      SCOTCH_meshExit(&mesh);
      return rc;
    }
    if ((rc = SCOTCH_graphInit(&graph)) != 0) {
      SCOTCH_stratExit(&strat);
      SCOTCH_meshExit(&mesh);
      return rc;
    }
    rc = SCOTCH_meshGraph(&mesh, &graph);
    if (rc == 0) {
      rc = save_graph(&graph, graph_file);
    }
    if (rc == 0) {
      rc = SCOTCH_graphOrder(&graph, &strat, permtab, NULL, &cblknbr, NULL, NULL);
    }
    SCOTCH_graphExit(&graph);
    SCOTCH_stratExit(&strat);
  }
  SCOTCH_meshExit(&mesh);
  return rc;
}
