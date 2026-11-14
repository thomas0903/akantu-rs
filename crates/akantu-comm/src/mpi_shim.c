/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* akantu-rs MPI shim (no upstream counterpart; upstream uses these handles
 * directly from C++, src/synchronizer/communicator_mpi_inline_impl.hh:
 * MPI_DOUBLE_INT for SCMinMaxLoc<double, int>, MPI_MINLOC / MPI_MAXLOC).
 *
 * Predefined MPI handles and MPI_IN_PLACE, C macros in mpi.h (not exported by the
 * rsmpi C shim), and the C layout of the MPI_DOUBLE_INT pair, for the
 * NativeMpi reduction policy of akantu-comm (decision record D-039).
 * Compiled with mpicc by crates/akantu-comm/build.rs (feature `mpi`). */
#include <mpi.h>
#include <stddef.h>

struct akantu_double_int {
  double value;
  int loc;
};

MPI_Datatype akantu_mpi_double_int(void) { return MPI_DOUBLE_INT; }
MPI_Op akantu_mpi_minloc(void) { return MPI_MINLOC; }
MPI_Op akantu_mpi_maxloc(void) { return MPI_MAXLOC; }
void *akantu_mpi_in_place(void) { return MPI_IN_PLACE; }
/* Fortran handle of a communicator for native libraries (MUMPS comm_fortran). */
int akantu_mpi_comm_c2f(MPI_Comm comm) { return (int)MPI_Comm_c2f(comm); }
size_t akantu_double_int_size(void) { return sizeof(struct akantu_double_int); }
size_t akantu_double_int_loc_offset(void) {
  return offsetof(struct akantu_double_int, loc);
}
