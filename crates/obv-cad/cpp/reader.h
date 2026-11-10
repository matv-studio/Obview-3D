// Lecteur STEP / IGES d'Obview 3D sur OpenCASCADE (XCAF) : assemblages, noms, couleurs, maillage (D-27).
#pragma once

#include "rust/cxx.h"

namespace obv {
struct CadOptions;
struct CadResult;
struct CadProgress;

CadResult read_cad(rust::Str path, bool iges, const CadOptions& options, const CadProgress& progress);
bool write_test_step(rust::Str path);
} // namespace obv
